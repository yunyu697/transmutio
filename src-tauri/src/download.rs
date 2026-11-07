//! 首次启动的可选下载：内置清单 + 多镜像按序回退 + 强制 SHA-256 校验 + 断点续传。
//!
//! 设计约束（与 README / LICENSING 一致）：
//! - 清单（含官方 SHA-256）随二进制编进应用，运行时只读，不可被替换；
//! - 镜像只换传输通道，**任何来源的文件都必须匹配内置校验和**，否则拒绝落盘；
//! - 包的许可变体必须与应用变体对应（LGPL 包只取 LGPL 构建，GPL 包只取 GPL 构建）；
//! - 下载失败一律给出可读原因（断网 / 超时 / 磁盘空间不足 / 校验失败 / 缺少必要文件），
//!   不会静默回退成「假装成功」；
//! - 不把二进制提交进仓库，安装包也不包含它们。

use crate::{flavor::Flavor, setup, tools};
use fs2::available_space;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::Duration;
use tauri::{AppHandle, Emitter, Manager, State};

pub const MANIFEST_JSON: &str = include_str!("../resources/tools-manifest.json");
pub const PROGRESS_EVENT: &str = "tools-download-progress";

/// 每次下载最多重试次数（含首次）。跨来源的回退不计入这个上限。
const ATTEMPTS_PER_SOURCE: u32 = 3;
/// 落位前预留空间（解压 + 临时文件），避免「下到一半才发现写满」。
const SPACE_MARGIN: u64 = 32 * 1024 * 1024;

