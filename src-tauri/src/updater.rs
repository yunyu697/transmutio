//! 更新器：检查（多镜像按序回退）→ 下载（断点续传 + 强制校验）→ 安装/重启。
//!
//! 交互口径（与设置页一致）：
//! - 只有一个「检查更新」入口；未检查时只显示「检查更新」与「打开主页」；
//! - 只有确实发现新版本才给出「更新」与「仅下载」，并显示版本号、包大小、来源；
//! - 「更新」= 下载 → 校验 → 启动安装 → 退出应用（安装器完成后重启）；
//! - 「仅下载」= 落到默认下载目录，按钮变「安装」，安装前再做一次大小 + 哈希 + 签名校验；
//! - 已下载且校验通过的包即使随后检查为「已是最新」也保留「安装」（状态持久化在 pending.json）。
//!
//! 签名体系沿用既有方案：minisign（更新包）+ Authenticode（操作系统信任，未启用）。
//! 公钥为空时不做任何降级：可以检查/下载，但安装被明确拒绝。

use crate::flavor::Flavor;
use serde::{Deserialize, Serialize};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter, Manager};

pub const REPO: &str = "yunyu697/transmutio";
pub const HOMEPAGE: &str = "https://github.com/yunyu697/transmutio";
const API_PATH: &str = "https://api.github.com/repos/yunyu697/transmutio/releases?per_page=30";
const DOWNLOAD_BASE: &str = "https://github.com/yunyu697/transmutio/releases/download";

/// 编译期公钥：空/注释即未配置。配置后发布流程需同时提供 `.sig` 附件。
pub const UPDATER_PUBKEY: &str = include_str!("../updater.pub");

pub const PROGRESS_EVENT: &str = "update-progress";
const ATTEMPTS_PER_SOURCE: u32 = 2;
const TEMP_PREFIX: &str = ".transmutio-update-";

// ---------------------------------------------------------------- 镜像

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Mirror {
    pub id: String,
    pub label: String,
    /// 空串 = 官方直连；否则为拼在官方 URL 前面的加速前缀。
    pub prefix: String,
    pub official: bool,
}

fn mirror(id: &str, label: &str, prefix: &str) -> Mirror {
    Mirror {
        id: id.into(),
        label: label.into(),
        prefix: prefix.into(),
        official: prefix.is_empty(),
    }
}

/// 官方 + 若干加速镜像。镜像只换传输通道，任何来源的文件都要过同一套校验。
pub fn mirrors() -> Vec<Mirror> {
    vec![
        mirror("github", "GitHub 官方", ""),
        mirror("gh-proxy", "gh-proxy 加速", "https://gh-proxy.com/"),
        mirror("ghfast", "ghfast 加速", "https://ghfast.top/"),
        mirror("ghproxy-net", "ghproxy.net 加速", "https://ghproxy.net/"),
        mirror("mirror-ghproxy", "mirror.ghproxy 加速", "https://mirror.ghproxy.com/"),
    ]
}

/// 回退顺序：用户所选镜像 → GitHub 官方 → 其余镜像。
pub fn ordered_mirrors(preferred: Option<&str>) -> Vec<Mirror> {
    let all = mirrors();
    let mut out: Vec<Mirror> = Vec::with_capacity(all.len());
    if let Some(id) = preferred {
        if let Some(found) = all.iter().find(|m| m.id == id) {
            out.push(found.clone());
        }
    }
    for m in &all {
        if m.official && !out.iter().any(|x| x.id == m.id) {
            out.push(m.clone());
        }
    }
    for m in all {
        if !out.iter().any(|x| x.id == m.id) {
            out.push(m);
        }
    }
    out
}

