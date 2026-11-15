//! 源文件改名：纯逻辑规划 + 极薄的磁盘执行层。
//!
//! 约束（与 UI 约定一致）：
//! - 只改同一个目录内的文件名，不接受路径分隔符；
//! - 输入不带扩展名时沿用原扩展名，带了就按输入来；
//! - 目标已存在（且不是自身的大小写变化）直接拒绝，不做覆盖；
//! - 批量改名先整体规划，任一项冲突就一个都不执行。

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RenamePlan {
    pub from: String,
    pub to: String,
    /// 目标文件名（不含目录），供 UI 直接展示。
    pub name: String,
    /// 冲突原因；为空表示可以执行。
    pub conflict: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RenameResult {
    pub from: String,
    pub to: String,
    pub name: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BatchRenameResult {
    pub plans: Vec<RenamePlan>,
    /// true 表示已经落盘；false 表示只做了预检（dryRun）或存在冲突。
    pub applied: bool,
}

/// 把用户输入解析成同目录下的目标路径。纯函数，不碰磁盘。
pub fn plan_rename(path: &str, input: &str) -> Result<RenamePlan, String> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return Err("文件名不能为空".to_string());
    }
    if trimmed.contains('/') || trimmed.contains('\\') {
        return Err("只能改文件名，不能改目录".to_string());
    }
    if trimmed == "." || trimmed == ".." {
        return Err("文件名不能是「.」或「..」".to_string());
    }
    let source = Path::new(path);
    let dir = source
        .parent()
        .ok_or_else(|| "无法定位文件所在目录".to_string())?;
    let original_ext = source
        .extension()
        .map(|e| e.to_string_lossy().to_string())
        .unwrap_or_default();
    let has_ext = Path::new(trimmed).extension().is_some() && !trimmed.starts_with('.');
    let name = if has_ext || original_ext.is_empty() {
        trimmed.to_string()
    } else {
        format!("{trimmed}.{original_ext}")
    };
    let target: PathBuf = dir.join(&name);
    Ok(RenamePlan {
        from: path.to_string(),
        to: target.to_string_lossy().to_string(),
        name,
        conflict: None,
    })
}

/// 批量模板：`{name}` = 原文件名（去扩展名），`{n}` = 序号；`pad` 为补零位数。
pub fn plan_batch_rename(
    paths: &[String],
    template: &str,
    start: usize,
    pad: usize,
) -> Result<Vec<RenamePlan>, String> {
    let trimmed = template.trim();
    if trimmed.is_empty() {
        return Err("命名模板不能为空".to_string());
    }
    if !trimmed.contains("{name}") && !trimmed.contains("{n}") {
        return Err("命名模板至少要包含 {name} 或 {n}".to_string());
    }
    let mut plans = Vec::with_capacity(paths.len());
    for (index, path) in paths.iter().enumerate() {
        let stem = Path::new(path)
            .file_stem()
            .map(|s| s.to_string_lossy().to_string())
            .ok_or_else(|| format!("无法从路径读取文件名：{path}"))?;
        let number = start + index;
        let digits = if pad > 0 {
            format!("{number:0width$}", width = pad)
        } else {
            number.to_string()
        };
        let name = trimmed.replace("{name}", &stem).replace("{n}", &digits);
        plans.push(plan_rename(path, &name)?);
    }
    // 批内重名：直接判冲突，不执行。
    for i in 0..plans.len() {
        for j in (i + 1)..plans.len() {
            if plans[i].to.eq_ignore_ascii_case(&plans[j].to) {
                plans[i].conflict = Some("与列表中另一项重名".to_string());
                plans[j].conflict = Some("与列表中另一项重名".to_string());
            }
        }
    }
    Ok(plans)
}

fn same_file(a: &Path, b: &Path) -> bool {
    match (a.canonicalize(), b.canonicalize()) {
        (Ok(a), Ok(b)) => {
            if cfg!(windows) {
                a.to_string_lossy().to_lowercase() == b.to_string_lossy().to_lowercase()
            } else {
                a == b
            }
        }
        _ => false,
    }
}

/// 磁盘检查：目标已存在且不是自身（大小写改名）就算冲突。薄 I/O 层。
pub fn check_conflict(plan: &mut RenamePlan) {
    if plan.conflict.is_some() {
        return;
    }
    let from = Path::new(&plan.from);
    let to = Path::new(&plan.to);
    if !from.exists() {
        plan.conflict = Some("源文件已不存在".to_string());
        return;
    }
    if from == to {
        plan.conflict = Some("文件名没有变化".to_string());
        return;
    }
    if to.exists() && !same_file(from, to) {
        plan.conflict = Some("目标文件名已存在".to_string());
    }
}

fn apply_one(plan: &RenamePlan) -> Result<(), String> {
    std::fs::rename(&plan.from, &plan.to).map_err(|e| format!("改名失败：{e}"))
}

/// 单文件改名：规划 → 冲突检查 → 执行。
pub fn rename_file(path: &str, input: &str) -> Result<RenameResult, String> {
    let mut plan = plan_rename(path, input)?;
    check_conflict(&mut plan);
    if let Some(reason) = plan.conflict {
        return Err(reason);
    }
    apply_one(&plan)?;
    Ok(RenameResult {
        from: plan.from,
        to: plan.to,
        name: plan.name,
    })
}