// ---------------------------------------------------------------- 清单结构

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Manifest {
    pub schema: u32,
    pub generated_at: String,
    pub note: String,
    pub sources: Vec<SourceDef>,
    pub packages: Vec<Package>,
    pub manual: Vec<ManualEntry>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct SourceDef {
    pub id: String,
    pub label: String,
    /// `None` = 直连；`Some(prefix)` = 第三方加速前缀（拼在官方 URL 前面）。
    pub base: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExtractRule {
    /// 压缩包内条目名以该片段结尾时命中（如 `/bin/ffmpeg.exe`）。
    pub marker: String,
    /// 落位到暂存目录的相对路径。
    pub to: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ExtractTreeRule {
    pub marker: String,
    pub to: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Package {
    pub id: String,
    /// ffmpeg
    pub kind: String,
    pub platform: String,
    pub arch: String,
    /// lgpl | gpl | any
    pub flavor: String,
    pub version: String,
    pub build_label: String,
    pub archive: String,
    pub file: String,
    pub size: u64,
    pub sha256: String,
    pub url: String,
    #[serde(default)]
    pub extract: Vec<ExtractRule>,
    #[serde(default)]
    pub extract_trees: Vec<ExtractTreeRule>,
    /// 落位目录前缀：ffmpeg
    pub land: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ManualEntry {
    pub kind: String,
    pub platform: String,
    pub flavor: String,
    pub label: String,
    pub url: String,
    pub reason: String,
}

fn exe_name(base: &str) -> String {
    if cfg!(windows) {
        format!("{base}.exe")
    } else {
        base.to_string()
    }
}

static MANIFEST: OnceLock<Result<Manifest, String>> = OnceLock::new();

pub fn manifest() -> Result<&'static Manifest, String> {
    MANIFEST
        .get_or_init(|| {
            serde_json::from_str::<Manifest>(MANIFEST_JSON)
                .map_err(|e| format!("内置工具清单损坏：{e}"))
        })
        .as_ref()
        .map_err(|e| e.clone())
}

pub fn current_platform() -> &'static str {
    if cfg!(windows) {
        "windows"
    } else if cfg!(target_os = "macos") {
        "macos"
    } else {
        "linux"
    }
}

// ---------------------------------------------------------------- 计划（纯函数，可离线断言）

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlanSource {
    pub source_id: String,
    pub label: String,
    pub url: String,
    pub mirror: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlanEntry {
    pub package_id: String,
    pub kind: String,
    pub version: String,
    pub build_label: String,
    pub file: String,
    pub size: u64,
    pub sha256: String,
    pub sources: Vec<PlanSource>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolsPlan {
    pub platform: String,
    pub arch: String,
    pub flavor: String,
    pub entries: Vec<PlanEntry>,
    pub manual: Vec<ManualEntry>,
    pub notes: Vec<String>,
}

fn source_url(source: &SourceDef, official: &str) -> String {
    match source.base.as_deref() {
        Some(base) if !base.is_empty() => format!("{base}{official}"),
        _ => official.to_string(),
    }
}

/// 来源顺序：用户指定的来源优先，其余按清单顺序（官方直连在第一位）。
pub fn ordered_sources<'a>(
    sources: &'a [SourceDef],
    preferred: Option<&str>,
) -> Vec<&'a SourceDef> {
    let mut out: Vec<&SourceDef> = Vec::new();
    if let Some(id) = preferred.map(str::trim).filter(|s| !s.is_empty()) {
        if let Some(hit) = sources.iter().find(|s| s.id == id) {
            out.push(hit);
        }
    }
    for source in sources {
        if !out.iter().any(|s| s.id == source.id) {
            out.push(source);
        }
    }
    out
}

/// 生成当前平台/架构/变体的下载计划。没有内置包的组合返回 `manual`（带可读原因），
/// 不会伪造一个「可下载」选项。
pub fn plan_for(flavor: Flavor, platform: &str, arch: &str) -> Result<ToolsPlan, String> {
    let manifest = manifest()?;
    let mut entries = Vec::new();
    for pkg in &manifest.packages {
        if pkg.platform != platform || pkg.arch != arch {
            continue;
        }
        if pkg.flavor != "any" && pkg.flavor != flavor.id() {
            continue;
        }
        let sources = manifest
            .sources
            .iter()
            .map(|s| PlanSource {
                source_id: s.id.clone(),
                label: s.label.clone(),
                url: source_url(s, &pkg.url),
                mirror: s.base.is_some(),
            })
            .collect();
        entries.push(PlanEntry {
            package_id: pkg.id.clone(),
            kind: pkg.kind.clone(),
            version: pkg.version.clone(),
            build_label: pkg.build_label.clone(),
            file: pkg.file.clone(),
            size: pkg.size,
            sha256: pkg.sha256.clone(),
            sources,
        });
    }
    let manual = manifest
        .manual
        .iter()
        .filter(|m| m.platform == platform && (m.flavor == "any" || m.flavor == flavor.id()))
        .cloned()
        .collect();
    let notes = vec![
        "校验和来自内置清单（取自上游官方发布），镜像文件同样按它校验；不匹配即拒绝落盘。"
            .to_string(),
        "失败会按来源顺序自动回退并显示当前来源；断点保留在 tools/downloads/，可续传。".to_string(),
    ];
    Ok(ToolsPlan {
        platform: platform.to_string(),
        arch: arch.to_string(),
        flavor: flavor.id().to_string(),
        entries,
        manual,
        notes,
    })
}

// ---------------------------------------------------------------- 校验与探测（纯函数）

/// 磁盘剩余空间：不足时给出可读原因与具体数字（不开始下载）。
pub fn ensure_space(dir: &Path, required: u64) -> Result<u64, String> {
    std::fs::create_dir_all(dir).map_err(|e| format!("创建下载目录失败：{e}"))?;
    let free = available_space(dir).map_err(|e| format!("无法读取磁盘剩余空间：{e}"))?;
    if free < required {
        return Err(format!(
            "磁盘空间不足：需要约 {}，可用 {}（不会开始下载）",
            human(required),
            human(free)
        ));
    }
    Ok(free)
}

fn human(bytes: u64) -> String {
    const MB: f64 = 1024.0 * 1024.0;
    if bytes >= 1024 * 1024 * 1024 {
        format!("{:.1} GB", bytes as f64 / (1024.0 * MB * 1024.0))
    } else {
        format!("{:.1} MB", bytes as f64 / MB)
    }
}

/// 下载完成后的强制校验：大小 + SHA-256，任何一项不符都不落盘。
pub fn verify_file(path: &Path, expected_sha: &str, expected_size: u64) -> Result<String, String> {
    let meta = std::fs::metadata(path).map_err(|e| format!("读取下载文件失败：{e}"))?;
    if meta.len() != expected_size {
        return Err(format!(
            "大小不符：期望 {} 字节，实际 {} 字节（已拒绝落盘）",
            expected_size,
            meta.len()
        ));
    }
    let mut file = std::fs::File::open(path).map_err(|e| format!("打开下载文件失败：{e}"))?;
    let mut hasher = Sha256::new();
    let mut buf = vec![0u8; 1024 * 1024];
    loop {
        let n = file
            .read(&mut buf)
            .map_err(|e| format!("读取下载文件失败：{e}"))?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    let digest = format!("{:x}", hasher.finalize());
    if !digest.eq_ignore_ascii_case(expected_sha.trim()) {
        return Err(format!(
            "SHA-256 校验失败：期望 {expected_sha}，实际 {digest}（已拒绝落盘）"
        ));
    }
    Ok(digest)
}

/// 断点续传的决策：只取决于服务器状态码和本地已有字节，便于离线断言。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResumeAction {
    /// 从头写（服务器返回 200，或本地没有临时文件）。
    Restart,
    /// 追加写（服务器接受 Range，返回 206）。
    Append,
    /// 本地临时文件已完整，直接进入校验。
    Complete,
    /// 服务器拒绝范围且不完整：丢弃临时文件重下。
    DropAndRestart,
}

pub fn resume_action(status: u16, existing: u64, total: u64) -> ResumeAction {
    if existing >= total && total > 0 {
        ResumeAction::Complete
    } else if status == 206 && existing > 0 {
        ResumeAction::Append
    } else if status == 200 {
        ResumeAction::Restart
    } else if status == 416 {
        ResumeAction::DropAndRestart
    } else {
        ResumeAction::Restart
    }
}

fn io_hint(error: &std::io::Error) -> String {
    let raw = error.raw_os_error();
    // Windows: 112 = ERROR_DISK_FULL；Unix: 28 = ENOSPC
    if raw == Some(112) || raw == Some(28) {
        return "磁盘空间不足，写入失败（可清理磁盘后重试，临时文件保留）".to_string();
    }
    format!("写入失败：{error}")
}

fn net_error(error: &ureq::Error) -> String {
    match error {
        ureq::Error::Status(code, _) => format!("服务器返回 HTTP {code}"),
        ureq::Error::Transport(t) => format!("网络错误：{t}"),
    }
}

// ---------------------------------------------------------------- 下载

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Progress {
    pub kind: String,
    pub package_id: String,
    /// checking | download | verify | extract | land | done | retry
    pub phase: String,
    pub source_id: String,
    pub source_label: String,
    pub source_index: usize,
    pub source_total: usize,
    pub attempt: u32,
    pub received: u64,
    pub total: u64,
    pub message: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LandedTool {
    pub kind: String,
    pub dir: String,
    pub version: String,
    pub archive_sha256: String,
    pub tools: Option<tools::ToolsInfo>,
}

static CANCEL: AtomicBool = AtomicBool::new(false);
static ACTIVE: Mutex<()> = Mutex::new(());

pub fn cancel() {
    CANCEL.store(true, Ordering::SeqCst);
}

fn cancelled() -> bool {
    CANCEL.load(Ordering::SeqCst)
}

fn backoff(attempt: u32) {
    std::thread::sleep(Duration::from_millis(600 * u64::from(attempt)));
}

fn agent() -> ureq::Agent {
    ureq::AgentBuilder::new()
        .timeout_connect(Duration::from_secs(10))
        .timeout_read(Duration::from_secs(30))
        .timeout_write(Duration::from_secs(30))
        .user_agent(concat!("Transmutio/", env!("CARGO_PKG_VERSION")))
        .build()
}

/// 单次传输：支持 Range 续传；服务器不支持时自动从头再来。
/// `on_bytes` 每次追加后回调（已收字节，总字节）。
fn fetch_once(
    agent: &ureq::Agent,
    url: &str,
    part: &Path,
    total: u64,
    mut on_bytes: impl FnMut(u64),
) -> Result<(), String> {
    let existing = std::fs::metadata(part).map(|m| m.len()).unwrap_or(0);
    let mut req = agent.get(url).set("Accept-Encoding", "identity");
    if existing > 0 && existing < total {
        req = req.set("Range", &format!("bytes={existing}-"));
    }
    let response = req.call().map_err(|e| net_error(&e))?;
    let status = response.status();
    let action = resume_action(status, existing, total);
    if action == ResumeAction::Complete {
        return Ok(());
    }
    let append = match action {
        ResumeAction::Append => true,
        ResumeAction::Restart => false,
        ResumeAction::DropAndRestart => {
            std::fs::remove_file(part).ok();
            return Err("服务器拒绝续传范围（临时文件已丢弃，将重试）".to_string());
        }
        ResumeAction::Complete => true,
    };
    let mut file = if append {
        std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(part)
    } else {
        std::fs::OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .open(part)
    }
    .map_err(|e| format!("打开临时文件失败：{e}"))?;

    let mut reader = response.into_reader();
    let mut buf = vec![0u8; 256 * 1024];
    let mut received = if append { existing } else { 0 };
    on_bytes(received);
    loop {
        if cancelled() {
            return Err("已取消下载（临时文件保留，可续传）".to_string());
        }
        let n = reader
            .read(&mut buf)
            .map_err(|e| format!("读取响应失败：{e}"))?;
        if n == 0 {
            break;
        }
        file.write_all(&buf[..n]).map_err(|e| io_hint(&e))?;
        received += n as u64;
        on_bytes(received);
    }
    file.flush().map_err(|e| io_hint(&e))?;
    if received != total {
        return Err(format!(
            "下载不完整：收到 {} / {} 字节（临时文件保留，可续传）",
            received, total
        ));
    }
    Ok(())
}

fn package_by_id<'a>(manifest: &'a Manifest, id: &str) -> Result<&'a Package, String> {
    manifest
        .packages
        .iter()
        .find(|p| p.id == id)
        .ok_or_else(|| format!("内置清单里没有下载包 {id}"))
}

/// 校验包与当前平台/变体是否匹配（GPL 包不能装进 LGPL 版，反之亦然）。
pub fn check_package(
    pkg: &Package,
    flavor: Flavor,
    platform: &str,
    arch: &str,
) -> Result<(), String> {
    if pkg.platform != platform || pkg.arch != arch {
        return Err(format!(
            "该下载包只适用于 {} / {}，当前是 {} / {}",
            pkg.platform, pkg.arch, platform, arch
        ));
    }
    if pkg.flavor != "any" && pkg.flavor != flavor.id() {
        return Err(format!(
            "该下载包是 {} 构建，与当前 {} 版不匹配（不会下载）",
            pkg.flavor.to_uppercase(),
            flavor.id().to_uppercase()
        ));
    }
    Ok(())
}

/// 阻塞式下载 + 校验 + 解压 + 落位。应用命令与无窗口检查共用这一份实现。
pub fn download_blocking(
    app_data: &Path,
    flavor: Flavor,
    package_id: &str,
    preferred_source: Option<&str>,
    mut progress: impl FnMut(&Progress),
) -> Result<LandedTool, String> {
    CANCEL.store(false, Ordering::SeqCst);
    let _active = ACTIVE
        .try_lock()
        .map_err(|_| "已有一个下载在进行中，请等待它结束或先取消".to_string())?;

    let manifest = manifest()?;
    let pkg = package_by_id(manifest, package_id)?.clone();
    check_package(&pkg, flavor, current_platform(), std::env::consts::ARCH)?;

    let tools_dir = app_data.join("tools");
    let downloads = tools_dir.join("downloads");
    let part = downloads.join(format!("{}.part", pkg.file));
    let existing = std::fs::metadata(&part).map(|m| m.len()).unwrap_or(0);
    let required = pkg
        .size
        .saturating_sub(existing)
        .saturating_add(SPACE_MARGIN);
    ensure_space(&downloads, required)?;

    let sources = ordered_sources(&manifest.sources, preferred_source);
    let mut failures: Vec<String> = Vec::new();
    let mut landed: Option<(String, String)> = None;

    for (index, source) in sources.iter().enumerate() {
        if cancelled() {
            return Err("已取消下载".to_string());
        }
        let url = source_url(source, &pkg.url);
        let label = source.label.clone();
        let mut last_error = String::new();
        for attempt in 1..=ATTEMPTS_PER_SOURCE {
            if cancelled() {
                return Err("已取消下载".to_string());
            }
            progress(&Progress {
                kind: pkg.kind.clone(),
                package_id: pkg.id.clone(),
                phase: "download".into(),
                source_id: source.id.clone(),
                source_label: label.clone(),
                source_index: index,
                source_total: sources.len(),
                attempt,
                received: std::fs::metadata(&part).map(|m| m.len()).unwrap_or(0),
                total: pkg.size,
                message: format!("正在从「{label}」下载（第 {attempt} 次尝试）"),
            });
            let downloaded = fetch_once(&agent(), &url, &part, pkg.size, |received| {
                progress(&Progress {
                    kind: pkg.kind.clone(),
                    package_id: pkg.id.clone(),
                    phase: "download".into(),
                    source_id: source.id.clone(),
                    source_label: label.clone(),
                    source_index: index,
                    source_total: sources.len(),
                    attempt,
                    received,
                    total: pkg.size,
                    message: format!("正在从「{label}」下载"),
                });
            });
            if let Err(err) = downloaded {
                if cancelled() {
                    return Err(err);
                }
                last_error = err.clone();
                failures.push(format!("{label}（第 {attempt} 次）：{err}"));
                if attempt < ATTEMPTS_PER_SOURCE {
                    progress(&Progress {
                        kind: pkg.kind.clone(),
                        package_id: pkg.id.clone(),
                        phase: "retry".into(),
                        source_id: source.id.clone(),
                        source_label: label.clone(),
                        source_index: index,
                        source_total: sources.len(),
                        attempt,
                        received: std::fs::metadata(&part).map(|m| m.len()).unwrap_or(0),
                        total: pkg.size,
                        message: format!("「{label}」失败：{err}；准备重试"),
                    });
                    backoff(attempt);
                }
                continue;
            }
            progress(&Progress {
                kind: pkg.kind.clone(),
                package_id: pkg.id.clone(),
                phase: "verify".into(),
                source_id: source.id.clone(),
                source_label: label.clone(),
                source_index: index,
                source_total: sources.len(),
                attempt,
                received: pkg.size,
                total: pkg.size,
                message: "正在按内置 SHA-256 校验".into(),
            });
            match verify_file(&part, &pkg.sha256, pkg.size) {
                Ok(digest) => {
                    landed = Some((digest, label.clone()));
                    break;
                }
                Err(err) => {
                    // 镜像给的文件不采信：删掉临时文件，换下一个来源重新下载。
                    std::fs::remove_file(&part).ok();
                    last_error = err.clone();
                    failures.push(format!("{label}：{err}"));
                    break;
                }
            }
        }
        if landed.is_some() {
            break;
        }
        if !last_error.is_empty() {
            progress(&Progress {
                kind: pkg.kind.clone(),
                package_id: pkg.id.clone(),
                phase: "retry".into(),
                source_id: source.id.clone(),
                source_label: label.clone(),
                source_index: index,
                source_total: sources.len(),
                attempt: ATTEMPTS_PER_SOURCE,
                received: 0,
                total: pkg.size,
                message: format!("切换到下一个来源（{label} 未通过）"),
            });
        }
    }

    let (digest, _winning_source) = landed.ok_or_else(|| {
        format!(
            "所有来源都未通过（{} 个来源，每个最多 {} 次）：\n{}",
            sources.len(),
            ATTEMPTS_PER_SOURCE,
            failures.join("\n")
        )
    })?;

    progress(&Progress {
        kind: pkg.kind.clone(),
        package_id: pkg.id.clone(),
        phase: "extract".into(),
        source_id: String::new(),
        source_label: String::new(),
        source_index: 0,
        source_total: sources.len(),
        attempt: 0,
        received: pkg.size,
        total: pkg.size,
        message: "校验通过，正在解压".into(),
    });
    let result = land(&tools_dir, flavor, &pkg, &part, &digest, &mut progress)?;
    std::fs::remove_file(&part).ok();
    progress(&Progress {
        kind: pkg.kind.clone(),
        package_id: pkg.id.clone(),
        phase: "done".into(),
        source_id: String::new(),
        source_label: String::new(),
        source_index: 0,
        source_total: sources.len(),
        attempt: 0,
        received: pkg.size,
        total: pkg.size,
        message: format!("已落位到 {}", result.dir),
    });
    Ok(result)
}

// ---------------------------------------------------------------- 解压与落位

fn staging_dir(tools_dir: &Path) -> PathBuf {
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    tools_dir.join(format!(".staging-{}-{stamp}", std::process::id()))
}

fn unique_dir(base: &Path) -> PathBuf {
    let mut candidate = base.to_path_buf();
    let mut index = 1;
    while candidate.exists() {
        candidate = base.with_file_name(format!(
            "{}-{index}",
            base.file_name()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_default()
        ));
        index += 1;
    }
    candidate
}

fn safe_version(version: &str) -> String {
    let v: String = version
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_') {
                c
            } else {
                '_'
            }
        })
        .collect();
    if v.is_empty() {
        "unknown".to_string()
    } else {
        v
    }
}