// ---------------------------------------------------------------- 数据结构

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceAttempt {
    pub id: String,
    pub label: String,
    pub ok: bool,
    pub elapsed_ms: u64,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateCandidate {
    pub version: String,
    pub tag: String,
    pub notes: String,
    pub published_at: String,
    pub asset_name: String,
    pub asset_size: u64,
    pub asset_sha256: Option<String>,
    pub asset_url: String,
    /// `SHA256SUMS` 直链（API 没给 digest 时用它取哈希）。
    pub sums_url: Option<String>,
    /// `.sig` 直链（Release 里存在时才有）。
    pub signature_url: Option<String>,
    pub source_id: String,
    pub source_label: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CheckResult {
    pub current_version: String,
    pub channel: String,
    pub platform: String,
    pub arch: String,
    pub available: bool,
    pub candidate: Option<UpdateCandidate>,
    pub attempts: Vec<SourceAttempt>,
    /// install | download-only
    pub install_mode: String,
    pub install_mode_label: String,
    pub signature_configured: bool,
    pub signature_note: String,
    pub homepage: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PendingUpdate {
    pub version: String,
    pub tag: String,
    pub asset_name: String,
    pub path: String,
    pub size: u64,
    pub sha256: String,
    pub source_id: String,
    pub source_label: String,
    pub signature_verified: bool,
    pub installable: bool,
    pub install_block_reason: Option<String>,
    pub downloaded_at: u64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DownloadResult {
    pub pending: PendingUpdate,
    pub attempts: Vec<SourceAttempt>,
    pub signature_verified: bool,
    pub installable: bool,
    pub install_block_reason: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InstallResult {
    pub launched: bool,
    pub method: String,
    pub message: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DownloadProgress {
    pub phase: String,
    pub source_id: String,
    pub source_label: String,
    pub source_index: usize,
    pub source_total: usize,
    pub received: u64,
    pub total: u64,
    pub percent: f64,
    pub message: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdaterInfo {
    pub current_version: String,
    pub channel: String,
    pub platform: String,
    pub arch: String,
    pub homepage: String,
    pub mirrors: Vec<Mirror>,
    pub install_mode: String,
    pub install_mode_label: String,
    pub signature_configured: bool,
    pub signature_note: String,
}

// ---------------------------------------------------------------- 版本号

/// 解析 `1.0.0-alpha5` / `v1.0.0`；返回 (主, 次, 补, 预发布标识)。
pub fn parse_version(raw: &str) -> Option<(u64, u64, u64, Option<String>)> {
    let text = raw.trim().trim_start_matches('v');
    let (core, pre) = match text.split_once('-') {
        Some((c, p)) if !p.is_empty() => (c, Some(p.to_string())),
        Some(_) => return None,
        None => (text, None),
    };
    let mut parts = core.split('.');
    let major = parts.next()?.parse().ok()?;
    let minor = parts.next()?.parse().ok()?;
    let patch = parts.next()?.parse().ok()?;
    if parts.next().is_some() {
        return None;
    }
    Some((major, minor, patch, pre))
}

/// 预发布标识比较：`alpha10` > `alpha9`（同前缀按数字比较），其余按 semver 规则。
fn cmp_pre(a: &str, b: &str) -> std::cmp::Ordering {
    use std::cmp::Ordering;
    let split = |s: &str| -> (String, Option<u64>) {
        let idx = s.find(|c: char| c.is_ascii_digit());
        match idx {
            Some(i) => (s[..i].to_string(), s[i..].parse().ok()),
            None => (s.to_string(), None),
        }
    };
    let (pa, na) = split(a);
    let (pb, nb) = split(b);
    match pa.cmp(&pb) {
        Ordering::Equal => match (na, nb) {
            (Some(x), Some(y)) => x.cmp(&y),
            (Some(_), None) => Ordering::Greater,
            (None, Some(_)) => Ordering::Less,
            (None, None) => Ordering::Equal,
        },
        other => other,
    }
}

pub fn is_newer(candidate: &str, current: &str) -> bool {
    match (parse_version(candidate), parse_version(current)) {
        (Some((ma, mi, pa, pra)), Some((mb, mib, pb, prb))) => {
            if (ma, mi, pa) != (mb, mib, pb) {
                return (ma, mi, pa) > (mb, mib, pb);
            }
            match (pra, prb) {
                // 正式版高于任何预发布。
                (None, Some(_)) => true,
                (Some(_), None) => false,
                (Some(x), Some(y)) => cmp_pre(&x, &y) == std::cmp::Ordering::Greater,
                (None, None) => false,
            }
        }
        _ => false,
    }
}

// ---------------------------------------------------------------- 平台/变体

fn platform_tag() -> &'static str {
    if cfg!(windows) {
        "windows"
    } else if cfg!(target_os = "macos") {
        "macos"
    } else {
        "linux"
    }
}

fn arch_tag() -> &'static str {
    if cfg!(target_arch = "aarch64") {
        "aarch64"
    } else if cfg!(target_arch = "x86_64") {
        "x86_64"
    } else {
        std::env::consts::ARCH
    }
}

fn asset_has_flavor(name: &str, flavor: Flavor) -> bool {
    match flavor {
        Flavor::Lgpl => name.starts_with("Transmutio_"),
        Flavor::Gpl => {
            name.starts_with("Transmutio.GPL_") || name.starts_with("Transmutio GPL_")
        }
    }
}

fn asset_has_platform(name: &str) -> bool {
    if cfg!(windows) {
        name.ends_with("_x64-setup.exe") || name.ends_with("_x64_en-US.msi")
    } else if cfg!(target_os = "macos") {
        name.ends_with(".dmg") && name.contains(arch_tag())
    } else {
        name.ends_with(".AppImage") || name.ends_with(".deb")
    }
}

fn asset_rank(name: &str) -> u8 {
    // 数字越小越优先：Windows 用 NSIS（可静默安装），Linux 用 AppImage（可自替换）。
    if cfg!(windows) {
        if name.ends_with("_x64-setup.exe") {
            return 0;
        }
        return 1;
    }
    if cfg!(target_os = "macos") {
        return 0;
    }
    if name.ends_with(".AppImage") {
        0
    } else {
        1
    }
}

/// 三平台能力边界：返回 (install_mode, 文案)。
pub fn install_mode() -> (&'static str, &'static str) {
    if cfg!(windows) {
        ("install", "Windows：下载 → 校验 → 启动安装器 → 应用退出，安装完成自动重启")
    } else if cfg!(target_os = "macos") {
        (
            "download-only",
            "macOS：检查 + 下载；打开 DMG 手动拖入「应用程序」，应用不会自行替换",
        )
    } else if std::env::var_os("APPIMAGE").is_some() {
        (
            "install",
            "Linux AppImage：下载 → 校验 → 替换当前 AppImage → 重启（deb 版仍为检查 + 下载）",
        )
    } else {
        (
            "download-only",
            "Linux（deb/发行包）：检查 + 下载；安装由包管理器完成，应用不会自行替换",
        )
    }
}

// ---------------------------------------------------------------- 公钥与签名

pub fn pubkey_text() -> Option<String> {
    let line = UPDATER_PUBKEY
        .lines()
        .map(str::trim)
        .find(|l| !l.is_empty() && !l.starts_with('#'))?;
    if line.is_empty() {
        None
    } else {
        Some(line.to_string())
    }
}

pub fn signature_note() -> String {
    if pubkey_text().is_some() {
        "已配置 minisign 更新公钥：安装前会校验大小、SHA-256 与签名。".into()
    } else {
        "未配置 minisign 更新公钥（src-tauri/updater.pub）：可以检查/下载，但安装被拒绝，不会降级跳过校验。"
            .into()
    }
}

/// 用指定公钥验证 minisign 签名；流式优先，旧格式回退到整文件读取。
pub fn verify_signature_with(pubkey_b64: &str, path: &Path, sig_text: &str) -> Result<(), String> {
    let pk = minisign_verify::PublicKey::from_base64(pubkey_b64)
        .map_err(|e| format!("公钥无法解析：{e}"))?;
    let sig = minisign_verify::Signature::decode(sig_text)
        .map_err(|e| format!("签名文件无法解析：{e}"))?;
    match pk.verify_stream(&sig) {
        Ok(mut verifier) => {
            let mut file = std::fs::File::open(path).map_err(|e| format!("打开更新包失败：{e}"))?;
            let mut buf = vec![0u8; 1024 * 1024];
            loop {
                let n = file
                    .read(&mut buf)
                    .map_err(|e| format!("读取更新包失败：{e}"))?;
                if n == 0 {
                    break;
                }
                verifier.update(&buf[..n]);
            }
            verifier.finalize().map_err(|e| format!("签名校验失败：{e}"))
        }
        Err(minisign_verify::Error::UnsupportedLegacyMode) => {
            let bytes = std::fs::read(path).map_err(|e| format!("读取更新包失败：{e}"))?;
            pk.verify(&bytes, &sig, true)
                .map_err(|e| format!("签名校验失败：{e}"))
        }
        Err(e) => Err(format!("签名校验无法开始：{e}")),
    }
}

pub fn verify_signature(path: &Path, sig_text: &str) -> Result<(), String> {
    let key = pubkey_text()
        .ok_or_else(|| "未配置更新签名公钥（src-tauri/updater.pub），按设计拒绝安装".to_string())?;
    verify_signature_with(&key, path, sig_text)
}

// ---------------------------------------------------------------- HTTP

fn agent() -> ureq::Agent {
    ureq::AgentBuilder::new()
        .timeout_connect(Duration::from_secs(10))
        .timeout_read(Duration::from_secs(30))
        .timeout_write(Duration::from_secs(30))
        .user_agent(concat!("Transmutio/", env!("CARGO_PKG_VERSION")))
        .build()
}

fn net_error(error: &ureq::Error) -> String {
    match error {
        ureq::Error::Status(code, _) => {
            if *code == 403 || *code == 429 {
                format!("服务器返回 HTTP {code}（GitHub API 限流，稍后重试或换镜像）")
            } else {
                format!("服务器返回 HTTP {code}")
            }
        }
        ureq::Error::Transport(t) => format!("网络错误：{t}"),
    }
}

fn http_text(url: &str) -> Result<String, String> {
    let response = agent()
        .get(url)
        .set("Accept", "application/vnd.github+json")
        .set("X-GitHub-Api-Version", "2022-11-28")
        .set("Accept-Encoding", "identity")
        .call()
        .map_err(|e| net_error(&e))?;
    if response.status() != 200 {
        return Err(format!("服务器返回 HTTP {}", response.status()));
    }
    response
        .into_string()
        .map_err(|e| format!("读取响应失败：{e}"))
}

// ---------------------------------------------------------------- 检查

fn release_asset<'a>(
    assets: &'a serde_json::Value,
    name: &str,
) -> Option<&'a serde_json::Value> {
    assets
        .as_array()?
        .iter()
        .find(|a| a.get("name").and_then(|v| v.as_str()) == Some(name))
}

fn pick_candidate(
    feed: &serde_json::Value,
    current: &str,
    flavor: Flavor,
    source: &Mirror,
) -> Option<UpdateCandidate> {
    let releases = feed.as_array()?;
    let mut best: Option<UpdateCandidate> = None;
    for release in releases {
        if release.get("draft").and_then(|v| v.as_bool()) == Some(true) {
            continue;
        }
        let tag = release.get("tag_name").and_then(|v| v.as_str()).unwrap_or("");
        let version = tag.trim_start_matches('v');
        if parse_version(version).is_none() || !is_newer(version, current) {
            continue;
        }
        let assets = match release.get("assets") {
            Some(v) => v,
            None => continue,
        };
        let mut chosen: Option<&serde_json::Value> = None;
        let mut chosen_rank = u8::MAX;
        for asset in assets.as_array().into_iter().flatten() {
            let name = asset.get("name").and_then(|v| v.as_str()).unwrap_or("");
            if !asset_has_flavor(name, flavor) || !asset_has_platform(name) {
                continue;
            }
            if !name.contains(version) {
                continue;
            }
            let rank = asset_rank(name);
            if rank < chosen_rank {
                chosen = Some(asset);
                chosen_rank = rank;
            }
        }
        let asset = match chosen {
            Some(a) => a,
            None => continue,
        };
        let name = asset
            .get("name")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let digest = asset
            .get("digest")
            .and_then(|v| v.as_str())
            .map(|d| d.trim_start_matches("sha256:").to_lowercase());
        let asset_url = asset
            .get("browser_download_url")
            .and_then(|v| v.as_str())
            .map(str::to_string)
            .unwrap_or_else(|| format!("{DOWNLOAD_BASE}/{tag}/{name}"));
        let sums_name = "SHA256SUMS";
        let sums_url = release_asset(assets, sums_name).and_then(|a| {
            a.get("browser_download_url")
                .and_then(|v| v.as_str())
                .map(str::to_string)
        });
        let signature_url = release_asset(assets, &format!("{name}.sig")).and_then(|a| {
            a.get("browser_download_url")
                .and_then(|v| v.as_str())
                .map(str::to_string)
        });
        let candidate = UpdateCandidate {
            version: version.to_string(),
            tag: tag.to_string(),
            notes: release
                .get("body")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string(),
            published_at: release
                .get("published_at")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string(),
            asset_size: asset.get("size").and_then(|v| v.as_u64()).unwrap_or(0),
            asset_sha256: digest,
            asset_name: name,
            asset_url,
            sums_url,
            signature_url,
            source_id: source.id.clone(),
            source_label: source.label.clone(),
        };
        let replace = match &best {
            Some(b) => is_newer(&candidate.version, &b.version),
            None => true,
        };
        if replace {
            best = Some(candidate);
        }
    }
    best
}

pub fn info() -> UpdaterInfo {
    let (mode, label) = install_mode();
    UpdaterInfo {
        current_version: env!("CARGO_PKG_VERSION").to_string(),
        channel: Flavor::current().update_channel().to_string(),
        platform: platform_tag().to_string(),
        arch: arch_tag().to_string(),
        homepage: HOMEPAGE.to_string(),
        mirrors: mirrors(),
        install_mode: mode.to_string(),
        install_mode_label: label.to_string(),
        signature_configured: pubkey_text().is_some(),
        signature_note: signature_note(),
    }
}

pub fn check_blocking(preferred: Option<&str>) -> CheckResult {
    check_from(env!("CARGO_PKG_VERSION"), preferred)
}

/// `current` 可显式指定，便于无窗口检查程序验证「发现新版本」分支。
pub fn check_from(current: &str, preferred: Option<&str>) -> CheckResult {
    let current = current.to_string();
    let flavor = Flavor::current();
    let ordered = ordered_mirrors(preferred);
    let total = ordered.len();
    let mut attempts = Vec::with_capacity(total);
    let mut candidate: Option<UpdateCandidate> = None;

    for (index, mirror) in ordered.iter().enumerate() {
        let started = Instant::now();
        let url = format!("{}{}", mirror.prefix, API_PATH);
        match http_text(&url).and_then(|text| {
            serde_json::from_str::<serde_json::Value>(&text)
                .map_err(|e| format!("更新清单解析失败：{e}"))
        }) {
            Ok(feed) => {
                let elapsed_ms = started.elapsed().as_millis() as u64;
                attempts.push(SourceAttempt {
                    id: mirror.id.clone(),
                    label: mirror.label.clone(),
                    ok: true,
                    elapsed_ms,
                    error: None,
                });
                if let Some(found) = pick_candidate(&feed, &current, flavor, mirror) {
                    candidate = Some(found);
                    break;
                }
                // 该源可用但已是最新：不需要再问其他源。
                break;
            }
            Err(error) => {
                let elapsed_ms = started.elapsed().as_millis() as u64;
                attempts.push(SourceAttempt {
                    id: mirror.id.clone(),
                    label: mirror.label.clone(),
                    ok: false,
                    elapsed_ms,
                    error: Some(error),
                });
                let _ = index;
            }
        }
    }

    let (mode, mode_label) = install_mode();
    CheckResult {
        current_version: current,
        channel: flavor.update_channel().to_string(),
        platform: platform_tag().to_string(),
        arch: arch_tag().to_string(),
        available: candidate.is_some(),
        candidate,
        attempts,
        install_mode: mode.to_string(),
        install_mode_label: mode_label.to_string(),
        signature_configured: pubkey_text().is_some(),
        signature_note: signature_note(),
        homepage: HOMEPAGE.to_string(),
    }
}

// ---------------------------------------------------------------- 路径与校验

fn downloads_dir(app: &AppHandle) -> PathBuf {
    app.path()
        .download_dir()
        .or_else(|_| app.path().home_dir())
        .unwrap_or_else(|_| std::env::temp_dir())
}

fn pending_path(app: &AppHandle) -> PathBuf {
    app.path()
        .app_data_dir()
        .map(|p| p.join("updates").join("pending.json"))
        .unwrap_or_else(|_| std::env::temp_dir().join("transmutio-updates").join("pending.json"))
}

fn read_pending(app: &AppHandle) -> Option<PendingUpdate> {
    let text = std::fs::read_to_string(pending_path(app)).ok()?;
    serde_json::from_str(&text).ok()
}

fn write_pending(app: &AppHandle, pending: &PendingUpdate) -> Result<(), String> {
    let path = pending_path(app);
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| format!("创建更新目录失败：{e}"))?;
    }
    let text = serde_json::to_string_pretty(pending).map_err(|e| format!("序列化失败：{e}"))?;
    std::fs::write(&path, text).map_err(|e| format!("写入更新状态失败：{e}"))
}

fn remove_pending(app: &AppHandle) {
    std::fs::remove_file(pending_path(app)).ok();
}

/// 启动清理：删除所有中断残留的临时文件；过期 pending（版本不高于当前）连同安装包一起清掉。
pub fn startup_cleanup(app: &AppHandle) {
    let downloads = downloads_dir(app);
    for dir in [downloads.clone(), pending_path(app).parent().map(|p| p.to_path_buf()).unwrap_or_else(|| downloads)] {
        let entries = match std::fs::read_dir(&dir) {
            Ok(v) => v,
            Err(_) => continue,
        };
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            if name.starts_with(TEMP_PREFIX) && name.ends_with(".part") {
                std::fs::remove_file(entry.path()).ok();
            }
        }
    }
    if let Some(pending) = read_pending(app) {
        let stale = match parse_version(env!("CARGO_PKG_VERSION")) {
            Some(cur) => !is_newer(&pending.version, env!("CARGO_PKG_VERSION")) && parse_version(&pending.version) <= Some(cur),
            None => false,
        };
        if stale {
            std::fs::remove_file(&pending.path).ok();
            std::fs::remove_file(format!("{}.sig", pending.path)).ok();
            remove_pending(app);
            return;
        }
        if !Path::new(&pending.path).is_file() {
            remove_pending(app);
        }
    }
}

