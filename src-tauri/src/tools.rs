use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};
use tauri::Manager;

/// 三平台 sidecar 目标三元组，用于定位 `ffmpeg-<triple>` 命名的二进制（Tauri externalBin 约定）。
pub fn target_triple() -> &'static str {
    if cfg!(all(target_os = "windows", target_arch = "x86_64")) {
        "x86_64-pc-windows-msvc"
    } else if cfg!(all(target_os = "windows", target_arch = "aarch64")) {
        "aarch64-pc-windows-msvc"
    } else if cfg!(all(target_os = "macos", target_arch = "aarch64")) {
        "aarch64-apple-darwin"
    } else if cfg!(all(target_os = "macos", target_arch = "x86_64")) {
        "x86_64-apple-darwin"
    } else if cfg!(all(target_os = "linux", target_arch = "x86_64")) {
        "x86_64-unknown-linux-gnu"
    } else if cfg!(all(target_os = "linux", target_arch = "aarch64")) {
        "aarch64-unknown-linux-gnu"
    } else {
        "unknown-target"
    }
}

fn exe_name(base: &str) -> String {
    if cfg!(windows) {
        format!("{base}.exe")
    } else {
        base.to_string()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ToolLocation {
    pub path: String,
    /// env | bundled | app-dir | resources | path | custom
    pub source: String,
    pub exists: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct BuildFlags {
    pub version: String,
    pub configuration: String,
    pub gpl: bool,
    pub nonfree: bool,
    /// 仅 LGPL：可安全分发。
    pub lgpl_ok: bool,
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ToolsInfo {
    pub ffmpeg: Option<ToolLocation>,
    pub ffprobe: Option<ToolLocation>,
    pub ffmpeg_version: Option<String>,
    pub ffprobe_version: Option<String>,
    pub build: BuildFlags,
    pub platform: String,
    pub arch: String,
    pub target_triple: String,
    pub warnings: Vec<String>,
    pub search_note: String,
}

fn looks_like_tool(path: &Path) -> bool {
    path.is_file()
}

/// 在给定目录里按约定查找二进制：`<dir>/name`、`<dir>/name-<triple>`、`<dir>/binaries/name`。
fn probe_dir(dir: &Path, base: &str, triple: &str) -> Option<PathBuf> {
    let plain = dir.join(exe_name(base));
    if looks_like_tool(&plain) {
        return Some(plain);
    }
    let suffixed = dir.join(exe_name(&format!("{base}-{triple}")));
    if looks_like_tool(&suffixed) {
        return Some(suffixed);
    }
    let in_binaries = dir.join("binaries").join(exe_name(base));
    if looks_like_tool(&in_binaries) {
        return Some(in_binaries);
    }
    let in_bin_suffixed = dir.join("binaries").join(exe_name(&format!("{base}-{triple}")));
    if looks_like_tool(&in_bin_suffixed) {
        return Some(in_bin_suffixed);
    }
    None
}

fn find_in_path(base: &str) -> Option<PathBuf> {
    let name = exe_name(base);
    let paths = std::env::var_os("PATH")?;
    for dir in std::env::split_paths(&paths) {
        let candidate = dir.join(&name);
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    None
}

fn sibling_of(path: &Path, base: &str) -> Option<PathBuf> {
    let dir = path.parent()?;
    let candidate = dir.join(exe_name(base));
    if candidate.is_file() {
        Some(candidate)
    } else {
        None
    }
}

/// 解析顺序：用户设置 > 环境变量 > 应用目录/sidecar > 资源目录 > PATH。
pub fn resolve(app: &tauri::AppHandle, custom_ffmpeg: Option<String>) -> ToolsInfo {
    let triple = target_triple();
    let exe_dir = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.to_path_buf()));

    let mut search: Vec<(PathBuf, &'static str)> = Vec::new();
    if let Some(dir) = exe_dir.clone() {
        search.push((dir.clone(), "app-dir"));
        // macOS: Contents/MacOS -> Contents/Resources
        if cfg!(target_os = "macos") {
            if let Some(parent) = dir.parent() {
                search.push((parent.join("Resources"), "resources"));
                search.push((parent.join("Resources").join("binaries"), "resources"));
            }
        }
        if cfg!(windows) {
            search.push((dir.join("resources"), "resources"));
        }
    }
    if let Ok(resource_dir) = app.path().resource_dir() {
        search.push((resource_dir.clone(), "resources"));
        search.push((resource_dir.join("ffmpeg"), "resources"));
    }

    let mut ffmpeg: Option<ToolLocation> = None;
    let mut ffprobe: Option<ToolLocation> = None;
    let mut hints: Vec<String> = Vec::new();

    // 1) 用户设置（设置页可填 ffmpeg 可执行文件路径）
    if let Some(custom) = custom_ffmpeg.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
        let p = PathBuf::from(custom);
        if p.is_file() {
            ffmpeg = Some(ToolLocation {
                path: p.to_string_lossy().to_string(),
                source: "custom".into(),
                exists: true,
            });
            ffprobe = sibling_of(&p, "ffprobe").map(|q| ToolLocation {
                path: q.to_string_lossy().to_string(),
                source: "custom".into(),
                exists: true,
            });
        } else {
            hints.push(format!("设置的 ffmpeg 路径不存在: {custom}"));
        }
    }

    // 2) 环境变量
    if ffmpeg.is_none() {
        if let Ok(env_path) = std::env::var("FFMPEG_PATH") {
            let p = PathBuf::from(&env_path);
            if p.is_file() {
                ffmpeg = Some(ToolLocation {
                    path: p.to_string_lossy().to_string(),
                    source: "env".into(),
                    exists: true,
                });
                ffprobe = std::env::var("FFPROBE_PATH")
                    .ok()
                    .map(PathBuf::from)
                    .filter(|q| q.is_file())
                    .or_else(|| sibling_of(&p, "ffprobe"))
                    .map(|q| ToolLocation {
                        path: q.to_string_lossy().to_string(),
                        source: "env".into(),
                        exists: true,
                    });
            } else {
                hints.push(format!("FFMPEG_PATH 指向的文件不存在: {env_path}"));
            }
        }
    }

    // 3) 应用目录 / 资源目录
    if ffmpeg.is_none() {
        for (dir, source) in &search {
            if let Some(found) = probe_dir(dir, "ffmpeg", triple) {
                ffmpeg = Some(ToolLocation {
                    path: found.to_string_lossy().to_string(),
                    source: source.to_string(),
                    exists: true,
                });
                ffprobe = probe_dir(dir, "ffprobe", triple).map(|q| ToolLocation {
                    path: q.to_string_lossy().to_string(),
                    source: source.to_string(),
                    exists: true,
                });
                break;
            }
        }
    }

    // 4) 系统 PATH（开发环境常见）
    if ffmpeg.is_none() {
        if let Some(found) = find_in_path("ffmpeg") {
            ffmpeg = Some(ToolLocation {
                path: found.to_string_lossy().to_string(),
                source: "path".into(),
                exists: true,
            });
            ffprobe = find_in_path("ffprobe").map(|q| ToolLocation {
                path: q.to_string_lossy().to_string(),
                source: "path".into(),
                exists: true,
            });
        }
    }

    // ffprobe 单独兜底
    if ffprobe.is_none() {
        if let Some(ff) = ffmpeg.as_ref() {
            ffprobe = sibling_of(Path::new(&ff.path), "ffprobe").map(|q| ToolLocation {
                path: q.to_string_lossy().to_string(),
                source: ff.source.clone(),
                exists: true,
            });
        }
    }
    if ffprobe.is_none() {
        ffprobe = find_in_path("ffprobe").map(|q| ToolLocation {
            path: q.to_string_lossy().to_string(),
            source: "path".into(),
            exists: true,
        });
    }

    let mut warnings = hints;
    // 每个工具只探测一次（版本与 configuration 都从同一次 `-version` 里取）。
    let ffmpeg_meta = ffmpeg.as_ref().and_then(|t| version_of(&t.path));
    let ffmpeg_version = ffmpeg_meta.as_ref().map(|v| v.0.clone());
    let configuration = ffmpeg_meta.map(|v| v.1).unwrap_or_default();
    let ffprobe_version = ffprobe
        .as_ref()
        .and_then(|t| version_of(&t.path).map(|v| v.0));
    let gpl = configuration.contains("--enable-gpl");
    let nonfree = configuration.contains("--enable-nonfree") || configuration.contains("--enable-libfdk-aac");
    let lgpl_ok = !gpl && !nonfree;
    if gpl || nonfree {
        warnings.push(
            "当前 FFmpeg 为 GPL/nonfree 构建：仅可用于本地开发验证，不得随默认安装包分发。\
             分发前请替换为 LGPL 构建（不含 libx264/libx265/fdk-aac/nonfree）。"
                .to_string(),
        );
    }
    if ffmpeg.is_none() {
        warnings.push(
            "未找到 ffmpeg。可设置环境变量 FFMPEG_PATH、在设置中指定路径，或将二进制放到应用目录/sidecar 资源目录。"
                .to_string(),
        );
    }

    ToolsInfo {
        ffmpeg,
        ffprobe,
        ffmpeg_version,
        ffprobe_version,
        build: BuildFlags {
            version: String::new(),
            configuration,
            gpl,
            nonfree,
            lgpl_ok,
            notes: Vec::new(),
        },
        platform: std::env::consts::OS.to_string(),
        arch: std::env::consts::ARCH.to_string(),
        target_triple: triple.to_string(),
        warnings,
        search_note: format!(
            "查找顺序：设置中的路径 → FFMPEG_PATH → 应用目录({}) → 资源目录 → PATH",
            triple
        ),
    }
}

/// 按显式路径构造工具信息（引导页落位后、headless 检查都用它，不依赖 Tauri 上下文）。
pub fn describe_from_paths(ffmpeg: &Path, ffprobe: Option<&Path>, source: &str) -> ToolsInfo {
    let triple = target_triple();
    let ffmpeg_loc = ffmpeg.is_file().then(|| ToolLocation {
        path: ffmpeg.to_string_lossy().to_string(),
        source: source.to_string(),
        exists: true,
    });
    let ffprobe_loc = ffprobe.filter(|p| p.is_file()).map(|p| ToolLocation {
        path: p.to_string_lossy().to_string(),
        source: source.to_string(),
        exists: true,
    });
    let (ffmpeg_version, configuration) = ffmpeg_loc
        .as_ref()
        .and_then(|t| version_of(&t.path))
        .map(|(v, c)| (Some(v), c))
        .unwrap_or((None, String::new()));
    let ffprobe_version = ffprobe_loc
        .as_ref()
        .and_then(|t| version_of(&t.path).map(|v| v.0));
    let gpl = configuration.contains("--enable-gpl");
    let nonfree = configuration.contains("--enable-nonfree") || configuration.contains("--enable-libfdk-aac");
    let mut warnings = Vec::new();
    if gpl || nonfree {
        warnings.push(
            "当前 FFmpeg 为 GPL/nonfree 构建：仅可用于本地开发验证，不得随默认安装包分发。".to_string(),
        );
    }
    if ffmpeg_loc.is_none() {
        warnings.push("指定的 ffmpeg 路径不存在。".to_string());
    }
    ToolsInfo {
        ffmpeg: ffmpeg_loc,
        ffprobe: ffprobe_loc,
        ffmpeg_version,
        ffprobe_version,
        build: BuildFlags {
            version: String::new(),
            configuration,
            gpl,
            nonfree,
            lgpl_ok: !gpl && !nonfree,
            notes: Vec::new(),
        },
        platform: std::env::consts::OS.to_string(),
        arch: std::env::consts::ARCH.to_string(),
        target_triple: triple.to_string(),
        warnings,
        search_note: format!("显式路径（source={source}）"),
    }
}

/// 在 PATH 中查找可执行文件（headless 检查与引导页共用）。
pub fn find_tool(base: &str) -> Option<PathBuf> {
    find_in_path(base)
}

/// 版本探测结果缓存：同一进程内同一个路径只 spawn 一次。
/// 「启动/诊断」路径原本每次 `resolve` 都要拉 ffmpeg 3 次 + ffprobe 1 次
/// （`resolve_tools` 与 `probe_capabilities` 各来一遍），每一次都是一个一闪而过的控制台窗口。
fn version_cache() -> &'static Mutex<HashMap<String, Option<(String, String)>>> {
    static CACHE: OnceLock<Mutex<HashMap<String, Option<(String, String)>>>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

fn cached(path: &str) -> Option<Option<(String, String)>> {
    match version_cache().lock() {
        Ok(cache) => cache.get(path).cloned(),
        Err(poisoned) => poisoned.into_inner().get(path).cloned(),
    }
}

/// 同一路径的探测串行化：启动时 `setup`、`resolve_tools`、`probe_capabilities` 会并发 `resolve`，
/// 只靠缓存仍可能同时 miss 而各 spawn 一次。这里保证同一路径同时只有一个探测，其余直接吃结果。
fn probe_locks() -> &'static Mutex<HashMap<String, Arc<Mutex<()>>>> {
    static LOCKS: OnceLock<Mutex<HashMap<String, Arc<Mutex<()>>>>> = OnceLock::new();
    LOCKS.get_or_init(|| Mutex::new(HashMap::new()))
}

fn probe_lock(path: &str) -> Arc<Mutex<()>> {
    let mut locks = match probe_locks().lock() {
        Ok(guard) => guard,
        Err(poisoned) => poisoned.into_inner(),
    };
    locks
        .entry(path.to_string())
        .or_insert_with(|| Arc::new(Mutex::new(())))
        .clone()
}

/// 返回 (版本串, configuration 串)。同步调用，仅用于启动/诊断，耗时几十毫秒。
pub fn version_of(path: &str) -> Option<(String, String)> {
    if let Some(hit) = cached(path) {
        return hit;
    }
    let guard = probe_lock(path);
    let _held = match guard.lock() {
        Ok(held) => held,
        Err(poisoned) => poisoned.into_inner(),
    };
    if let Some(hit) = cached(path) {
        return hit;
    }
    let result = read_version(path);
    if let Ok(mut cache) = version_cache().lock() {
        cache.insert(path.to_string(), result.clone());
    }
    result
}

fn read_version(path: &str) -> Option<(String, String)> {
    let mut cmd = std::process::Command::new(path);
    cmd.args(["-hide_banner", "-version"])
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());
    #[cfg(windows)]
    {
        // 缺这个标志，GUI 父进程每 spawn 一个控制台程序就闪一个黑框。
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    let output = cmd.output().ok()?;
    if !output.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&output.stdout).to_string()
        + &String::from_utf8_lossy(&output.stderr).to_string();
    let first = text.lines().next().unwrap_or("").to_string();
    let version = first
        .strip_prefix("ffmpeg version ")
        .or_else(|| {
            text.lines()
                .next()
                .and_then(|l| l.split("version ").nth(1))
        })
        .unwrap_or("")
        .split_whitespace()
        .next()
        .unwrap_or("")
        .to_string();
    let configuration = text
        .lines()
        .find(|l| l.trim_start().starts_with("configuration:"))
        .map(|l| l.trim_start().trim_start_matches("configuration:").trim().to_string())
        .unwrap_or_default();
    Some((version, configuration))
}