fn write_entry(mut entry: impl Read, dest: &Path) -> Result<(), String> {
    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("创建目录失败：{e}"))?;
    }
    let mut file = std::fs::File::create(dest).map_err(|e| io_hint(&e))?;
    let mut buf = vec![0u8; 256 * 1024];
    loop {
        let n = entry
            .read(&mut buf)
            .map_err(|e| format!("解压读取失败：{e}"))?;
        if n == 0 {
            break;
        }
        file.write_all(&buf[..n]).map_err(|e| io_hint(&e))?;
    }
    Ok(())
}

fn extract(archive: &Path, staging: &Path, pkg: &Package) -> Result<(), String> {
    let file = std::fs::File::open(archive).map_err(|e| format!("打开压缩包失败：{e}"))?;
    let mut zip =
        zip::ZipArchive::new(file).map_err(|e| format!("压缩包无法读取（可能已损坏）：{e}"))?;
    let mut matched = vec![false; pkg.extract.len()];
    for index in 0..zip.len() {
        let mut entry = zip
            .by_index(index)
            .map_err(|e| format!("读取压缩包条目失败：{e}"))?;
        if entry.is_dir() {
            continue;
        }
        let name = entry.name().to_string();
        for (slot, rule) in pkg.extract.iter().enumerate() {
            if matched[slot] || !name.ends_with(rule.marker.as_str()) {
                continue;
            }
            let dest = staging.join(&rule.to);
            write_entry(&mut entry, &dest)?;
            matched[slot] = true;
        }
        for rule in &pkg.extract_trees {
            let Some(pos) = name.find(rule.marker.as_str()) else {
                continue;
            };
            let rel = &name[pos + rule.marker.len()..];
            if rel.is_empty() || rel.contains("..") {
                continue;
            }
            let dest = staging.join(&rule.to).join(rel);
            write_entry(&mut entry, &dest)?;
        }
    }
    for (slot, rule) in pkg.extract.iter().enumerate() {
        if !matched[slot] {
            return Err(format!(
                "压缩包内缺少必要文件（匹配 {}），已拒绝落位",
                rule.marker
            ));
        }
    }
    Ok(())
}

