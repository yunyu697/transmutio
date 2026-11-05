use crate::{flavor, jobs::AppState, tools};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tauri::{AppHandle, Manager, State};

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct FfToolInspection {
    pub path: String,
    pub exists: bool,
    pub size: u64,
    pub sha256: String,
    pub version: String,
    pub configuration: String,
    pub gpl: bool,
    pub nonfree: bool,
    pub lgpl_ok: bool,
    /// 是否至少有一个 H.264 编码器（含硬编、OpenH264、GPL 的 libx264）。
    pub has_h264_encoder: bool,
    pub has_aac_encoder: bool,
    pub notes: Vec<String>,
}

fn exe_name(base: &str) -> String {
    if cfg!(windows) {
        format!("{base}.exe")
    } else {
        base.to_string()
    }
}

pub fn hash_file(path: &Path) -> std::io::Result<String> {
    let mut file = std::fs::File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buf = vec![0u8; 1024 * 1024];
    loop {
        let n = file.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(format!("{:x}", hasher.finalize()))
}

fn encoder_list(ffmpeg: &str) -> String {
    let mut cmd = std::process::Command::new(ffmpeg);
    cmd.args(["-hide_banner", "-encoders"])
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());
    #[cfg(windows)]
    {
        // 引导页「检测所选 ffmpeg」时也会 spawn，缺这个标志就会闪一个控制台窗口。
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    match cmd.output() {
        Ok(o) => String::from_utf8_lossy(&o.stdout).to_string() + &String::from_utf8_lossy(&o.stderr),
        Err(_) => String::new(),
    }
}

fn inspect_blocking(path: &str) -> FfToolInspection {
    let p = PathBuf::from(path);
    let mut result = FfToolInspection {
        path: path.to_string(),
        ..Default::default()
    };
    if !p.is_file() {
        result.notes.push("文件不存在或不是可执行文件".into());
        return result;
    }
    result.exists = true;
    result.size = std::fs::metadata(&p).map(|m| m.len()).unwrap_or(0);
    result.sha256 = hash_file(&p).unwrap_or_default();

    let (version, configuration) = tools::version_of(path).unwrap_or_default();
    result.version = version.clone();
    result.configuration = configuration.clone();
    result.gpl = configuration.contains("--enable-gpl");
    result.nonfree =
        configuration.contains("--enable-nonfree") || configuration.contains("--enable-libfdk-aac");
    result.lgpl_ok = !result.gpl && !result.nonfree;

    let encoders = encoder_list(path);
    result.has_h264_encoder = encoders.lines().any(|l| {
        let l = l.trim();
        l.contains("h264_") || l.contains("libx264") || l.contains("libopenh264")
    });
    result.has_aac_encoder = encoders.lines().any(|l| {
        let l = l.trim();
        l.contains(" aac") || l.contains("libfdk_aac") || l.contains("aac_at")
    });

    if version.is_empty() {
        result.notes.push("无法读取 ffmpeg 版本（可能不是 ffmpeg 可执行文件）".into());
    }
    if result.nonfree {
        result.notes.push("包含 nonfree 组件：任何变体都不接受".into());
    } else if result.gpl {
        result.notes.push(if flavor::Flavor::current().require_gpl_build() {
            "GPL 构建：符合 GPL 版（Transmutio GPL）的许可要求".into()
        } else {
            "GPL 构建：LGPL 版不接受；如需 x264/x265 软件编码请改用 GPL 版".into()
        });
    } else if flavor::Flavor::current().require_gpl_build() {
        result
            .notes
            .push("LGPL 构建：GPL 版需要 GPL 构建（含 libx264/libx265）".into());
    }
    if !result.has_h264_encoder {
        result.notes.push("未发现 H.264 编码器".into());
    }
    result
}

#[tauri::command]
pub async fn inspect_ffmpeg_file(path: String) -> Result<FfToolInspection, String> {
    tokio::task::spawn_blocking(move || inspect_blocking(&path))
        .await
        .map_err(|e| format!("校验失败：{e}"))
}

/// 同步版本，供无窗口的落位检查复用（与应用命令共用同一份实现）。
pub fn inspect_file(path: &str) -> FfToolInspection {
    inspect_blocking(&path.check_path())
}

fn copy_atomic(src: &Path, dest: &Path) -> std::io::Result<()> {
    let tmp = dest.with_file_name(format!(
        ".tmp-{}-{}",
        std::process::id(),
        dest.file_name().map(|s| s.to_string_lossy().to_string()).unwrap_or_default()
    ));
    std::fs::copy(src, &tmp)?;
    std::fs::rename(&tmp, dest)?;
    Ok(())
}

fn unique_dir(base: &Path) -> PathBuf {
    let mut candidate = base.to_path_buf();
    let mut index = 1;
    while candidate.exists() {
        candidate = base.with_file_name(format!(
            "{}-{index}",
            base.file_name().map(|s| s.to_string_lossy().to_string()).unwrap_or_default()
        ));
        index += 1;
    }
    candidate
}

/// 落位核心：把用户显式选择的 ffmpeg/ffprobe 校验后复制到 `app_data/tools/ffmpeg-<版本>/`。
/// 不做任何静默下载；不复制 nonfree。许可规则由 `flavor` 决定：
/// - `gpl` 变体：只接受 GPL 构建（`allow_gpl` 固定为 true）；
/// - `lgpl` 变体：拒绝 GPL 构建（仅 debug 构建可由 `dev_override` 打开）。
/// 不使用 Tauri 类型，应用命令与无窗口落位检查共用这一份实现。
pub fn install_blocking(
    app_data_dir: &Path,
    flavor: flavor::Flavor,
    ffmpeg_path: &str,
    ffprobe_path: Option<&str>,
    expected_sha256: Option<&str>,
    dev_override: bool,
) -> Result<tools::ToolsInfo, String> {
    let ffmpeg_path = ffmpeg_path.check_path();
    let ff = inspect_blocking(&ffmpeg_path);
    if !ff.exists {
        return Err("所选 ffmpeg 文件不存在".into());
    }
    if ff.version.is_empty() {
        return Err("所选文件不是可用的 ffmpeg（无法读取版本）".into());
    }
    flavor.accepts_build(ff.gpl, ff.nonfree, dev_override)?;
    if let Some(expected) = expected_sha256.map(str::trim).filter(|s| !s.is_empty()) {
        if !ff.sha256.eq_ignore_ascii_case(expected) {
            return Err(format!(
                "哈希不匹配：期望 {expected}，实际 {}（未落位任何文件）",
                ff.sha256
            ));
        }
    }
    if let Some(probe_path) = ffprobe_path.map(str::trim).filter(|s| !s.is_empty()) {
        let probe = inspect_blocking(probe_path);
        if !probe.exists {
            return Err("所选 ffprobe 文件不存在".into());
        }
    }

    let safe_version = {
        let v = ff.version.replace(['/', '\\', ':', ' '], "_");
        if v.is_empty() { "unknown".to_string() } else { v }
    };
    let dest_root = app_data_dir.join("tools");
    std::fs::create_dir_all(&dest_root)
        .map_err(|e| format!("创建工具目录失败：{e}"))?;
    let dest_dir = unique_dir(&dest_root.join(format!("ffmpeg-{safe_version}")));
    std::fs::create_dir_all(&dest_dir).map_err(|e| format!("创建落位目录失败：{e}"))?;

    let dest_ffmpeg = dest_dir.join(exe_name("ffmpeg"));
    copy_atomic(Path::new(&ffmpeg_path), &dest_ffmpeg)
        .map_err(|e| format!("复制 ffmpeg 失败：{e}"))?;
    let dest_ffprobe = if let Some(probe_path) = ffprobe_path.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
        let target = dest_dir.join(exe_name("ffprobe"));
        copy_atomic(Path::new(probe_path), &target)
            .map_err(|e| format!("复制 ffprobe 失败：{e}"))?;
        Some(target)
    } else {
        // 尝试从源目录同级的 ffprobe
        let sibling = Path::new(&ffmpeg_path)
            .parent()
            .map(|d| d.join(exe_name("ffprobe")))
            .filter(|p| p.is_file());
        sibling.map(|src| {
            let target = dest_dir.join(exe_name("ffprobe"));
            let _ = copy_atomic(&src, &target);
            target
        })
    };

    // 落位后复核：哈希必须与校验时一致，防止复制过程出错。
    let copied_hash = hash_file(&dest_ffmpeg).map_err(|e| format!("复核落位文件失败：{e}"))?;
    if copied_hash != ff.sha256 {
        let _ = std::fs::remove_dir_all(&dest_dir);
        return Err("落位后哈希与源文件不一致，已回滚".into());
    }

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        for p in [Some(&dest_ffmpeg), dest_ffprobe.as_ref()].into_iter().flatten() {
            if let Ok(meta) = std::fs::metadata(p) {
                let mut perms = meta.permissions();
                perms.set_mode(0o755);
                let _ = std::fs::set_permissions(p, perms);
            }
        }
    }

    Ok(tools::describe_from_paths(
        &dest_ffmpeg,
        dest_ffprobe.as_deref(),
        "app-data",
    ))
}