fn parse_sums(text: &str, asset_name: &str) -> Option<String> {
    for line in text.lines() {
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() < 2 {
            continue;
        }
        let joined = parts[1..].join(" ");
        if joined.trim_start_matches('*') == asset_name {
            return Some(parts[0].to_lowercase());
        }
    }
    None
}

fn unique_target(dir: &Path, name: &str) -> PathBuf {
    let target = dir.join(name);
    if !target.exists() {
        return target;
    }
    let (stem, ext) = match name.rsplit_once('.') {
        Some((s, e)) => (s, format!(".{e}")),
        None => (name, String::new()),
    };
    for i in 1..10_000 {
        let candidate = dir.join(format!("{stem} ({i}){ext}"));
        if !candidate.exists() {
            return candidate;
        }
    }
    dir.join(format!("{stem}-{}", std::process::id()))
}

// ---------------------------------------------------------------- 下载

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
    if status == 416 {
        std::fs::remove_file(part).ok();
        return Err("服务器拒绝续传范围（临时文件已丢弃，将重试）".into());
    }
    let append = status == 206 && existing > 0;
    let mut file = if append {
        std::fs::OpenOptions::new().create(true).append(true).open(part)
    } else {
        std::fs::OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .open(part)
    }
    .map_err(|e| format!("创建临时文件失败：{e}"))?;
    let mut reader = response.into_reader();
    let mut buf = vec![0u8; 256 * 1024];
    let mut received = if append { existing } else { 0 };
    loop {
        let n = reader
            .read(&mut buf)
            .map_err(|e| format!("下载中断：{e}"))?;
        if n == 0 {
            break;
        }
        file.write_all(&buf[..n])
            .map_err(|e| format!("写入临时文件失败：{e}"))?;
        received += n as u64;
        on_bytes(received);
    }
    file.flush().ok();
    Ok(())
}