#[cfg(unix)]
fn make_executable(path: &Path) {
    use std::os::unix::fs::PermissionsExt;
    if let Ok(meta) = std::fs::metadata(path) {
        let mut perms = meta.permissions();
        perms.set_mode(0o755);
        let _ = std::fs::set_permissions(path, perms);
    }
}

#[cfg(not(unix))]
fn make_executable(_path: &Path) {}

fn land(
    tools_dir: &Path,
    flavor: Flavor,
    pkg: &Package,
    archive: &Path,
    digest: &str,
    progress: &mut impl FnMut(&Progress),
) -> Result<LandedTool, String> {
    let staging = staging_dir(tools_dir);
    std::fs::create_dir_all(&staging).map_err(|e| format!("创建暂存目录失败：{e}"))?;
    let outcome = land_inner(tools_dir, flavor, pkg, archive, digest, &staging, progress);
    if outcome.is_err() {
        std::fs::remove_dir_all(&staging).ok();
    }
    outcome
}

fn land_inner(
    tools_dir: &Path,
    flavor: Flavor,
    pkg: &Package,
    archive: &Path,
    digest: &str,
    staging: &Path,
    progress: &mut impl FnMut(&Progress),
) -> Result<LandedTool, String> {
    extract(archive, staging, pkg)?;

    match pkg.kind.as_str() {
        "ffmpeg" => {
            let staged_ffmpeg = staging.join(exe_name("ffmpeg"));
            let staged_ffprobe = staging.join(exe_name("ffprobe"));
            let inspection = setup::inspect_file(&staged_ffmpeg.to_string_lossy());
            if !inspection.exists || inspection.version.is_empty() {
                return Err("解压出的 ffmpeg 无法运行（版本读取失败），已拒绝落位".into());
            }
            flavor.accepts_build(inspection.gpl, inspection.nonfree, false)?;
            progress(&Progress {
                kind: pkg.kind.clone(),
                package_id: pkg.id.clone(),
                phase: "land".into(),
                source_id: String::new(),
                source_label: String::new(),
                source_index: 0,
                source_total: 0,
                attempt: 0,
                received: pkg.size,
                total: pkg.size,
                message: format!("构建配置校验通过（{}）", inspection.version),
            });
            let dest =
                unique_dir(&tools_dir.join(format!("ffmpeg-{}", safe_version(&pkg.version))));
            std::fs::create_dir_all(&dest).map_err(|e| format!("创建落位目录失败：{e}"))?;
            let ffmpeg = dest.join(exe_name("ffmpeg"));
            let ffprobe = dest.join(exe_name("ffprobe"));
            std::fs::copy(&staged_ffmpeg, &ffmpeg).map_err(|e| format!("复制 ffmpeg 失败：{e}"))?;
            if staged_ffprobe.is_file() {
                std::fs::copy(&staged_ffprobe, &ffprobe)
                    .map_err(|e| format!("复制 ffprobe 失败：{e}"))?;
            }
            make_executable(&ffmpeg);
            make_executable(&ffprobe);
            // 落位后复核：复制过程出错不能被当成成功。
            let copied = setup::hash_file(&ffmpeg).map_err(|e| format!("复核落位文件失败：{e}"))?;
            if copied != inspection.sha256 {
                std::fs::remove_dir_all(&dest).ok();
                return Err("落位后哈希与源文件不一致，已回滚".into());
            }
            let info = tools::describe_from_paths(
                &ffmpeg,
                if ffprobe.is_file() {
                    Some(ffprobe.as_path())
                } else {
                    None
                },
                "app-data",
            );
            std::fs::remove_dir_all(staging).ok();
            Ok(LandedTool {
                kind: pkg.kind.clone(),
                dir: dest.to_string_lossy().to_string(),
                version: pkg.version.clone(),
                archive_sha256: digest.to_string(),
                tools: Some(info),
            })
        }
        other => Err(format!("未知的工具类型：{other}")),
    }
}

