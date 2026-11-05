//! 双变体验证（无窗口）：编译期策略、策略矩阵、数据目录隔离、真实 x264 转码。
//!
//! 同一个源码在同一台机器上分别以 `--features gpl` 与默认（LGPL）编译运行本检查：
//! - GPL 版：接受 GPL 构建，真实用 libx264 转出 MP4；
//! - LGPL 版：拒绝 GPL 构建，任务层也拒绝 GPL 编码器。
//!
//! 层级标注：
//!   * 「逻辑层」= 纯策略函数 + 真实文件系统落位，不涉及窗口；
//!   * 「真实转码」= 真实 ffmpeg 二进制 + 真实文件系统（无窗口）。

use crate::flavor::{self, Flavor};
use crate::jobs::{self, AppState, Ctx, Job, JobSpec};
use crate::probe::{Capabilities, DecodeCap, EncoderCap};
use crate::{setup, tools};
use serde::Serialize;
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

const LOGIC: &str = "逻辑层（编译期策略 + 真实文件系统，无窗口）";
const REAL: &str = "真实 ffmpeg + 真实文件系统（无窗口）";

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct Check {
    name: String,
    status: String,
    level: String,
    detail: String,
    evidence: Value,
}

#[derive(Debug, Clone, Serialize)]
struct Summary {
    passed: usize,
    failed: usize,
    skipped: usize,
    total: usize,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct Report {
    generated_at: i64,
    flavor: String,
    variant: Value,
    summary: Summary,
    results: Vec<Check>,
    unverified: Vec<String>,
}

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

struct Recorder {
    checks: Vec<Check>,
    passed: usize,
    failed: usize,
    skipped: usize,
}

impl Recorder {
    fn new() -> Self {
        Self {
            checks: Vec::new(),
            passed: 0,
            failed: 0,
            skipped: 0,
        }
    }
    fn pass(&mut self, name: &str, level: &str, detail: String, evidence: Value) {
        self.passed += 1;
        self.checks.push(Check {
            name: name.into(),
            status: "pass".into(),
            level: level.into(),
            detail,
            evidence,
        });
    }
    fn fail(&mut self, name: &str, level: &str, detail: String, evidence: Value) {
        self.failed += 1;
        self.checks.push(Check {
            name: name.into(),
            status: "fail".into(),
            level: level.into(),
            detail,
            evidence,
        });
    }
    fn skip(&mut self, name: &str, level: &str, detail: String) {
        self.skipped += 1;
        self.checks.push(Check {
            name: name.into(),
            status: "skip".into(),
            level: level.into(),
            detail,
            evidence: json!({}),
        });
    }
    fn judge(&mut self, name: &str, level: &str, ok: bool, detail: String, evidence: Value) {
        if ok {
            self.pass(name, level, detail, evidence);
        } else {
            self.fail(name, level, detail, evidence);
        }
    }
}

fn temp_files(dir: &Path) -> Vec<String> {
    std::fs::read_dir(dir)
        .map(|rd| {
            rd.filter_map(|e| e.ok())
                .map(|e| e.file_name().to_string_lossy().to_string())
                .filter(|n| n.starts_with(".ff-tmp-"))
                .collect()
        })
        .unwrap_or_default()
}

fn run_ffmpeg(ffmpeg: &str, args: &[&str]) -> Result<String, String> {
    let out = std::process::Command::new(ffmpeg)
        .args(args)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::piped())
        .output()
        .map_err(|e| format!("启动 ffmpeg 失败：{e}"))?;
    if out.status.success() {
        Ok(String::from_utf8_lossy(&out.stderr).to_string())
    } else {
        let tail: String = String::from_utf8_lossy(&out.stderr)
            .lines()
            .rev()
            .take(4)
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect::<Vec<_>>()
            .join(" | ");
        Err(format!("ffmpeg 退出码 {:?}：{tail}", out.status.code()))
    }
}