fn download_file<F>(
    mirror: &Mirror,
    index: usize,
    total_sources: usize,
    url: &str,
    part: &Path,
    total: u64,
    mut on_progress: F,
) -> Result<(), String>
where
    F: FnMut(&str, usize, usize, u64, u64),
{
    let agent = agent();
    let mut last_error = String::new();
    for attempt in 1..=ATTEMPTS_PER_SOURCE {
        let existing = std::fs::metadata(part).map(|m| m.len()).unwrap_or(0);
        on_progress(
            if attempt == 1 { "download" } else { "retry" },
            index,
            total_sources,
            existing,
            total,
        );
        let result = fetch_once(&agent, url, part, total, |received| {
            on_progress("download", index, total_sources, received, total);
        });
        match result {
            Ok(()) => return Ok(()),
            Err(error) => {
                let _ = mirror;
                last_error = error;
            }
        }
    }
    Err(last_error)
}

fn fetch_signature(mirror: &Mirror, signature_url: &str) -> Result<String, String> {
    let url = format!("{}{}", mirror.prefix, signature_url);
    http_text(&url).map_err(|e| format!("拉取签名失败：{e}"))
}

fn fetch_sums(mirror: &Mirror, sums_url: &str, asset_name: &str) -> Result<String, String> {
    let url = format!("{}{}", mirror.prefix, sums_url);
    let text = http_text(&url).map_err(|e| format!("拉取 SHA256SUMS 失败：{e}"))?;
    parse_sums(&text, asset_name)
        .ok_or_else(|| format!("SHA256SUMS 中没有 {asset_name} 的条目"))
}