// ---------------------------------------------------------------- Tauri 命令

use crate::jobs::AppState;
use std::sync::Arc;

#[tauri::command]
pub async fn tools_download_plan() -> Result<ToolsPlan, String> {
    plan_for(
        Flavor::current(),
        current_platform(),
        std::env::consts::ARCH,
    )
}

#[tauri::command]
pub async fn download_tool(
    app: AppHandle,
    state: State<'_, Arc<AppState>>,
    package_id: String,
    source_id: Option<String>,
) -> Result<LandedTool, String> {
    let app_data = app
        .path()
        .app_data_dir()
        .map_err(|e| format!("无法定位应用数据目录：{e}"))?;
    let flavor = Flavor::current();
    let emitter = app.clone();
    let result = tokio::task::spawn_blocking(move || {
        download_blocking(&app_data, flavor, &package_id, source_id.as_deref(), |p| {
            let _ = emitter.emit(PROGRESS_EVENT, p);
        })
    })
    .await
    .map_err(|e| format!("下载线程失败：{e}"))??;
    if let Some(info) = result.tools.clone() {
        *state.tools.lock().await = Some(info);
    }
    Ok(result)
}

#[tauri::command]
pub fn cancel_tool_download() {
    cancel();
}

// ---------------------------------------------------------------- 离线断言

