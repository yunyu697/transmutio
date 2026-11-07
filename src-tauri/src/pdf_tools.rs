//! 外部 PDF 工具检测。合并/拆分走内置 `lopdf`，不需要任何外部工具；
//! PDF → 图片需要 `pdftoppm`，PDF → 文本需要 `pdftotext`（同属 Poppler）。
//! 缺失时这里给出明确原因，UI 入口直接置灰。

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct PdfToolsInfo {
    pub pdftoppm: Option<String>,
    /// PDF → 文本用的 pdftotext（与 pdftoppm 同包分发）
    pub pdftotext: Option<String>,
    /// 搜索来源：env | path | custom | resources
    pub source: Option<String>,
    /// pdftoppm 是否可用（PDF → 图片）
    pub available: bool,
    /// 不可用时给用户看的完整原因（含安装建议）。
    pub reason: Option<String>,
    /// pdftotext 是否可用（PDF → 文本）
    pub text_available: bool,
    pub text_reason: Option<String>,
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

/// 解析顺序：显式路径 → 设置里的工具目录 → TRANSMUTIO_PDF_TOOLS（目录列表）→ PATH。
/// 不做任何隐式下载；找不到就明确说找不到。pdftoppm 与 pdftotext 各自独立判定。
pub fn resolve(custom_dir: Option<&str>) -> PdfToolsInfo {
    resolve_with(
        custom_dir,
        std::env::var_os("PDFTOPPM_PATH").map(PathBuf::from),
        std::env::var_os("PDFTOTEXT_PATH").map(PathBuf::from),
        std::env::var_os("TRANSMUTIO_PDF_TOOLS").as_deref(),
        std::env::var_os("PATH").as_deref(),
    )
}

fn resolve_one(
    base: &str,
    custom_dir: Option<&str>,
    explicit: Option<PathBuf>,
    env_dirs: Option<&std::ffi::OsStr>,
    path_env: Option<&std::ffi::OsStr>,
) -> (Option<PathBuf>, Option<String>) {
    let mut source: Option<String> = None;
    let mut found: Option<PathBuf> = None;

    if let Some(path) = explicit.filter(|p| p.is_file()) {
        found = Some(path);
        source = Some("env".into());
    }

    if found.is_none() {
        if let Some(dir) = custom_dir.map(str::trim).filter(|s| !s.is_empty()) {
            found = find_in_dir(Path::new(dir), base);
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
                if let Some(path) = find_in_dir(&dir, base) {
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
                    let candidate = dir.join(exe_name(base));
                    candidate.is_file().then_some(candidate)
                })
                .find(|_| true)
        }) {
            found = Some(path);
            source = Some("path".into());
        }
    }
    (found, source)
}

fn resolve_with(
    custom_dir: Option<&str>,
    poppm_explicit: Option<PathBuf>,
    text_explicit: Option<PathBuf>,
    env_dirs: Option<&std::ffi::OsStr>,
    path_env: Option<&std::ffi::OsStr>,
) -> PdfToolsInfo {
    let (poppm, mut source) = resolve_one("pdftoppm", custom_dir, poppm_explicit, env_dirs, path_env);
    let (text, text_source) = resolve_one("pdftotext", custom_dir, text_explicit, env_dirs, path_env);
    if source.is_none() {
        source = text_source;
    }

    let install_hint = "\n安装方式：Windows 可下载 Poppler for Windows 并解压，在设置中填写解压目录（或把 \
         Library/bin 加入 PATH）；也可设置环境变量 PDFTOPPM_PATH / PDFTOTEXT_PATH 指向对应 exe。";
    let available = poppm.is_some();
    let reason = if available {
        None
    } else {
        Some(format!(
            "未找到 pdftoppm（Poppler）。「PDF 转图片」需要它完成栅格化；合并/拆分不受影响。{install_hint}"
        ))
    };
    let text_available = text.is_some();
    let text_reason = if text_available {
        None
    } else {
        Some(format!(
            "未找到 pdftotext（Poppler）。「PDF 转文本」需要它提取文字；合并/拆分不受影响。{install_hint}"
        ))
    };

    PdfToolsInfo {
        pdftoppm: poppm.map(|p| p.to_string_lossy().to_string()),
        pdftotext: text.map(|p| p.to_string_lossy().to_string()),
        source,
        available,
        reason,
        text_available,
        text_reason,
        note: "查找顺序：显式路径 → 设置中的工具目录 → TRANSMUTIO_PDF_TOOLS → PATH".into(),
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
            None,
            Some(std::ffi::OsStr::new("")),
        );
        assert!(!info.available);
        assert!(info.reason.as_deref().unwrap_or("").contains("pdftoppm"));
        assert!(info.pdftoppm.is_none());
        assert!(!info.text_available);
        assert!(info.text_reason.as_deref().unwrap_or("").contains("pdftotext"));
    }

    #[test]
    fn explicit_existing_path_wins() {
        let current = std::env::current_exe().unwrap();
        let dir = current.parent().unwrap().to_path_buf();
        if dir.join(exe_name("pdftoppm")).is_file() {
            let info = resolve_with(None, Some(dir.join(exe_name("pdftoppm"))), None, None, None);
            assert!(info.available);
            assert_eq!(info.source.as_deref(), Some("env"));
        }
        if dir.join(exe_name("pdftotext")).is_file() {
            let info = resolve_with(None, None, Some(dir.join(exe_name("pdftotext"))), None, None);
            assert!(info.text_available);
        }
    }
}