/// 下载请求（与 UI / 检查结果解耦，便于无窗口检查程序复用）。
#[derive(Debug, Clone)]
pub struct DownloadSpec {
    pub version: String,
    pub tag: String,
    pub asset_name: String,
    pub asset_size: u64,
    pub asset_sha256: Option<String>,
    pub asset_url: String,
    pub sums_url: Option<String>,
    pub signature_url: Option<String>,
}

#[derive(Debug, Clone)]
pub struct HeadlessDownload {
    pub path: PathBuf,
    pub sha256: String,
    pub source_id: String,
    pub source_label: String,
    pub signature_verified: bool,
    pub attempts: Vec<SourceAttempt>,
}

/// 不带 Tauri 的下载主流程：多源回退 → 落临时文件 → 大小 + 哈希（+ 签名）→ 原子改名。
pub fn download_headless<F>(
    dir: &Path,
    spec: &DownloadSpec,
    preferred: Option<&str>,
    mut progress: F,
) -> Result<HeadlessDownload, String>
where
    F: FnMut(&str, usize, usize, u64, u64, &str),
{
    let signature_configured = pubkey_text().is_some();
    crate::download::ensure_space(dir, spec.asset_size.saturating_add(8 * 1024 * 1024))?;
    std::fs::create_dir_all(dir).ok();
    let ordered = ordered_mirrors(preferred);
    let total_sources = ordered.len();
    let temp = dir.join(format!(
        "{TEMP_PREFIX}{}.part",
        spec.asset_name.replace(['/', '\\', ' '], "_")
    ));

    let mut attempts = Vec::with_capacity(total_sources);
    let mut landed: Option<(String, String, String, bool)> = None;

    for (index, mirror) in ordered.iter().enumerate() {
        let started = Instant::now();
        let url = format!("{}{}", mirror.prefix, spec.asset_url);
        let result = (|| -> Result<(String, bool), String> {
            download_file(mirror, index, total_sources, &url, &temp, spec.asset_size, |phase, i, t, received, total| {
                progress(phase, i, t, received, total, &mirror.label);
            })?;
            let sha = match &spec.asset_sha256 {
                Some(v) if !v.trim().is_empty() => {
                    crate::download::verify_file(&temp, v, spec.asset_size)?.to_lowercase()
                }
                _ => {
                    let sums = match &spec.sums_url {
                        Some(u) => fetch_sums(mirror, u, &spec.asset_name)?,
                        None => {
                            return Err("该 Release 未提供 SHA-256（digest 与 SHA256SUMS 均缺失），已拒绝落盘"
                                .into())
                        }
                    };
                    crate::download::verify_file(&temp, &sums, spec.asset_size)?
                }
            };
            if !signature_configured {
                return Ok((sha, false));
            }
            let sig_url = spec.signature_url.as_deref().ok_or_else(|| {
                "该 Release 未提供 .sig 签名附件，已拒绝（不降级跳过校验）".to_string()
            })?;
            let sig_text = fetch_signature(mirror, sig_url)?;
            verify_signature(&temp, &sig_text)?;
            std::fs::write(format!("{}.sig", temp.display()), sig_text)
                .map_err(|e| format!("保存签名失败：{e}"))?;
            Ok((sha, true))
        })();

        match result {
            Ok((sha, signature_verified)) => {
                attempts.push(SourceAttempt {
                    id: mirror.id.clone(),
                    label: mirror.label.clone(),
                    ok: true,
                    elapsed_ms: started.elapsed().as_millis() as u64,
                    error: None,
                });
                landed = Some((
                    sha,
                    mirror.id.clone(),
                    mirror.label.clone(),
                    signature_verified,
                ));
                break;
            }
            Err(error) => {
                // 失败或换源：清理临时文件（含签名），不留下半成品。
                std::fs::remove_file(&temp).ok();
                std::fs::remove_file(format!("{}.sig", temp.display())).ok();
                attempts.push(SourceAttempt {
                    id: mirror.id.clone(),
                    label: mirror.label.clone(),
                    ok: false,
                    elapsed_ms: started.elapsed().as_millis() as u64,
                    error: Some(error),
                });
            }
        }
    }

    let (sha, source_id, source_label, signature_verified) = landed.ok_or_else(|| {
        let reasons: Vec<String> = attempts
            .iter()
            .filter_map(|a| a.error.as_ref().map(|e| format!("{}：{e}", a.label)))
            .collect();
        format!("所有更新源均失败，未下载任何文件。{}", reasons.join("；"))
    })?;

    let target = unique_target(dir, &spec.asset_name);
    std::fs::rename(&temp, &target).map_err(|e| format!("落盘失败：{e}"))?;
    let temp_sig = format!("{}.sig", temp.display());
    if signature_verified {
        std::fs::rename(&temp_sig, format!("{}.sig", target.display())).ok();
    } else {
        std::fs::remove_file(&temp_sig).ok();
    }

    Ok(HeadlessDownload {
        path: target,
        sha256: sha,
        source_id,
        source_label,
        signature_verified,
        attempts,
    })
}