#[cfg(test)]
mod tests {
    use super::*;

    fn is_hex64(value: &str) -> bool {
        value.len() == 64 && value.chars().all(|c| c.is_ascii_hexdigit())
    }

    #[test]
    fn manifest_is_internally_consistent() {
        let m = manifest().expect("内置清单必须能解析");
        assert_eq!(m.schema, 1);
        assert!(m
            .sources
            .iter()
            .any(|s| s.id == "official" && s.base.is_none()));
        assert!(m.sources.iter().filter(|s| s.base.is_some()).count() >= 2);
        for pkg in &m.packages {
            assert!(
                is_hex64(&pkg.sha256),
                "{} 的 sha256 不是 64 位十六进制",
                pkg.id
            );
            assert!(pkg.size > 1_000_000, "{} 的大小异常", pkg.id);
            assert!(
                pkg.url.starts_with("https://github.com/"),
                "{} 的官方源必须是 GitHub Release",
                pkg.id
            );
            assert!(
                pkg.url.contains(&pkg.file),
                "{} 的 URL 必须指向清单里的文件名",
                pkg.id
            );
            assert!(matches!(pkg.kind.as_str(), "ffmpeg"));
            assert!(matches!(pkg.flavor.as_str(), "lgpl" | "gpl" | "any"));
            assert!(
                !pkg.extract.is_empty() || !pkg.extract_trees.is_empty(),
                "{} 没有解压规则",
                pkg.id
            );
        }
        let ids: Vec<_> = m.packages.iter().map(|p| p.id.as_str()).collect();
        let mut seen = std::collections::HashSet::new();
        for id in &ids {
            assert!(seen.insert(*id), "下载包 id 重复：{id}");
        }
    }