/// Tauri 命令：落位成功后把工具信息写回共享状态，供入队/调度使用。
#[tauri::command]
pub async fn install_ffmpeg_files(
    app: AppHandle,
    state: State<'_, Arc<AppState>>,
    ffmpeg_path: String,
    ffprobe_path: Option<String>,
    expected_sha256: Option<String>,
    allow_gpl: bool,
) -> Result<tools::ToolsInfo, String> {
    let app_data = app
        .path()
        .app_data_dir()
        .map_err(|e| format!("无法定位应用数据目录：{e}"))?;
    let flavor_now = flavor::Flavor::current();
    let dev_override = flavor::allow_gpl_effective(allow_gpl);
    let info = tokio::task::spawn_blocking(move || {
        install_blocking(
            &app_data,
            flavor_now,
            &ffmpeg_path,
            ffprobe_path.as_deref(),
            expected_sha256.as_deref(),
            dev_override,
        )
    })
    .await
    .map_err(|e| format!("安装失败：{e}"))??;
    *state.tools.lock().await = Some(info.clone());
    Ok(info)
}

/// 去掉 Windows 资源管理器复制路径时可能带上的引号。
trait CleanPath {
    fn check_path(&self) -> String;
}

impl CleanPath for String {
    fn check_path(&self) -> String {
        self.as_str().check_path()
    }
}

impl CleanPath for str {
    fn check_path(&self) -> String {
        self.trim().trim_matches('"').trim().to_string()
    }
}