fn probe_codec(ffprobe: Option<&str>, file: &Path) -> Option<String> {
    let ffprobe = ffprobe?;
    let out = std::process::Command::new(ffprobe)
        .args([
            "-v",
            "error",
            "-select_streams",
            "v:0",
            "-show_entries",
            "stream=codec_name,encoder",
            "-of",
            "default=nw=1",
            &file.to_string_lossy(),
        ])
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    Some(String::from_utf8_lossy(&out.stdout).replace('\n', " "))
}

fn caps_with_gpl_encoders(version: &str) -> Capabilities {
    // 只放 GPL 软件编码器：LGPL 版应因此拿不到任何可用编码器。
    let mk = |id: &str, label: &str| EncoderCap {
        id: id.into(),
        codec: "h264".into(),
        label: label.into(),
        vendor: "software".into(),
        hardware: false,
        license: "gpl".into(),
        allowed: true,
        compiled: true,
        available: true,
        reason: None,
    };
    let x265 = EncoderCap {
        codec: "hevc".into(),
        ..mk("libx265", "x265 (HEVC)")
    };
    Capabilities {
        probed_at: now_ms(),
        cache_hit: false,
        fingerprint: "flavor-check-caps".into(),
        platform: std::env::consts::OS.into(),
        arch: std::env::consts::ARCH.into(),
        ffmpeg_version: version.into(),
        build_gpl: true,
        build_nonfree: false,
        video_encoders: vec![mk("libx264", "x264"), x265],
        audio_encoders: vec![],
        decode_accels: vec![DecodeCap {
            id: "d3d11va".into(),
            label: "D3D11VA".into(),
            available: false,
            reason: Some("本检查不启用硬解".into()),
        }],
        software_h264_available: true,
        notes: vec!["flavor-check 手工能力表：只含 GPL 编码器，用于验证变体放行/拒绝".into()],
    }
}

fn video_params(encoder: &str) -> Value {
    json!({
        "codec": "h264",
        "encoder": encoder,
        "rateMode": "quality",
        "quality": 26,
        "bitrateKbps": 3000,
        "resolution": "720",
        "fps": "source",
        "audioMode": "none",
        "audioBitrateKbps": 128,
        "hwDecode": false,
        "faststart": true
    })
}