    #[test]
    fn plan_respects_flavor_and_platform() {
        let lgpl = plan_for(Flavor::Lgpl, "windows", "x86_64").unwrap();
        assert!(lgpl
            .entries
            .iter()
            .any(|e| e.package_id == "ffmpeg-windows-x64-lgpl"));
        assert!(!lgpl
            .entries
            .iter()
            .any(|e| e.package_id == "ffmpeg-windows-x64-gpl"));
        let gpl = plan_for(Flavor::Gpl, "windows", "x86_64").unwrap();
        assert!(gpl
            .entries
            .iter()
            .any(|e| e.package_id == "ffmpeg-windows-x64-gpl"));
        assert!(!gpl
            .entries
            .iter()
            .any(|e| e.package_id == "ffmpeg-windows-x64-lgpl"));
        for entry in &lgpl.entries {
            assert!(
                entry.sources.len() >= 2,
                "{} 至少要有官方源 + 一个镜像",
                entry.package_id
            );
            assert_eq!(entry.sources[0].source_id, "official");
            assert!(!entry.sources[0].mirror);
            assert!(entry.sources.iter().skip(1).all(|s| s.mirror));
        }
    }

    #[test]
    fn platform_without_pinned_package_returns_manual_reason() {
        for platform in ["macos", "linux"] {
            for flavor in [Flavor::Lgpl, Flavor::Gpl] {
                let plan = plan_for(flavor, platform, "x86_64").unwrap();
                assert!(
                    plan.entries.iter().all(|e| e.kind != "ffmpeg"),
                    "{platform}/{flavor:?} 不应有 FFmpeg 自动下载包"
                );
                let manual: Vec<_> = plan.manual.iter().filter(|m| m.kind == "ffmpeg").collect();
                assert_eq!(manual.len(), 1, "{platform}/{flavor:?} 缺少手动原因");
                assert!(!manual[0].reason.trim().is_empty());
                assert!(manual[0].reason.contains("不提供自动下载"));
            }
        }
    }