/// 批量改名：先整体预检；有冲突就原样返回计划并 applied=false。
/// 执行中途失败会按倒序回滚已改的文件，避免半途状态。
pub fn batch_rename_files(
    paths: &[String],
    template: &str,
    start: usize,
    pad: usize,
    dry_run: bool,
) -> Result<BatchRenameResult, String> {
    if paths.is_empty() {
        return Err("没有需要改名的文件".to_string());
    }
    let mut plans = plan_batch_rename(paths, template, start, pad)?;
    for plan in plans.iter_mut() {
        check_conflict(plan);
    }
    let has_conflict = plans.iter().any(|p| p.conflict.is_some());
    if dry_run || has_conflict {
        return Ok(BatchRenameResult {
            plans,
            applied: false,
        });
    }
    let mut done: Vec<&RenamePlan> = Vec::new();
    for plan in &plans {
        if let Err(error) = apply_one(plan) {
            for applied in done.iter().rev() {
                let _ = std::fs::rename(&applied.to, &applied.from);
            }
            return Err(error);
        }
        done.push(plan);
    }
    Ok(BatchRenameResult {
        plans,
        applied: true,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_dir(tag: &str) -> PathBuf {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("ff-rename-{tag}-{stamp}"));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn keeps_original_extension_when_omitted() {
        let plan = plan_rename("/tmp/clip.mov", "开场").unwrap();
        assert_eq!(plan.name, "开场.mov");
        assert!(plan.to.ends_with("开场.mov"));
        assert!(plan.conflict.is_none());
    }

    #[test]
    fn accepts_explicit_extension() {
        let plan = plan_rename("/tmp/clip.mov", "开场.mp4").unwrap();
        assert_eq!(plan.name, "开场.mp4");
    }

    #[test]
    fn rejects_path_parts_and_empty_names() {
        assert!(plan_rename("/tmp/clip.mov", "  ").is_err());
        assert!(plan_rename("/tmp/clip.mov", "a/b").is_err());
        assert!(plan_rename("/tmp/clip.mov", "a\\b").is_err());
        assert!(plan_rename("/tmp/clip.mov", "..").is_err());
    }

    #[test]
    fn batch_template_pads_numbers() {
        let paths = vec![
            "/tmp/a.mp4".to_string(),
            "/tmp/b.mp4".to_string(),
        ];
        let plans = plan_batch_rename(&paths, "{name}-{n}", 1, 2).unwrap();
        assert_eq!(plans[0].name, "a-01.mp4");
        assert_eq!(plans[1].name, "b-02.mp4");
    }

    #[test]
    fn batch_marks_duplicate_targets() {
        let paths = vec![
            "/tmp/clip.mp4".to_string(),
            "/tmp/clip.mov".to_string(),
        ];
        // 显式扩展名让两个不同源文件撞到同一个目标名
        let plans = plan_batch_rename(&paths, "{name}.mp4", 1, 0).unwrap();
        assert_eq!(plans[0].conflict.as_deref(), Some("与列表中另一项重名"));
        assert_eq!(plans[1].conflict.as_deref(), Some("与列表中另一项重名"));
    }

    #[test]
    fn rename_on_disk_and_reject_existing_target() {
        let dir = temp_dir("disk");
        let from = dir.join("a.txt");
        let other = dir.join("b.txt");
        std::fs::write(&from, b"a").unwrap();
        std::fs::write(&other, b"b").unwrap();
        let renamed = rename_file(from.to_str().unwrap(), "c").unwrap();
        assert!(renamed.to.ends_with("c.txt"));
        assert!(!from.exists());
        assert!(dir.join("c.txt").exists());

        let err = rename_file(dir.join("c.txt").to_str().unwrap(), "b.txt").unwrap_err();
        assert_eq!(err, "目标文件名已存在");
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn batch_refuses_to_run_with_conflicts() {
        let dir = temp_dir("batch");
        let a = dir.join("a.txt");
        let b = dir.join("b.txt");
        std::fs::write(&a, b"a").unwrap();
        std::fs::write(&b, b"b").unwrap();
        // 预先占住 1.txt，让 {n} 模板的第一项冲突
        std::fs::write(dir.join("1.txt"), b"taken").unwrap();
        let paths = vec![
            a.to_string_lossy().to_string(),
            b.to_string_lossy().to_string(),
        ];
        let result = batch_rename_files(&paths, "{n}", 1, 0, false).unwrap();
        assert!(!result.applied);
        assert!(result.plans.iter().any(|p| p.conflict.is_some()));
        assert!(a.exists() && b.exists());
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn batch_applies_after_clean_preflight() {
        let dir = temp_dir("batch-ok");
        let a = dir.join("a.txt");
        let b = dir.join("b.txt");
        std::fs::write(&a, b"a").unwrap();
        std::fs::write(&b, b"b").unwrap();
        let paths = vec![
            a.to_string_lossy().to_string(),
            b.to_string_lossy().to_string(),
        ];
        let dry = batch_rename_files(&paths, "{n}", 1, 2, true).unwrap();
        assert!(!dry.applied);
        assert!(dry.plans.iter().all(|p| p.conflict.is_none()));

        let applied = batch_rename_files(&paths, "{n}", 1, 2, false).unwrap();
        assert!(applied.applied);
        assert!(dir.join("01.txt").exists());
        assert!(dir.join("02.txt").exists());
        assert!(!a.exists() && !b.exists());
        std::fs::remove_dir_all(&dir).ok();
    }
}