async fn wait_job(ctx: &Ctx, id: &str, timeout: Duration) -> Result<Job, String> {
    let deadline = Instant::now() + timeout;
    loop {
        if let Some(job) = jobs::list(ctx).await.into_iter().find(|j| j.id == id) {
            if job.status == "done" || job.status == "failed" || job.status == "cancelled" {
                return Ok(job);
            }
        }
        if Instant::now() > deadline {
            return Err(format!("等待任务 {id} 超时"));
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

fn find_build(gpl: bool) -> Option<PathBuf> {
    // 先看 samples/_dl 中准备的构建，再回落到 PATH。
    let candidates = [
        "samples/_dl/ffmpeg-lgpl/ffmpeg-master-latest-win64-lgpl/bin/ffmpeg.exe",
        "samples/_dl/ffmpeg-lgpl/ffmpeg-master-latest-win64-lgpl/bin/ffmpeg",
    ];
    if !gpl {
        for c in candidates {
            let p = PathBuf::from(c);
            if p.exists() {
                return Some(p);
            }
        }
    }
    let found = tools::find_tool("ffmpeg")?;
    let info = setup::inspect_file(&found.to_string_lossy());
    if info.gpl == gpl && !info.nonfree {
        Some(found)
    } else {
        None
    }
}

pub async fn run_checks(report_path: &str) -> i32 {
    let mut rec = Recorder::new();
    let flavor = Flavor::current();
    let is_gpl = flavor.id() == "gpl";

    // ---------- 1. 编译期开关 ----------
    let compiled_ok = flavor::BUILD_FLAVOR == flavor.id()
        && flavor.require_gpl_build() == is_gpl
        && flavor.allow_gpl(false) == is_gpl;
    rec.judge(
        "编译期变体与开关固定",
        LOGIC,
        compiled_ok,
        format!(
            "flavor={} buildFlavor={} allowGpl={} requireGplBuild={} debug={}",
            flavor.id(),
            flavor::BUILD_FLAVOR,
            flavor.allow_gpl(false),
            flavor.require_gpl_build(),
            cfg!(debug_assertions)
        ),
        json!(flavor.info()),
    );

    // ---------- 2. 环境变量/设置项不能翻转 ----------
    std::env::set_var("FORMATFORGE_ALLOW_GPL", if is_gpl { "0" } else { "1" });
    std::env::set_var("FORMATFORGE_DISABLE_GPL", if is_gpl { "1" } else { "0" });
    let flip_ok = if is_gpl {
        flavor::allow_gpl_effective(false) && flavor.allow_gpl(false)
    } else {
        flavor::allow_gpl_effective(false) == false
            && flavor.accepts_build(true, false, false).is_err()
    };
    let flip_evidence = json!({
        "env": { "FORMATFORGE_ALLOW_GPL": std::env::var("FORMATFORGE_ALLOW_GPL").ok(), "FORMATFORGE_DISABLE_GPL": std::env::var("FORMATFORGE_DISABLE_GPL").ok() },
        "allowGplEffective(false)": flavor::allow_gpl_effective(false),
        "allowGplEffective(true)": flavor::allow_gpl_effective(true),
        "debug": cfg!(debug_assertions),
    });
    rec.judge(
        "运行时设置/环境变量不能翻转 allowGpl",
        LOGIC,
        flip_ok,
        if is_gpl {
            "GPL 版：设置项传 false、环境变量置 0/1，allowGpl 仍为 true".into()
        } else {
            "LGPL 版：设置项关闭 + 环境变量设 1，策略仍拒绝 GPL（debug 开发开关单独记录）".into()
        },
        flip_evidence,
    );
    std::env::remove_var("FORMATFORGE_ALLOW_GPL");
    std::env::remove_var("FORMATFORGE_DISABLE_GPL");

    // ---------- 3. 策略矩阵 ----------
    let lgpl_build = find_build(false);
    let gpl_build = find_build(true);
    let matrix_ok = if is_gpl {
        flavor.accepts_build(true, false, false).is_ok()
            && flavor.accepts_build(false, false, false).is_err()
            && flavor.accepts_build(true, true, false).is_err()
    } else {
        flavor.accepts_build(true, false, false).is_err()
            && flavor
                .accepts_build(true, false, false)
                .err()
                .map(|e| e.contains("GPL 版"))
                .unwrap_or(false)
            && flavor.accepts_build(false, false, false).is_ok()
            && flavor.accepts_build(true, true, true).is_err()
    };
    rec.judge(
        "策略矩阵：接受的构建类型",
        LOGIC,
        matrix_ok,
        if is_gpl {
            "GPL 版接受 GPL 构建、拒绝 LGPL 构建与 nonfree".into()
        } else {
            "LGPL 版拒绝 GPL 构建（文案指向 GPL 版）、接受 LGPL、拒绝 nonfree".into()
        },
        json!({
            "gplBuild": flavor.accepts_build(true, false, false).err(),
            "lgplBuild": flavor.accepts_build(false, false, false).err(),
            "nonfree": flavor.accepts_build(true, true, true).err(),
        }),
    );

    // ---------- 4. 数据目录隔离 ----------
    let work = PathBuf::from("samples").join(format!("flavor-check-{}", flavor.id()));
    let _ = std::fs::remove_dir_all(&work);
    let _ = std::fs::create_dir_all(&work);
    let lgpl_appdata = work.join(Flavor::Lgpl.app_identifier());
    let gpl_appdata = work.join(Flavor::Gpl.app_identifier());
    let ids_distinct = lgpl_appdata != gpl_appdata
        && Flavor::Lgpl.app_identifier() != Flavor::Gpl.app_identifier();
    match (&lgpl_build, &gpl_build) {
        (Some(lg), Some(gp)) => {
            let installed_lgpl = setup::install_blocking(
                &lgpl_appdata,
                Flavor::Lgpl,
                &lg.to_string_lossy(),
                None,
                None,
                false,
            );
            let installed_gpl = setup::install_blocking(
                &gpl_appdata,
                Flavor::Gpl,
                &gp.to_string_lossy(),
                None,
                None,
                false,
            );
            let lgpl_path = installed_lgpl
                .as_ref()
                .ok()
                .and_then(|i| i.ffmpeg.as_ref().map(|t| t.path.clone()));
            let gpl_path = installed_gpl
                .as_ref()
                .ok()
                .and_then(|i| i.ffmpeg.as_ref().map(|t| t.path.clone()));
            let isolated = ids_distinct
                && lgpl_path
                    .as_deref()
                    .map(|p| Path::new(p).starts_with(&lgpl_appdata))
                    .unwrap_or(false)
                && gpl_path
                    .as_deref()
                    .map(|p| Path::new(p).starts_with(&gpl_appdata))
                    .unwrap_or(false)
                && lgpl_path != gpl_path;
            rec.judge(
                "数据目录隔离：两变体 tools 路径不同",
                LOGIC,
                isolated,
                format!(
                    "lgplAppData={} · gplAppData={} · lgplDest={:?} · gplDest={:?}",
                    lgpl_appdata.display(),
                    gpl_appdata.display(),
                    lgpl_path,
                    gpl_path
                ),
                json!({
                    "lgplAppData": lgpl_appdata.to_string_lossy(),
                    "gplAppData": gpl_appdata.to_string_lossy(),
                    "lgplInstalled": installed_lgpl.as_ref().map(|i| i.ffmpeg.as_ref().map(|t| t.path.clone())),
                    "gplInstalled": installed_gpl.as_ref().map(|i| i.ffmpeg.as_ref().map(|t| t.path.clone())),
                }),
            );
            // 删除 GPL 版目录，LGPL 版仍完整
            let _ = std::fs::remove_dir_all(&gpl_appdata);
            let lgpl_survives = lgpl_path
                .as_deref()
                .map(|p| Path::new(p).exists())
                .unwrap_or(false);
            rec.judge(
                "数据目录隔离：删除一端不影响另一端",
                LOGIC,
                lgpl_survives && !gpl_appdata.exists(),
                format!(
                    "删除 {} 后，LGPL 落位文件仍存在={lgpl_survives}",
                    gpl_appdata.display()
                ),
                json!({ "lgplStillExists": lgpl_survives, "gplRemoved": !gpl_appdata.exists() }),
            );
        }
        _ => {
            rec.skip(
                "数据目录隔离：两变体 tools 路径不同",
                LOGIC,
                "缺少真实构建（LGPL 或 GPL）".into(),
            );
            rec.skip("数据目录隔离：删除一端不影响另一端", LOGIC, "同上".into());
        }
    }

    // ---------- 5. 真实转码：GPL 版用 libx264 ----------
    if is_gpl {
        match (&gpl_build, &lgpl_build) {
            (Some(gp), _) => {
                let ffmpeg = gp.to_string_lossy().to_string();
                let ffprobe = ffmpeg
                    .replace("ffmpeg.exe", "ffprobe.exe")
                    .replace("/ffmpeg", "/ffprobe");
                let ffprobe = Path::new(&ffprobe).exists().then(|| ffprobe.clone());
                let src = work.join("src-3s.mp4");
                let gen = run_ffmpeg(
                    &ffmpeg,
                    &[
                        "-hide_banner",
                        "-y",
                        "-f",
                        "lavfi",
                        "-i",
                        "testsrc2=size=640x360:rate=30",
                        "-t",
                        "3",
                        "-c:v",
                        "mpeg4",
                        "-q:v",
                        "5",
                        &src.to_string_lossy(),
                    ],
                );
                match gen {
                    Err(e) => {
                        rec.fail("真实转码：生成素材", REAL, e, json!({}));
                    }
                    Ok(_) => {
                        rec.pass(
                            "真实转码：生成素材（3s/640x360）",
                            REAL,
                            src.display().to_string(),
                            json!({ "input": src.to_string_lossy(), "ffmpeg": ffmpeg }),
                        );
                        let out_dir = work.join("gpl-out");
                        let _ = std::fs::create_dir_all(&out_dir);
                        let info = tools::describe_from_paths(
                            gp,
                            ffprobe.as_deref().map(Path::new),
                            "custom",
                        );
                        let state = Arc::new(AppState::default());
                        *state.tools.lock().await = Some(info.clone());
                        {
                            let mut inner = state.inner.lock().await;
                            inner.caps = Some(caps_with_gpl_encoders(
                                info.ffmpeg_version.clone().unwrap_or_default().as_str(),
                            ));
                            // 故意传 false：证明 GPL 版的 allowGpl 来自编译期而非运行时设置。
                            inner.allow_gpl = false;
                            inner.concurrency = 1;
                        }
                        let ctx = Ctx::silent(state.clone());
                        tokio::spawn(jobs::start_scheduler(ctx.clone()));
                        let spec = JobSpec {
                            kind: "video-mp4".into(),
                            input: src.to_string_lossy().to_string(),
                            output_dir: out_dir.to_string_lossy().to_string(),
                            inputs: Vec::new(),
                            name_template: None,
                            conflict: None,
                            output_stem: Some("gpl-x264".into()),
                            params: video_params("software"),
                        };
                        match jobs::enqueue(&ctx, vec![spec]).await {
                            Err(e) => rec.fail("真实转码：入队 x264 任务", REAL, e, json!({})),
                            Ok(created) => {
                                let job = created[0].clone();
                                match wait_job(&ctx, &job.id, Duration::from_secs(90)).await {
                                    Err(e) => rec.fail("真实转码：任务完成", REAL, e, json!({})),
                                    Ok(done) => {
                                        let exists = Path::new(&done.output)
                                            .metadata()
                                            .map(|m| m.len() > 1000)
                                            .unwrap_or(false);
                                        let temps = temp_files(&out_dir);
                                        let codec = probe_codec(
                                            ffprobe.as_deref(),
                                            Path::new(&done.output),
                                        );
                                        let ok = done.status == "done"
                                            && done.encoder_used.as_deref() == Some("libx264")
                                            && exists
                                            && temps.is_empty()
                                            && codec
                                                .as_deref()
                                                .map(|c| c.contains("h264"))
                                                .unwrap_or(false);
                                        rec.judge(
                                            "真实转码：GPL 版用 libx264 转出 MP4",
                                            REAL,
                                            ok,
                                            format!(
                                                "status={} encoder={:?} size={:?} temp={:?} ffprobe={:?}",
                                                done.status,
                                                done.encoder_used,
                                                Path::new(&done.output).metadata().map(|m| m.len()).ok(),
                                                temps,
                                                codec
                                            ),
                                            json!({
                                                "ffmpeg": ffmpeg,
                                                "output": done.output,
                                                "encoderUsed": done.encoder_used,
                                                "message": done.message,
                                                "errorKind": done.error_kind,
                                                "tempFiles": temps,
                                                "ffprobeVideo": codec,
                                            }),
                                        );
                                    }
                                }
                            }
                        }
                    }
                }
            }
            _ => rec.skip(
                "真实转码：GPL 版用 libx264 转出 MP4",
                REAL,
                "未找到 GPL 构建".into(),
            ),
        }
    }

    // ---------- 6. LGPL 版拒绝 GPL 编码器（任务层） ----------
    if !is_gpl {
        let input = lgpl_build.clone().or_else(|| gpl_build.clone());
        match input {
            Some(bin) => {
                let ffmpeg = bin.to_string_lossy().to_string();
                let src = work.join("src-3s.mp4");
                let gen = run_ffmpeg(
                    &ffmpeg,
                    &[
                        "-hide_banner",
                        "-y",
                        "-f",
                        "lavfi",
                        "-i",
                        "testsrc2=size=640x360:rate=30",
                        "-t",
                        "3",
                        "-c:v",
                        "mpeg4",
                        "-q:v",
                        "5",
                        &src.to_string_lossy(),
                    ],
                );
                if let Err(e) = gen {
                    rec.fail("真实转码：生成素材", REAL, e, json!({}));
                } else {
                    let out_dir = work.join("lgpl-out");
                    let _ = std::fs::create_dir_all(&out_dir);
                    let info = tools::describe_from_paths(&bin, None, "custom");
                    let state = Arc::new(AppState::default());
                    *state.tools.lock().await = Some(info.clone());
                    {
                        let mut inner = state.inner.lock().await;
                        inner.caps = Some(caps_with_gpl_encoders(
                            info.ffmpeg_version.clone().unwrap_or_default().as_str(),
                        ));
                        inner.allow_gpl = false;
                        inner.concurrency = 1;
                    }
                    let ctx = Ctx::silent(state.clone());
                    tokio::spawn(jobs::start_scheduler(ctx.clone()));
                    for (label, encoder) in [("software", "software"), ("显式 libx264", "libx264")]
                    {
                        let stem = format!("lgpl-reject-{encoder}");
                        let spec = JobSpec {
                            kind: "video-mp4".into(),
                            input: src.to_string_lossy().to_string(),
                            output_dir: out_dir.to_string_lossy().to_string(),
                            inputs: Vec::new(),
                            name_template: None,
                            conflict: None,
                            output_stem: Some(stem),
                            params: video_params(encoder),
                        };
                        match jobs::enqueue(&ctx, vec![spec]).await {
                            Err(e) => rec.fail("LGPL 版：拒绝 GPL 编码器", REAL, e, json!({})),
                            Ok(created) => {
                                let job = created[0].clone();
                                match wait_job(&ctx, &job.id, Duration::from_secs(30)).await {
                                    Err(e) => {
                                        rec.fail("LGPL 版：拒绝 GPL 编码器", REAL, e, json!({}))
                                    }
                                    Ok(done) => {
                                        let no_output = !Path::new(&done.output).exists();
                                        let temps = temp_files(&out_dir);
                                        rec.judge(
                                            &format!("LGPL 版：拒绝 GPL 编码器（{label}）"),
                                            REAL,
                                            done.status == "failed"
                                                && done.encoder_used.is_none()
                                                && no_output
                                                && temps.is_empty(),
                                            format!(
                                                "status={} errorKind={:?} message={:?} outputExists={} temp={:?}",
                                                done.status,
                                                done.error_kind,
                                                done.message,
                                                !no_output,
                                                temps
                                            ),
                                            json!({
                                                "encoderRequested": encoder,
                                                "status": done.status,
                                                "errorKind": done.error_kind,
                                                "message": done.message,
                                                "output": done.output,
                                                "tempFiles": temps,
                                            }),
                                        );
                                    }
                                }
                            }
                        }
                    }
                }
            }
            None => rec.skip("LGPL 版：拒绝 GPL 编码器", REAL, "未找到真实 ffmpeg".into()),
        }
    }

    let report = Report {
        generated_at: now_ms(),
        flavor: flavor.id().into(),
        variant: flavor.info(),
        summary: Summary {
            passed: rec.passed,
            failed: rec.failed,
            skipped: rec.skipped,
            total: rec.checks.len(),
        },
        results: rec.checks,
        unverified: vec![
            "代码签名与 macOS 公证：未验证（构建产物未签名）".into(),
            "真实用户机器上的变体切换/升级路径：未验证（需要真实安装环境）".into(),
            "GPL 源码要约与「不随包分发」措辞的法律审查：未验证，需要法律确认".into(),
            "Linux/macOS 变体构建与安装包合规物：未验证（仅 Windows 完成）".into(),
        ],
    };

    if let Some(parent) = Path::new(report_path).parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let text = serde_json::to_string_pretty(&report).unwrap_or_else(|_| "{}".into());
    if let Err(e) = std::fs::write(report_path, text) {
        eprintln!("无法写入报告 {report_path}：{e}");
    }
    println!(
        "flavor-check[{}]：{} passed / {} failed / {} skipped (total {}) → {}",
        flavor.id(),
        report.summary.passed,
        report.summary.failed,
        report.summary.skipped,
        report.summary.total,
        report_path
    );
    if report.summary.failed == 0 {
        0
    } else {
        1
    }
}