    #[test]
    fn wrong_variant_package_is_rejected_before_download() {
        let m = manifest().unwrap();
        let lgpl = package_by_id(m, "ffmpeg-windows-x64-lgpl").unwrap();
        let err = check_package(lgpl, Flavor::Gpl, "windows", "x86_64").unwrap_err();
        assert!(err.contains("不匹配"));
        let gpl = package_by_id(m, "ffmpeg-windows-x64-gpl").unwrap();
        let err = check_package(gpl, Flavor::Lgpl, "windows", "x86_64").unwrap_err();
        assert!(err.contains("不匹配"));
        // 平台不符也必须在下载前拒绝。
        assert!(check_package(lgpl, Flavor::Lgpl, "linux", "x86_64").is_err());
    }

    #[test]
    fn checksum_mismatch_is_rejected_and_nothing_is_kept() {
        let dir = std::env::temp_dir().join(format!("transmutio-verify-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("fake.zip");
        std::fs::write(&file, b"not a real archive").unwrap();
        let err = verify_file(&file, &"0".repeat(64), 3).unwrap_err();
        assert!(err.contains("大小不符"), "{err}");
        let err = verify_file(&file, &"0".repeat(64), 18).unwrap_err();
        assert!(err.contains("SHA-256 校验失败"), "{err}");
        assert!(err.contains("拒绝落盘"), "{err}");
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn checksum_pass_reports_digest() {
        let dir = std::env::temp_dir().join(format!("transmutio-verify-ok-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("ok.bin");
        std::fs::write(&file, b"abc").unwrap();
        // sha256("abc") = ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad
        let digest = verify_file(
            &file,
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad",
            3,
        )
        .unwrap();
        assert_eq!(digest.len(), 64);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn disk_space_shortage_is_readable_and_blocks_start() {
        let dir = std::env::temp_dir().join(format!("transmutio-space-{}", std::process::id()));
        let err = ensure_space(&dir, u64::MAX / 2).unwrap_err();
        assert!(err.contains("磁盘空间不足"), "{err}");
        assert!(err.contains("不会开始下载"), "{err}");
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn resume_actions_cover_server_behaviors() {
        assert_eq!(resume_action(200, 0, 100), ResumeAction::Restart);
        assert_eq!(resume_action(200, 40, 100), ResumeAction::Restart);
        assert_eq!(resume_action(206, 40, 100), ResumeAction::Append);
        assert_eq!(resume_action(206, 0, 100), ResumeAction::Restart);
        assert_eq!(resume_action(416, 40, 100), ResumeAction::DropAndRestart);
        assert_eq!(resume_action(206, 100, 100), ResumeAction::Complete);
        assert_eq!(resume_action(200, 100, 100), ResumeAction::Complete);
    }

    #[test]
    fn source_order_prefers_selected_then_official_then_mirrors() {
        let m = manifest().unwrap();
        let order = ordered_sources(&m.sources, Some("ghproxy"));
        assert_eq!(order[0].id, "ghproxy");
        assert_eq!(order[1].id, "official");
        let order = ordered_sources(&m.sources, Some("不存在"));
        assert_eq!(order[0].id, "official");
        let order = ordered_sources(&m.sources, None);
        assert_eq!(order.len(), m.sources.len());
        assert_eq!(order[0].id, "official");
    }

    #[test]
    fn mirror_urls_are_prefix_plus_official_url() {
        let m = manifest().unwrap();
        let pkg = package_by_id(m, "ffmpeg-windows-x64-lgpl").unwrap();
        let official = m.sources.iter().find(|s| s.id == "official").unwrap();
        assert_eq!(source_url(official, &pkg.url), pkg.url);
        let mirror = m.sources.iter().find(|s| s.id == "ghfast").unwrap();
        let url = source_url(mirror, &pkg.url);
        assert!(url.starts_with("https://ghfast.top/https://github.com/BtbN/"));
        assert!(url.ends_with(&pkg.file));
    }
}