// ---------------------------------------------------------------- 命令

#[tauri::command]
pub async fn update_info() -> UpdaterInfo {
    info()
}

#[tauri::command]
pub async fn update_check(preferred: Option<String>) -> CheckResult {
    tokio::task::spawn_blocking(move || check_blocking(preferred.as_deref()))
        .await
        .unwrap_or_else(|e| CheckResult {
            current_version: env!("CARGO_PKG_VERSION").to_string(),
            channel: Flavor::current().update_channel().to_string(),
            platform: platform_tag().to_string(),
            arch: arch_tag().to_string(),
            available: false,
            candidate: None,
            attempts: vec![SourceAttempt {
                id: "internal".into(),
                label: "本地".into(),
                ok: false,
                elapsed_ms: 0,
                error: Some(format!("检查任务失败：{e}")),
            }],
            install_mode: install_mode().0.to_string(),
            install_mode_label: install_mode().1.to_string(),
            signature_configured: pubkey_text().is_some(),
            signature_note: signature_note(),
            homepage: HOMEPAGE.to_string(),
        })
}

#[tauri::command]
pub async fn update_pending(app: AppHandle) -> Option<PendingUpdate> {
    read_pending(&app)
}

#[tauri::command]
pub async fn update_download(
    app: AppHandle,
    version: String,
    tag: String,
    asset_name: String,
    asset_size: u64,
    asset_sha256: Option<String>,
    asset_url: String,
    sums_url: Option<String>,
    signature_url: Option<String>,
    preferred: Option<String>,
) -> Result<DownloadResult, String> {
    let handle = app.clone();
    let task = tokio::task::spawn_blocking(move || {
        let spec = DownloadSpec {
            version,
            tag,
            asset_name,
            asset_size,
            asset_sha256,
            asset_url,
            sums_url,
            signature_url,
        };
        let dir = downloads_dir(&handle);
        let downloaded = download_headless(
            &dir,
            &spec,
            preferred.as_deref(),
            |phase, index, total_sources, received, total, label| {
                let percent = if total > 0 {
                    (received as f64 / total as f64) * 100.0
                } else {
                    0.0
                };
                let _ = handle.emit(
                    PROGRESS_EVENT,
                    DownloadProgress {
                        phase: phase.into(),
                        source_id: String::new(),
                        source_label: label.to_string(),
                        source_index: index,
                        source_total: total_sources,
                        received,
                        total,
                        percent,
                        message: format!("{label} · {received} / {total} 字节"),
                    },
                );
            },
        )?;
        let (mode, _) = install_mode();
        let install_block = if mode != "install" {
            Some(install_mode().1.to_string())
        } else if !downloaded.signature_verified {
            Some(
                "未配置更新签名公钥，无法完成签名校验：安装被拒绝（可以保留安装包，配置公钥后重新下载）"
                    .to_string(),
            )
        } else {
            None
        };
        let installable = install_block.is_none();
        let downloaded_at = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        let pending = PendingUpdate {
            version: spec.version,
            tag: spec.tag,
            asset_name: spec.asset_name,
            path: downloaded.path.to_string_lossy().to_string(),
            size: spec.asset_size,
            sha256: downloaded.sha256,
            source_id: downloaded.source_id,
            source_label: downloaded.source_label,
            signature_verified: downloaded.signature_verified,
            installable,
            install_block_reason: install_block.clone(),
            downloaded_at,
        };
        write_pending(&handle, &pending)?;
        let _ = handle.emit(
            PROGRESS_EVENT,
            DownloadProgress {
                phase: "done".into(),
                source_id: pending.source_id.clone(),
                source_label: pending.source_label.clone(),
                source_index: 0,
                source_total: mirrors().len(),
                received: pending.size,
                total: pending.size,
                percent: 100.0,
                message: "下载与校验完成".into(),
            },
        );
        Ok(DownloadResult {
            pending,
            attempts: downloaded.attempts,
            signature_verified: downloaded.signature_verified,
            installable,
            install_block_reason: install_block,
        })
    });
    task.await
        .map_err(|e| format!("下载任务失败：{e}"))?
}

