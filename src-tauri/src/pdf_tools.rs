//! 外部 PDF 渲染器检测。合并/拆分走内置 `lopdf`，不需要任何外部工具；
//! 只有 PDF → 图片需要 `pdftoppm`。缺失时这里给出明确原因，UI 入口直接置灰。

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct PdfToolsInfo {
    pub pdftoppm: Option<String>,
    /// 搜索来源：env | path | custom | resources
    pub source: Option<String>,
    pub available: bool,
    /// 不可用时给用户看的完整原因（含安装建议）。
    pub reason: Option<String>,
    pub note: String,
}

fn exe_name(base: &str) -> String {
    if cfg!(windows) {
        format!("{base}.exe")
    } else {
        base.to_string()
    }
}

fn find_in_dir(dir: &Path, base: &str) -> Option<PathBuf> {
    let direct = dir.join(exe_name(base));
    if direct.is_file() {
        return Some(direct);
    }
    // Poppler Windows 包常见结构：<root>/Library/bin/pdftoppm.exe。
    let nested = dir.join("Library").join("bin").join(exe_name(base));
    if nested.is_file() {
        return Some(nested);
    }
    let bin = dir.join("bin").join(exe_name(base));
    if bin.is_file() {
        return Some(bin);
    }
    None
}

/// 解析顺序：PDFTOPPM_PATH（完整路径）→ 设置里的工具目录 → TRANSMUTIO_PDF_TOOLS（目录列表）→ PATH。
/// 不做任何隐式下载；找不到就明确说找不到。
pub fn resolve(custom_dir: Option<&str>) -> PdfToolsInfo {
    resolve_with(
        custom_dir,
        std::env::var_os("PDFTOPPM_PATH").map(PathBuf::from),
        std::env::var_os("TRANSMUTIO_PDF_TOOLS").as_deref(),
        std::env::var_os("PATH").as_deref(),
    )
}

fn resolve_with(
    custom_dir: Option<&str>,
    explicit: Option<PathBuf>,
    env_dirs: Option<&std::ffi::OsStr>,
    path_env: Option<&std::ffi::OsStr>,
) -> PdfToolsInfo {
    let mut source: Option<String> = None;
    let mut found: Option<PathBuf> = None;

    if let Some(path) = explicit.filter(|p| p.is_file()) {
        found = Some(path);
        source = Some("env".into());
    }

    if found.is_none() {
        if let Some(dir) = custom_dir.map(str::trim).filter(|s| !s.is_empty()) {
            found = find_in_dir(Path::new(dir), "pdftoppm");
            if found.is_some() {
                source = Some("custom".into());
            }
        }
    }

    if found.is_none() {
        if let Some(list) = env_dirs {
            for dir in std::env::split_paths(list) {
                if dir.as_os_str().is_empty() {
                    continue;
                }
                if let Some(path) = find_in_dir(&dir, "pdftoppm") {
                    found = Some(path);
                    source = Some("env".into());
                    break;
                }
            }
        }
    }

    if found.is_none() {
        if let Some(path) = path_env.and_then(|paths| {
            std::env::split_paths(paths)
                .filter_map(|dir| {
                    let candidate = dir.join(exe_name("pdftoppm"));
                    candidate.is_file().then_some(candidate)
                })
                .find(|_| true)
        }) {
            found = Some(path);
            source = Some("path".into());
        }
    }

    let available = found.is_some();
    let reason = if available {
        None
    } else {
        Some(
            "未找到 pdftoppm（Poppler）。「PDF 转图片」需要它完成栅格化；合并/拆分不受影响。\n\
             安装方式：Windows 可下载 Poppler for Windows 并解压，在设置中填写解压目录（或把 \
             Library/bin 加入 PATH）；也可设置环境变量 PDFTOPPM_PATH 指向 pdftoppm.exe。"
                .to_string(),
        )
    };

    PdfToolsInfo {
        pdftoppm: found.map(|p| p.to_string_lossy().to_string()),
        source,
        available,
        reason,
        note: "查找顺序：PDFTOPPM_PATH → 设置中的工具目录 → TRANSMUTIO_PDF_TOOLS → PATH".into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_tool_is_reported_with_reason() {
        // 纯函数路径：显式路径不存在、目录不存在、环境目录为空、PATH 为空。
        let info = resolve_with(
            Some("/definitely/not/here"),
            None,
            None,
            Some(std::ffi::OsStr::new("")),
        );
        assert!(!info.available);
        assert!(info.reason.as_deref().unwrap_or("").contains("pdftoppm"));
        assert!(info.pdftoppm.is_none());
    }

    #[test]
    fn explicit_existing_path_wins() {
        let current = std::env::current_exe().unwrap();
        let dir = current.parent().unwrap().to_path_buf();
        if dir.join(exe_name("pdftoppm")).is_file() {
            let info = resolve_with(None, Some(dir.join(exe_name("pdftoppm"))), None, None);
            assert!(info.available);
            assert_eq!(info.source.as_deref(), Some("env"));
        }
    }
}