#[tauri::command]
pub async fn update_install(app: AppHandle) -> Result<InstallResult, String> {
    let pending = read_pending(&app).ok_or_else(|| "没有已下载的更新包".to_string())?;
    let path = PathBuf::from(&pending.path);
    if !path.is_file() {
        remove_pending(&app);
        return Err("更新包已不存在（可能被移动或删除），请重新下载".into());
    }
    // 安装前二次校验：大小 + 哈希 + 签名。
    crate::download::verify_file(&path, &pending.sha256, pending.size)?;
    if pubkey_text().is_some() {
        let sig_path = format!("{}.sig", path.display());
        let sig = std::fs::read_to_string(&sig_path)
            .map_err(|_| "缺少签名文件，拒绝安装（请重新下载）".to_string())?;
        verify_signature(&path, &sig)?;
    } else {
        // 不降级：没有公钥就没有可信校验，一律拒绝安装/打开。
        return Err("未配置更新签名公钥（src-tauri/updater.pub），按设计拒绝安装".into());
    }
    let (mode, label) = install_mode();
    if mode != "install" {
        // macOS / Linux 发行包：打开安装包让用户自行安装，不假装自更新。
        open_path(&app, &path)?;
        return Ok(InstallResult {
            launched: false,
            method: "open".into(),
            message: label.to_string(),
        });
    }

    #[cfg(windows)]
    {
        use std::process::Command;
        let mut cmd = Command::new(&path);
        cmd.arg("/S");
        cmd.spawn()
            .map_err(|e| format!("启动安装器失败：{e}"))?;
        let handle = app.clone();
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(900));
            handle.exit(0);
        });
        return Ok(InstallResult {
            launched: true,
            method: "nsis-silent".into(),
            message: "安装器已启动，应用即将退出；安装完成后安装器会自动重启新版本".into(),
        });
    }

    #[cfg(target_os = "linux")]
    {
        if let Some(current) = std::env::var_os("APPIMAGE") {
            let current = PathBuf::from(current);
            let new_path = current.with_extension("new");
            std::fs::copy(&path, &new_path).map_err(|e| format!("写入新 AppImage 失败：{e}"))?;
            use std::os::unix::fs::PermissionsExt;
            let mut perms = std::fs::metadata(&new_path)
                .map_err(|e| format!("读取新 AppImage 权限失败：{e}"))?
                .permissions();
            perms.set_mode(0o755);
            std::fs::set_permissions(&new_path, perms).ok();
            std::fs::rename(&new_path, &current).map_err(|e| format!("替换 AppImage 失败：{e}"))?;
            std::process::Command::new(&current)
                .spawn()
                .map_err(|e| format!("重启失败：{e}"))?;
            let handle = app.clone();
            std::thread::spawn(move || {
                std::thread::sleep(Duration::from_millis(300));
                handle.exit(0);
            });
            return Ok(InstallResult {
                launched: true,
                method: "appimage-replace".into(),
                message: "已替换当前 AppImage，正在重启".into(),
            });
        }
        return Err("当前不是 AppImage 运行形态，无法自行替换".into());
    }

    #[allow(unreachable_code)]
    Err("当前平台不支持自动安装".into())
}

fn open_path(app: &AppHandle, path: &Path) -> Result<(), String> {
    use tauri_plugin_opener::OpenerExt;
    app.opener()
        .open_path(path.to_string_lossy().to_string(), None::<&str>)
        .map_err(|e| format!("打开更新包失败：{e}"))
}

#[tauri::command]
pub fn open_homepage(app: AppHandle) -> Result<(), String> {
    use tauri_plugin_opener::OpenerExt;
    app.opener()
        .open_url(HOMEPAGE.to_string(), None::<&str>)
        .map_err(|e| format!("打开主页失败：{e}"))
}

// ---------------------------------------------------------------- 测试

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_ordering_handles_alpha_numbers() {
        assert!(is_newer("1.0.0-alpha5", "1.0.0-alpha4"));
        assert!(is_newer("1.0.0-alpha10", "1.0.0-alpha9"));
        assert!(!is_newer("1.0.0-alpha4", "1.0.0-alpha5"));
        assert!(!is_newer("1.0.0-alpha5", "1.0.0-alpha5"));
        assert!(is_newer("1.0.0", "1.0.0-alpha5"));
        assert!(!is_newer("1.0.0-alpha5", "1.0.0"));
        assert!(is_newer("1.1.0", "1.0.9"));
        assert!(!is_newer("v1.0.0-alpha4", "1.0.0-alpha5"));
        assert_eq!(parse_version("v1.0.0-alpha5").unwrap().3.as_deref(), Some("alpha5"));
    }

    #[test]
    fn mirror_order_is_selected_then_official_then_rest() {
        let ordered = ordered_mirrors(Some("ghfast"));
        assert_eq!(ordered[0].id, "ghfast");
        assert_eq!(ordered[1].id, "github");
        assert!(ordered.iter().skip(2).all(|m| !m.official));
        let default = ordered_mirrors(None);
        assert_eq!(default[0].id, "github");
    }

    #[test]
    fn assets_are_filtered_by_flavor_and_platform() {
        assert!(asset_has_flavor("Transmutio_1.0.0-alpha4_x64-setup.exe", Flavor::Lgpl));
        assert!(!asset_has_flavor("Transmutio.GPL_1.0.0-alpha4_x64-setup.exe", Flavor::Lgpl));
        assert!(asset_has_flavor("Transmutio.GPL_1.0.0-alpha4_x64-setup.exe", Flavor::Gpl));
        assert!(asset_has_platform(if cfg!(windows) {
            "Transmutio_1.0.0-alpha4_x64-setup.exe"
        } else if cfg!(target_os = "macos") {
            "Transmutio_1.0.0-alpha4_aarch64.dmg"
        } else {
            "Transmutio_1.0.0-alpha4_amd64.AppImage"
        }));
    }

    #[test]
    fn sha256sums_parser_matches_asset_name() {
        let text = "aaa  Transmutio_1.0.0-alpha4_x64-setup.exe\nbbb  SHA256SUMS\n";
        assert_eq!(parse_sums(text, "Transmutio_1.0.0-alpha4_x64-setup.exe").as_deref(), Some("aaa"));
        assert_eq!(parse_sums(text, "missing"), None);
    }

    #[test]
    fn minisign_verification_accepts_valid_and_rejects_tampered() {
        // 来自 minisign-verify 的公开测试向量。
        let pk = "RWQf6LRCGA9i53mlYecO4IzT51TGPpvWucNSCh1CBM0QTaLn73Y7GFO3";
        let sig = "untrusted comment: signature from minisign secret key\n\
RUQf6LRCGA9i559r3g7V1qNyJDApGip8MfqcadIgT9CuhV3EMhHoN1mGTkUidF/z7SrlQgXdy8ofjb7bNJJylDOocrCo8KLzZwo=\n\
trusted comment: timestamp:1633700835\tfile:test\tprehashed\n\
wLMDjy9FLAuxZ3q4NlEvkgtyhrr0gtTu6KC4KBJdITbbOeAi1zBIYo0v4iTgt8jJpIidRJnp94ABQkJAgAooBQ==";
        let dir = std::env::temp_dir().join(format!("transmutio-sig-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("test.bin");
        std::fs::write(&file, b"test").unwrap();
        assert!(verify_signature_with(pk, &file, sig).is_ok());
        std::fs::write(&file, b"tampered").unwrap();
        assert!(verify_signature_with(pk, &file, sig).is_err());
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn candidate_picking_skips_current_and_drafts() {
        let feed = serde_json::json!([
            { "tag_name": "v1.0.0-alpha4", "draft": false, "assets": [] },
            { "tag_name": "v1.0.0-alpha6", "draft": true, "assets": [] }
        ]);
        let source = mirrors()[0].clone();
        assert!(pick_candidate(&feed, "1.0.0-alpha5", Flavor::Lgpl, &source).is_none());
    }
}
