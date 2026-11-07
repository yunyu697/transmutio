//! 无窗口的真实链路检查：直接驱动 `jobs` 模块（与应用命令同一份代码），
//! 使用真实 ffmpeg 二进制与真实文件系统，覆盖同名冲突、并发、取消、失败重试。
//!
//! 这里没有 UI、没有 Tauri 窗口、没有 mock 转码。每一项结果都写着证据层级。

use crate::jobs::{self, AppState, Ctx, Job, JobSpec};
use crate::probe::{Capabilities, DecodeCap, EncoderCap};
use crate::tools;
use serde::Serialize;
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

const LEVEL: &str = "真实任务核心 + 真实 ffmpeg + 真实文件系统（无窗口）";

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct Check {
    name: String,
    status: String,
    detail: String,
    level: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct Summary {
    passed: usize,
    failed: usize,
    total: usize,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct Report {
    generated_at: i64,
    level: String,
    environment: Value,
    summary: Summary,
    results: Vec<Check>,
}

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

fn pass(results: &mut Vec<Check>, name: &str, detail: String) {
    results.push(Check {
        name: name.into(),
        status: "pass".into(),
        detail,
        level: LEVEL.into(),
    });
}

fn fail(results: &mut Vec<Check>, name: &str, detail: String) {
    results.push(Check {
        name: name.into(),
        status: "fail".into(),
        detail,
        level: LEVEL.into(),
    });
}

/// 读取音频的最大音量（dBFS），用于验证混音后既没静音也没削波。
fn max_volume_db(ffmpeg: &str, input: &Path) -> Result<f64, String> {
    let out = std::process::Command::new(ffmpeg)
        .args([
            "-hide_banner",
            "-nostdin",
            "-i",
            &input.to_string_lossy(),
            "-af",
            "volumedetect",
            "-f",
            "null",
            "-",
        ])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::piped())
        .output()
        .map_err(|e| format!("启动 ffmpeg 失败：{e}"))?;
    let text = String::from_utf8_lossy(&out.stderr);
    let marker = text
        .split("max_volume:")
        .nth(1)
        .ok_or_else(|| "volumedetect 未输出 max_volume".to_string())?;
    marker
        .split_whitespace()
        .next()
        .and_then(|v| v.parse::<f64>().ok())
        .ok_or_else(|| format!("无法解析 max_volume：{}", marker.trim()))
}

fn video_params(hw_decode: bool) -> Value {
    json!({
        "codec": "h264",
        "encoder": "auto",
        "rateMode": "quality",
        "quality": 26,
        "bitrateKbps": 6000,
        "resolution": "720",
        "fps": "source",
        "audioMode": "none",
        "audioBitrateKbps": 128,
        "hwDecode": hw_decode,
        "faststart": true
    })
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

/// 用 ffmpeg 的 volumedetect 读输出音量（dB），用于验证响度归一化确实生效。
fn mean_volume_db(ffmpeg: &str, path: &Path) -> Result<f64, String> {
    let out = std::process::Command::new(ffmpeg)
        .args([
            "-hide_banner",
            "-i",
            &path.to_string_lossy(),
            "-vn",
            "-af",
            "volumedetect",
            "-f",
            "null",
            "-",
        ])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::piped())
        .output()
        .map_err(|e| format!("启动 ffmpeg 失败：{e}"))?;
    let text = String::from_utf8_lossy(&out.stderr);
    let marker = text
        .split("mean_volume:")
        .nth(1)
        .ok_or_else(|| "volumedetect 未输出 mean_volume".to_string())?;
    marker
        .split("dB")
        .next()
        .and_then(|v| v.trim().parse::<f64>().ok())
        .ok_or_else(|| format!("无法解析 mean_volume：{}", marker.trim()))
}

/// 解码首帧并返回 framemd5 哈希，用于证明两个输出的画面确实不同（如水印真的烧进去了）。
fn first_frame_hash(ffmpeg: &str, path: &Path) -> Result<String, String> {
    let out = std::process::Command::new(ffmpeg)
        .args([
            "-hide_banner",
            "-v",
            "error",
            "-i",
            &path.to_string_lossy(),
            "-frames:v",
            "1",
            "-f",
            "framemd5",
            "-",
        ])
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .output()
        .map_err(|e| format!("启动 ffmpeg 失败：{e}"))?;
    let text = String::from_utf8_lossy(&out.stdout);
    let line = text
        .lines()
        .filter(|l| !l.trim_start().starts_with('#'))
        .next_back()
        .ok_or_else(|| "framemd5 未输出帧哈希".to_string())?;
    line.split(',')
        .next_back()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .ok_or_else(|| "framemd5 输出中没有哈希字段".to_string())
}

/// 首帧 PSNR（dB）：两路视频的时基不同，逐帧比对会随时间漂移，
/// 因此只取第 1 帧做像素级比对（裁剪区域是否一致）。
fn average_psnr(ffmpeg: &str, left: &Path, right: &Path) -> Result<f64, String> {
    let out = std::process::Command::new(ffmpeg)
        .args([
            "-hide_banner",
            "-i",
            &left.to_string_lossy(),
            "-i",
            &right.to_string_lossy(),
            "-lavfi",
            "[0:v][1:v]psnr",
            "-frames:v",
            "1",
            "-f",
            "null",
            "-",
        ])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::piped())
        .output()
        .map_err(|e| format!("启动 ffmpeg 失败：{e}"))?;
    let text = String::from_utf8_lossy(&out.stderr);
    let marker = text
        .split("average:")
        .nth(1)
        .ok_or_else(|| format!("PSNR 未输出 average：{}", text.lines().last().unwrap_or("")))?;
    marker
        .split_whitespace()
        .next()
        .and_then(|v| v.parse::<f64>().ok())
        .ok_or_else(|| format!("无法解析 PSNR：{}", marker.trim()))
}

fn run_ffmpeg(ffmpeg: &str, args: &[String]) -> Result<(), String> {
    let out = std::process::Command::new(ffmpeg)
        .args(args)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::piped())
        .output()
        .map_err(|e| format!("启动 ffmpeg 失败：{e}"))?;
    if out.status.success() {
        Ok(())
    } else {
        let tail: String = String::from_utf8_lossy(&out.stderr)
            .lines()
            .rev()
            .take(3)
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect::<Vec<_>>()
            .join(" | ");
        Err(format!("ffmpeg 失败：{tail}"))
    }
}

fn video_codec_args(ffmpeg: &str) -> Vec<String> {
    if tools::version_of(ffmpeg)
        .map(|(_, c)| c.contains("--enable-libx264"))
        .unwrap_or(false)
    {
        vec![
            "-c:v".to_string(),
            "libx264".into(),
            "-preset".into(),
            "ultrafast".into(),
        ]
    } else {
        vec![
            "-c:v".to_string(),
            "mpeg4".into(),
            "-q:v".into(),
            "4".into(),
        ]
    }
}

fn make_source(ffmpeg: &str, path: &Path, seconds: u32) -> Result<(), String> {
    make_source_sized(ffmpeg, path, seconds, 1280, 720)
}

fn make_source_sized(
    ffmpeg: &str,
    path: &Path,
    seconds: u32,
    width: u32,
    height: u32,
) -> Result<(), String> {
    let mut args = vec![
        "-hide_banner".into(),
        "-loglevel".into(),
        "error".into(),
        "-y".into(),
        "-f".into(),
        "lavfi".into(),
        "-i".into(),
        format!("testsrc2=size={width}x{height}:rate=30:duration={seconds}"),
    ];
    args.extend(video_codec_args(ffmpeg));
    args.extend([
        "-pix_fmt".into(),
        "yuv420p".into(),
        path.to_string_lossy().to_string(),
    ]);
    run_ffmpeg(ffmpeg, &args)
}

fn make_source_bitrate(ffmpeg: &str, path: &Path, seconds: u32, kbps: u32) -> Result<(), String> {
    let mut args = vec![
        "-hide_banner".into(),
        "-loglevel".into(),
        "error".into(),
        "-y".into(),
        "-f".into(),
        "lavfi".into(),
        "-i".into(),
        format!("testsrc2=size=1280x720:rate=30:duration={seconds}"),
    ];
    args.extend(video_codec_args(ffmpeg));
    args.extend([
        "-b:v".into(),
        format!("{kbps}k"),
        "-maxrate".into(),
        format!("{kbps}k"),
        "-bufsize".into(),
        format!("{}k", kbps * 2),
        "-pix_fmt".into(),
        "yuv420p".into(),
        path.to_string_lossy().to_string(),
    ]);
    run_ffmpeg(ffmpeg, &args)
}

fn make_audio_wav(ffmpeg: &str, path: &Path, seconds: u32) -> Result<(), String> {
    let args = vec![
        "-hide_banner".into(),
        "-loglevel".into(),
        "error".into(),
        "-y".into(),
        "-f".into(),
        "lavfi".into(),
        "-i".into(),
        format!("sine=frequency=440:sample_rate=48000:duration={seconds}"),
        "-ac".into(),
        "2".into(),
        "-c:a".into(),
        "pcm_s16le".into(),
        path.to_string_lossy().to_string(),
    ];
    run_ffmpeg(ffmpeg, &args)
}

/// 统计输出里的视频/音频流数量（用于混流断言）。
fn stream_counts(ffprobe: &str, path: &Path) -> Result<(u32, u32), String> {
    let out = std::process::Command::new(ffprobe)
        .args([
            "-v",
            "error",
            "-show_entries",
            "stream=codec_type",
            "-of",
            "csv=p=0",
            &path.to_string_lossy(),
        ])
        .output()
        .map_err(|e| format!("启动 ffprobe 失败：{e}"))?;
    if !out.status.success() {
        return Err("ffprobe 读取流信息失败".into());
    }
    let text = String::from_utf8_lossy(&out.stdout);
    let video = text.lines().filter(|l| l.trim() == "video").count() as u32;
    let audio = text.lines().filter(|l| l.trim() == "audio").count() as u32;
    Ok((video, audio))
}

/// GIF 帧数（解码计数，不是元数据猜测）。
fn frame_count(ffprobe: &str, path: &Path) -> Result<u32, String> {
    let out = std::process::Command::new(ffprobe)
        .args([
            "-v",
            "error",
            "-select_streams",
            "v:0",
            "-count_frames",
            "-show_entries",
            "stream=nb_read_frames",
            "-of",
            "default=nokey=1:noprint_wrappers=1",
            &path.to_string_lossy(),
        ])
        .output()
        .map_err(|e| format!("启动 ffprobe 失败：{e}"))?;
    String::from_utf8_lossy(&out.stdout)
        .trim()
        .parse::<u32>()
        .map_err(|_| "无法解析帧数".to_string())
}

/// 给一张 JPEG 注入 EXIF Orientation（合成样本，不是相机原图）。
fn inject_exif_orientation(path: &Path, orientation: u16) -> Result<(), String> {
    let bytes = std::fs::read(path).map_err(|e| format!("读取 JPEG 失败：{e}"))?;
    if bytes.len() < 2 || bytes[0] != 0xFF || bytes[1] != 0xD8 {
        return Err("不是 JPEG".into());
    }
    let mut segment: Vec<u8> = vec![0xFF, 0xE1, 0x00, 0x20];
    segment.extend_from_slice(b"Exif\0\0");
    segment.extend_from_slice(&[0x49, 0x49, 0x2A, 0x00, 0x08, 0x00, 0x00, 0x00]);
    segment.extend_from_slice(&[0x01, 0x00]);
    segment.extend_from_slice(&[0x12, 0x01, 0x03, 0x00]);
    segment.extend_from_slice(&[0x01, 0x00, 0x00, 0x00]);
    segment.extend_from_slice(&orientation.to_le_bytes());
    segment.extend_from_slice(&[0x00, 0x00]);
    segment.extend_from_slice(&[0x00, 0x00, 0x00, 0x00]);
    let mut out = bytes[0..2].to_vec();
    out.extend_from_slice(&segment);
    out.extend_from_slice(&bytes[2..]);
    std::fs::write(path, out).map_err(|e| format!("写入 JPEG 失败：{e}"))
}

fn test_caps(version: &str) -> Capabilities {
    let amf = EncoderCap {
        id: "h264_amf".into(),
        codec: "h264".into(),
        label: "AMD AMF".into(),
        vendor: "amd".into(),
        hardware: true,
        license: "lgpl".into(),
        allowed: true,
        compiled: true,
        available: true,
        reason: None,
    };
    let x264 = EncoderCap {
        id: "libx264".into(),
        codec: "h264".into(),
        label: "libx264".into(),
        vendor: "software".into(),
        hardware: false,
        license: "gpl".into(),
        allowed: true,
        compiled: true,
        available: true,
        reason: None,
    };
    let nvenc = EncoderCap {
        id: "h264_nvenc".into(),
        codec: "h264".into(),
        label: "NVIDIA NVENC".into(),
        vendor: "nvidia".into(),
        hardware: true,
        license: "lgpl".into(),
        allowed: true,
        compiled: true,
        available: false,
        reason: Some("试编失败（无设备）".into()),
    };
    Capabilities {
        probed_at: now_ms(),
        cache_hit: false,
        fingerprint: "headless-test-caps".into(),
        platform: std::env::consts::OS.into(),
        arch: std::env::consts::ARCH.into(),
        ffmpeg_version: version.into(),
        build_gpl: true,
        build_nonfree: false,
        video_encoders: vec![amf, nvenc, x264],
        audio_encoders: vec![],
        decode_accels: vec![DecodeCap {
            id: "d3d11va".into(),
            label: "D3D11VA".into(),
            available: false,
            reason: Some("本检查未启用硬解，避免依赖 GPU".into()),
        }],
        software_h264_available: true,
        subtitle_burn: false,
        notes: vec!["headless 检查用手工能力表，不替代真实探测；真实探测由 verify-mvp 覆盖".into()],
    }
}

async fn wait_until<F>(
    ctx: &Ctx,
    timeout: Duration,
    mut predicate: F,
    label: &str,
) -> Result<Vec<Job>, String>
where
    F: FnMut(&[Job]) -> bool,
{
    let deadline = Instant::now() + timeout;
    loop {
        let all = jobs::list(ctx).await;
        if predicate(&all) {
            return Ok(all);
        }
        if Instant::now() > deadline {
            let snapshot: Vec<String> = all
                .iter()
                .map(|j| format!("{}={}", j.output, j.status))
                .collect();
            return Err(format!("等待超时：{label}；当前 {}", snapshot.join(", ")));
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

async fn run_one(ctx: &Ctx, spec: JobSpec, timeout_secs: u64) -> Result<Job, String> {
    let created = jobs::enqueue(ctx, vec![spec]).await?;
    let id = created[0].id.clone();
    let snapshot = wait_until(
        ctx,
        Duration::from_secs(timeout_secs),
        |all| {
            all.iter()
                .any(|j| j.id == id && (j.status == "done" || j.status == "failed"))
        },
        "单任务结束",
    )
    .await?;
    snapshot
        .into_iter()
        .find(|j| j.id == id)
        .ok_or_else(|| "任务消失".to_string())
}

pub async fn run_checks(report_path: &str) -> i32 {
    let mut results: Vec<Check> = Vec::new();
    let workspace = PathBuf::from("samples/headless");
    let _ = std::fs::remove_dir_all(&workspace);
    if let Err(e) = std::fs::create_dir_all(&workspace) {
        eprintln!("无法创建 {}：{e}", workspace.display());
        return 1;
    }

    let ffmpeg_path = match tools::find_tool("ffmpeg") {
        Some(p) => p.to_string_lossy().to_string(),
        None => {
            let report = Report {
                generated_at: now_ms(),
                level: LEVEL.into(),
                environment: json!({ "error": "PATH 中没有 ffmpeg" }),
                summary: Summary {
                    passed: 0,
                    failed: 1,
                    total: 1,
                },
                results: vec![Check {
                    name: "定位 ffmpeg".into(),
                    status: "fail".into(),
                    detail: "PATH 中没有 ffmpeg，无法执行任何真实转换".into(),
                    level: LEVEL.into(),
                }],
            };
            let _ = write_report(report_path, &report);
            return 1;
        }
    };
    let ffprobe = tools::find_tool("ffprobe").map(|p| p.to_string_lossy().to_string());
    let info = tools::describe_from_paths(
        Path::new(&ffmpeg_path),
        ffprobe.as_deref().map(Path::new),
        "path",
    );

    let state = Arc::new(AppState::default());
    *state.tools.lock().await = Some(info.clone());
    *state.log_dir.lock().await = Some(workspace.join("logs"));
    {
        let mut inner = state.inner.lock().await;
        let version = info.ffmpeg_version.clone().unwrap_or_default();
        inner.caps = Some(test_caps(&version));
        inner.allow_gpl = true;
        inner.concurrency = 4;
    }
    // 完成后动作的回调注入：真实检查只记录调用，绝不打开窗口或关机。
    let after_calls: Arc<std::sync::Mutex<Vec<(String, String)>>> =
        Arc::new(std::sync::Mutex::new(Vec::new()));
    let recorder = after_calls.clone();
    let ctx = Ctx::silent(state.clone()).with_after_action(move |action, dir| {
        if let Ok(mut guard) = recorder.lock() {
            guard.push((action.to_string(), dir.to_string()));
        }
    });
    tokio::spawn(jobs::start_scheduler(ctx.clone()));

    // 真实素材：一条 60s 720p 视频 + 一份损坏文件
    let src = workspace.join("src-60s.mp4");
    let src_short = workspace.join("src-8s.mp4");
    match make_source(&ffmpeg_path, &src, 60) {
        Ok(()) => pass(
            &mut results,
            "生成真实素材（60s/720p）",
            src.display().to_string(),
        ),
        Err(e) => {
            fail(&mut results, "生成真实素材（60s/720p）", e);
            return finish(report_path, info, results).await;
        }
    }
    if let Err(e) = make_source(&ffmpeg_path, &src_short, 8) {
        fail(&mut results, "生成真实素材（8s）", e);
    } else {
        pass(
            &mut results,
            "生成真实素材（8s）",
            "用于失败/重试用例".into(),
        );
    }
    let corrupt = workspace.join("corrupt.mp4");
    let _ = std::fs::write(&corrupt, b"NOT A VIDEO \x00\x01\x02");

    let out_a = workspace.join("conflict");
    let _ = std::fs::create_dir_all(&out_a);
    let out_b = workspace.join("concurrent");
    let _ = std::fs::create_dir_all(&out_b);
    let out_c = workspace.join("cancel");
    let _ = std::fs::create_dir_all(&out_c);
    let out_d = workspace.join("retry");
    let _ = std::fs::create_dir_all(&out_d);

    // 1) 同名输出冲突：两个任务同名入队，必须得到两个不同文件名，且都真实生成
    let specs: Vec<JobSpec> = (0..2)
        .map(|_| JobSpec {
            kind: "video-mp4".into(),
            input: src_short.to_string_lossy().to_string(),
            output_dir: out_a.to_string_lossy().to_string(),
            inputs: Vec::new(),
            name_template: None,
            conflict: None,
            output_stem: Some("同名输出".into()),
            params: video_params(false),
        })
        .collect();
    match jobs::enqueue(&ctx, specs).await {
        Err(e) => fail(&mut results, "同名冲突入队", e),
        Ok(created) => {
            let distinct = created[0].output != created[1].output;
            let wait = wait_until(
                &ctx,
                Duration::from_secs(90),
                |all| {
                    all.iter()
                        .filter(|j| j.output.ends_with(".mp4"))
                        .all(|j| j.status == "done" || j.status == "failed")
                },
                "同名冲突任务完成",
            )
            .await;
            match wait {
                Err(e) => fail(&mut results, "同名冲突完成", e),
                Ok(all) => {
                    let ours: Vec<&Job> = all
                        .iter()
                        .filter(|j| j.input == src_short.to_string_lossy())
                        .collect();
                    let all_done = ours.iter().all(|j| j.status == "done");
                    let files_ok = ours.iter().all(|j| {
                        Path::new(&j.output)
                            .metadata()
                            .map(|m| m.len() > 1000)
                            .unwrap_or(false)
                    });
                    let temps = temp_files(&out_a);
                    if distinct && all_done && files_ok && temps.is_empty() {
                        pass(
                            &mut results,
                            "同名冲突：输出不覆盖且两任务都成功",
                            format!("输出：{}, {}", ours[0].output, ours[1].output),
                        );
                    } else {
                        fail(
                            &mut results,
                            "同名冲突：输出不覆盖且两任务都成功",
                            format!(
                                "distinct={distinct} done={all_done} files_ok={files_ok} temp={temps:?}"
                            ),
                        );
                    }
                }
            }
        }
    }

    // 2) 4 槽并发：4 条 60s 视频同时入队，观测 running 峰值
    let specs: Vec<JobSpec> = (0..4)
        .map(|i| JobSpec {
            kind: "video-mp4".into(),
            input: src.to_string_lossy().to_string(),
            output_dir: out_b.to_string_lossy().to_string(),
            inputs: Vec::new(),
            name_template: None,
            conflict: None,
            output_stem: Some(format!("并发-{i}")),
            params: video_params(false),
        })
        .collect();
    match jobs::enqueue(&ctx, specs).await {
        Err(e) => fail(&mut results, "4 槽并加入队", e),
        Ok(created) => {
            let ids: Vec<String> = created.iter().map(|j| j.id.clone()).collect();
            let mut max_running = 0usize;
            let deadline = Instant::now() + Duration::from_secs(180);
            let mut finished = false;
            while Instant::now() < deadline {
                let all = jobs::list(&ctx).await;
                let running = all
                    .iter()
                    .filter(|j| ids.contains(&j.id) && j.status == "running")
                    .count();
                max_running = max_running.max(running);
                if all
                    .iter()
                    .filter(|j| ids.contains(&j.id))
                    .all(|j| j.status == "done" || j.status == "failed")
                {
                    finished = true;
                    break;
                }
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
            let all = jobs::list(&ctx).await;
            let ours: Vec<&Job> = all.iter().filter(|j| ids.contains(&j.id)).collect();
            let done = ours.iter().filter(|j| j.status == "done").count();
            let files_ok = ours.iter().all(|j| {
                Path::new(&j.output)
                    .metadata()
                    .map(|m| m.len() > 1000)
                    .unwrap_or(false)
            });
            let temps = temp_files(&out_b);
            if finished && done == 4 && max_running >= 2 && files_ok && temps.is_empty() {
                pass(
                    &mut results,
                    "4 槽并发：同时运行且全部成功",
                    format!(
                        "running 峰值={max_running}，完成={done}/4，临时残留={}",
                        temps.len()
                    ),
                );
            } else {
                fail(
                    &mut results,
                    "4 槽并发：同时运行且全部成功",
                    format!(
                        "finished={finished} done={done}/4 running峰值={max_running} files_ok={files_ok} temp={temps:?}"
                    ),
                );
            }
        }
    }

    // 3) 取消：运行中取消，子进程被杀、输出不产生、临时文件清空
    let spec = JobSpec {
        kind: "video-mp4".into(),
        input: src.to_string_lossy().to_string(),
        output_dir: out_c.to_string_lossy().to_string(),
        inputs: Vec::new(),
        name_template: None,
        conflict: None,
        output_stem: Some("取消测试".into()),
        params: video_params(false),
    };
    match jobs::enqueue(&ctx, vec![spec]).await {
        Err(e) => fail(&mut results, "取消：入队", e),
        Ok(created) => {
            let job = created[0].clone();
            let started = wait_until(
                &ctx,
                Duration::from_secs(20),
                |all| all.iter().any(|j| j.id == job.id && j.status == "running"),
                "任务进入 running",
            )
            .await;
            match started {
                Err(e) => fail(&mut results, "取消：任务未进入运行", e),
                Ok(_) => {
                    if let Err(e) = jobs::cancel(&ctx, &job.id).await {
                        fail(&mut results, "取消：调用失败", e);
                    } else {
                        let waited = wait_until(
                            &ctx,
                            Duration::from_secs(30),
                            |all| {
                                all.iter()
                                    .any(|j| j.id == job.id && j.status == "cancelled")
                            },
                            "任务状态变为 cancelled",
                        )
                        .await;
                        // 延迟兜底清理是 3s，这里再等 4s 观察临时文件
                        tokio::time::sleep(Duration::from_secs(4)).await;
                        let temps = temp_files(&out_c);
                        let output_exists = Path::new(&job.output).exists();
                        match waited {
                            Ok(_) if temps.is_empty() && !output_exists => pass(
                                &mut results,
                                "取消：杀进程 + 无输出 + 无临时文件",
                                format!(
                                    "status=cancelled，temp={}，output存在={output_exists}",
                                    temps.len()
                                ),
                            ),
                            Ok(_) => fail(
                                &mut results,
                                "取消：杀进程 + 无输出 + 无临时文件",
                                format!("temp={temps:?}，output存在={output_exists}"),
                            ),
                            Err(e) => fail(&mut results, "取消：状态未变为 cancelled", e),
                        }
                    }
                }
            }
        }
    }

    // 4) 失败分类 + 重试保留参数
    let params = video_params(false);
    let spec = JobSpec {
        kind: "video-mp4".into(),
        input: corrupt.to_string_lossy().to_string(),
        output_dir: out_d.to_string_lossy().to_string(),
        inputs: Vec::new(),
        name_template: None,
        conflict: None,
        output_stem: Some("损坏输入".into()),
        params: params.clone(),
    };
    match jobs::enqueue(&ctx, vec![spec]).await {
        Err(e) => fail(&mut results, "失败分类：入队", e),
        Ok(created) => {
            let job = created[0].clone();
            let waited = wait_until(
                &ctx,
                Duration::from_secs(30),
                |all| {
                    all.iter()
                        .any(|j| j.id == job.id && (j.status == "failed" || j.status == "done"))
                },
                "损坏输入任务结束",
            )
            .await;
            match waited {
                Err(e) => fail(&mut results, "失败分类", e),
                Ok(all) => {
                    let current = all.iter().find(|j| j.id == job.id).cloned();
                    match current {
                        Some(j)
                            if j.status == "failed" && j.error_kind.as_deref() == Some("input") =>
                        {
                            pass(
                                &mut results,
                                "失败分类：损坏输入 → input",
                                j.message.clone().unwrap_or_default(),
                            )
                        }
                        Some(j) => fail(
                            &mut results,
                            "失败分类：损坏输入 → input",
                            format!(
                                "status={} errorKind={:?} message={:?}",
                                j.status, j.error_kind, j.message
                            ),
                        ),
                        None => fail(&mut results, "失败分类", "任务消失".into()),
                    }
                }
            }

            // 重试：attempts 递增，params 与 output 保持不变
            if let Err(e) = jobs::retry(&ctx, &job.id).await {
                fail(&mut results, "重试：重新入队失败", e);
            } else {
                let waited = wait_until(
                    &ctx,
                    Duration::from_secs(30),
                    |all| all.iter().any(|j| j.id == job.id && !j.queue_like()),
                    "重试任务结束",
                )
                .await;
                let all = jobs::list(&ctx).await;
                if let Some(j) = all.iter().find(|x| x.id == job.id) {
                    let params_same = j.params == params;
                    let output_same = j.output == job.output;
                    if waited.is_ok() && j.attempts >= 1 && params_same && output_same {
                        pass(
                            &mut results,
                            "重试：原参数与输出保留，attempts 递增",
                            format!(
                                "attempts={}，params相同={params_same}，output相同={output_same}",
                                j.attempts
                            ),
                        );
                    } else {
                        fail(
                            &mut results,
                            "重试：原参数与输出保留，attempts 递增",
                            format!(
                                "attempts={} params相同={params_same} output相同={output_same} err={:?}",
                                j.attempts, waited.err()
                            ),
                        );
                    }
                } else {
                    fail(&mut results, "重试", "任务消失".into());
                }
            }
        }
    }

    // 5) 旋转 90°：1280x720 必须变成 720x1280（不是只写元数据）
    let out_e = workspace.join("rotate");
    let _ = std::fs::create_dir_all(&out_e);
    let mut params = video_params(false);
    params["encoder"] = json!("software");
    params["resolution"] = json!("source");
    params["rotation"] = json!("90");
    let spec = JobSpec {
        kind: "video-mp4".into(),
        input: src_short.to_string_lossy().to_string(),
        output_dir: out_e.to_string_lossy().to_string(),
        inputs: Vec::new(),
        name_template: None,
        conflict: None,
        output_stem: Some("旋转90".into()),
        params,
    };
    match run_one(&ctx, spec, 120).await {
        Err(e) => fail(&mut results, "旋转 90°", e),
        Ok(j) if j.status == "done" => match ffprobe.as_deref() {
            Some(fp) => {
                let info = crate::probe::probe_media(fp, &j.output).await;
                match (info.width, info.height) {
                    (Some(720), Some(1280)) => pass(
                        &mut results,
                        "旋转 90°：输出宽高互换（720x1280）",
                        format!(
                            "来源 1280x720 → 输出 {}x{}，encoder={:?}",
                            info.width.unwrap_or(0),
                            info.height.unwrap_or(0),
                            j.encoder_used
                        ),
                    ),
                    (w, h) => fail(
                        &mut results,
                        "旋转 90°：输出宽高互换（720x1280）",
                        format!("实际 {w:?}x{h:?}，encoder={:?}", j.encoder_used),
                    ),
                }
            }
            None => fail(
                &mut results,
                "旋转 90°",
                "缺少 ffprobe，无法做产物断言".into(),
            ),
        },
        Ok(j) => fail(
            &mut results,
            "旋转 90°",
            format!("status={} message={:?}", j.status, j.message),
        ),
    }

    // 6) 居中裁剪 9:16：1280x720 裁成约 404x720（等比，不拉伸）
    let out_f = workspace.join("crop");
    let _ = std::fs::create_dir_all(&out_f);
    let mut params = video_params(false);
    params["encoder"] = json!("software");
    params["resolution"] = json!("source");
    params["crop"] = json!("9:16");
    let spec = JobSpec {
        kind: "video-mp4".into(),
        input: src_short.to_string_lossy().to_string(),
        output_dir: out_f.to_string_lossy().to_string(),
        inputs: Vec::new(),
        name_template: None,
        conflict: None,
        output_stem: Some("裁剪9x16".into()),
        params,
    };
    match run_one(&ctx, spec, 120).await {
        Err(e) => fail(&mut results, "居中裁剪 9:16", e),
        Ok(j) if j.status == "done" => match ffprobe.as_deref() {
            Some(fp) => {
                let info = crate::probe::probe_media(fp, &j.output).await;
                let ratio = match (info.width, info.height) {
                    (Some(w), Some(h)) if h > 0 => w as f64 / h as f64,
                    _ => 0.0,
                };
                let ok = info.height == Some(720) && (ratio - 9.0 / 16.0).abs() < 0.02;
                let detail = format!(
                    "来源 1280x720 → 输出 {}x{}（宽高比 {ratio:.4}，目标 {:.4}）",
                    info.width.unwrap_or(0),
                    info.height.unwrap_or(0),
                    9.0 / 16.0
                );
                if ok {
                    pass(&mut results, "居中裁剪 9:16：等比裁剪不拉伸", detail);
                } else {
                    fail(&mut results, "居中裁剪 9:16：等比裁剪不拉伸", detail);
                }
            }
            None => fail(
                &mut results,
                "居中裁剪 9:16",
                "缺少 ffprobe，无法做产物断言".into(),
            ),
        },
        Ok(j) => fail(
            &mut results,
            "居中裁剪 9:16",
            format!("status={} message={:?}", j.status, j.message),
        ),
    }

    // 6b) 任意矩形裁剪：1280x720 的 x10% y20% 50%x50% → 640x360 @ (128,144)，
    //     与独立 ffmpeg 裁剪参考逐帧比对（PSNR），证明截取位置和尺寸都正确。
    let out_rect = workspace.join("crop-rect");
    let _ = std::fs::create_dir_all(&out_rect);
    let mut params = video_params(false);
    params["encoder"] = json!("software");
    params["resolution"] = json!("source");
    params["quality"] = json!(18);
    params["cropMode"] = json!("rect");
    params["cropXPct"] = json!(10.0);
    params["cropYPct"] = json!(20.0);
    params["cropWPct"] = json!(50.0);
    params["cropHPct"] = json!(50.0);
    let spec = JobSpec {
        kind: "video-mp4".into(),
        input: src_short.to_string_lossy().to_string(),
        output_dir: out_rect.to_string_lossy().to_string(),
        inputs: Vec::new(),
        name_template: None,
        conflict: None,
        output_stem: Some("矩形裁剪".into()),
        params,
    };
    match run_one(&ctx, spec, 120).await {
        Err(e) => fail(&mut results, "任意矩形裁剪", e),
        Ok(j) if j.status == "done" => match ffprobe.as_deref() {
            Some(fp) => {
                let info = crate::probe::probe_media(fp, &j.output).await;
                let dims_ok = info.width == Some(640) && info.height == Some(360);
                let reference = workspace.join("crop-rect-ref.mkv");
                let ref_args: Vec<String> = vec![
                    "-hide_banner".into(),
                    "-loglevel".into(),
                    "error".into(),
                    "-y".into(),
                    "-i".into(),
                    src_short.to_string_lossy().to_string(),
                    "-vf".into(),
                    "crop=640:360:128:144".into(),
                    "-c:v".into(),
                    "ffv1".into(),
                    reference.to_string_lossy().to_string(),
                ];
                let psnr = match run_ffmpeg(&ffmpeg_path, &ref_args) {
                    Err(e) => Err(format!("生成参考裁剪失败：{e}")),
                    Ok(_) => average_psnr(&ffmpeg_path, Path::new(&j.output), reference.as_path()),
                };
                match psnr {
                    Ok(db) => {
                        let ok = dims_ok && db >= 45.0;
                        let detail = format!(
                            "来源 1280x720，矩形 10%/20%/50%/50% → 输出 {}x{}（目标 640x360），与独立 crop=640:360:128:144 参考的首帧 PSNR {db:.2} dB（阈值 45）",
                            info.width.unwrap_or(0),
                            info.height.unwrap_or(0)
                        );
                        if ok {
                            pass(&mut results, "任意矩形裁剪：尺寸与截取位置正确", detail);
                        } else {
                            fail(&mut results, "任意矩形裁剪：尺寸与截取位置正确", detail);
                        }
                    }
                    Err(e) => fail(&mut results, "任意矩形裁剪：PSNR 比对", e),
                }
            }
            None => fail(
                &mut results,
                "任意矩形裁剪",
                "缺少 ffprobe，无法做产物断言".into(),
            ),
        },
        Ok(j) => fail(
            &mut results,
            "任意矩形裁剪",
            format!("status={} message={:?}", j.status, j.message),
        ),
    }

    // 6c) 裁剪预览帧：真实调用后端函数，校验返回 PNG 的 IHDR 尺寸
    match jobs::preview_frame_data_url(&ffmpeg_path, &src_short.to_string_lossy(), 0.5, 480).await {
        Err(e) => fail(&mut results, "裁剪预览帧", e),
        Ok(url) => {
            use base64::Engine;
            let prefix = "data:image/png;base64,";
            let png = url
                .strip_prefix(prefix)
                .ok_or_else(|| "预览帧不是 PNG data URL".to_string())
                .and_then(|body| {
                    base64::engine::general_purpose::STANDARD
                        .decode(body)
                        .map_err(|e| format!("预览帧 base64 解码失败：{e}"))
                });
            match png {
                Ok(bytes)
                    if bytes.len() > 8
                        && bytes[0..8] == [0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a] =>
                {
                    let width = u32::from_be_bytes([bytes[16], bytes[17], bytes[18], bytes[19]]);
                    let height = u32::from_be_bytes([bytes[20], bytes[21], bytes[22], bytes[23]]);
                    let detail = format!(
                        "PNG {}x{}，{} 字节（最长边限制 480）",
                        width,
                        height,
                        bytes.len()
                    );
                    if width == 480 && height == 270 {
                        pass(&mut results, "裁剪预览帧：PNG 尺寸正确", detail);
                    } else {
                        fail(&mut results, "裁剪预览帧：PNG 尺寸正确", detail);
                    }
                }
                Ok(bytes) => fail(
                    &mut results,
                    "裁剪预览帧：PNG 尺寸正确",
                    format!("不是有效 PNG（{} 字节）", bytes.len()),
                ),
                Err(e) => fail(&mut results, "裁剪预览帧：PNG 尺寸正确", e),
            }
        }
    }

    // 6d) 设备预设模板：apple-1080p（MP4/H.264/1080p/30 fps/AAC 128k）真实产物
    let out_preset = workspace.join("preset-apple");
    let _ = std::fs::create_dir_all(&out_preset);
    let preset = crate::device_presets::find("apple-1080p").expect("预设必须存在");
    let mut params = video_params(false);
    params["encoder"] = json!("software");
    if let (Some(obj), Some(over)) = (params.as_object_mut(), preset.params_json().as_object()) {
        for (key, value) in over {
            obj.insert(key.clone(), value.clone());
        }
    }
    let spec = JobSpec {
        kind: "video-mp4".into(),
        input: src_short.to_string_lossy().to_string(),
        output_dir: out_preset.to_string_lossy().to_string(),
        inputs: Vec::new(),
        name_template: None,
        conflict: None,
        output_stem: Some("设备预设".into()),
        params,
    };
    match run_one(&ctx, spec, 120).await {
        Err(e) => fail(&mut results, "设备预设模板：apple-1080p", e),
        Ok(j) if j.status == "done" => match ffprobe.as_deref() {
            Some(fp) => {
                let info = crate::probe::probe_media(fp, &j.output).await;
                let fps = info.fps.unwrap_or(0.0);
                let ok = info.width == Some(1920)
                    && info.height == Some(1080)
                    && (fps - 30.0).abs() < 0.5;
                let detail = format!(
                    "720p 源 → {}x{}（目标 1920x1080），帧率 {fps:.2}（目标 30），编码器 {:?}",
                    info.width.unwrap_or(0),
                    info.height.unwrap_or(0),
                    j.encoder_used
                );
                if ok {
                    pass(&mut results, "设备预设模板：apple-1080p", detail);
                } else {
                    fail(&mut results, "设备预设模板：apple-1080p", detail);
                }
            }
            None => fail(
                &mut results,
                "设备预设模板：apple-1080p",
                "缺少 ffprobe，无法做产物断言".into(),
            ),
        },
        Ok(j) => fail(
            &mut results,
            "设备预设模板：apple-1080p",
            format!("status={} message={:?}", j.status, j.message),
        ),
    }

    // 7) 响度归一化：-32 dB 左右的低音量素材应被拉到接近 -16 LUFS
    let quiet = workspace.join("quiet.wav");
    let quiet_args = vec![
        "-hide_banner".to_string(),
        "-loglevel".into(),
        "error".into(),
        "-y".into(),
        "-f".into(),
        "lavfi".into(),
        "-i".into(),
        "sine=frequency=440:duration=3".into(),
        "-af".into(),
        "volume=0.05".into(),
        "-ar".into(),
        "44100".into(),
        quiet.to_string_lossy().to_string(),
    ];
    if let Err(e) = run_ffmpeg(&ffmpeg_path, &quiet_args) {
        fail(
            &mut results,
            "音量归一化",
            format!("生成低音量素材失败：{e}"),
        );
    } else {
        match mean_volume_db(&ffmpeg_path, &quiet) {
            Err(e) => fail(&mut results, "音量归一化", format!("读取源音量失败：{e}")),
            Ok(before) => {
                let out_g = workspace.join("loudnorm");
                let _ = std::fs::create_dir_all(&out_g);
                let spec = JobSpec {
                    kind: "audio-mp3".into(),
                    input: quiet.to_string_lossy().to_string(),
                    output_dir: out_g.to_string_lossy().to_string(),
                    inputs: Vec::new(),
                    name_template: None,
                    conflict: None,
                    output_stem: Some("归一化".into()),
                    params: json!({
                        "format": "mp3",
                        "bitrateKbps": 192,
                        "sampleRate": "source",
                        "channels": "source",
                        "normalize": true
                    }),
                };
                match run_one(&ctx, spec, 60).await {
                    Err(e) => fail(&mut results, "音量归一化", e),
                    Ok(j) if j.status == "done" => {
                        match mean_volume_db(&ffmpeg_path, Path::new(&j.output)) {
                            Ok(after) if after > before + 5.0 => pass(
                                &mut results,
                                "音量归一化：低音量素材被提升 ≥5 dB",
                                format!("mean_volume {before:.1} dB → {after:.1} dB"),
                            ),
                            Ok(after) => fail(
                                &mut results,
                                "音量归一化：低音量素材被提升 ≥5 dB",
                                format!("提升不足：{before:.1} dB → {after:.1} dB"),
                            ),
                            Err(e) => {
                                fail(&mut results, "音量归一化", format!("读取输出音量失败：{e}"))
                            }
                        }
                    }
                    Ok(j) => fail(
                        &mut results,
                        "音量归一化",
                        format!("status={} message={:?}", j.status, j.message),
                    ),
                }
            }
        }
    }

    // 8) 每任务日志：失败任务必须落盘完整 stderr，供用户导出排障
    let out_h = workspace.join("job-log");
    let _ = std::fs::create_dir_all(&out_h);
    let spec = JobSpec {
        kind: "video-mp4".into(),
        input: corrupt.to_string_lossy().to_string(),
        output_dir: out_h.to_string_lossy().to_string(),
        inputs: Vec::new(),
        name_template: None,
        conflict: None,
        output_stem: Some("日志用例".into()),
        params: video_params(false),
    };
    match run_one(&ctx, spec, 30).await {
        Err(e) => fail(&mut results, "任务日志", e),
        Ok(j) => {
            let log_path = j.log_path.clone().unwrap_or_default();
            let exists = !log_path.is_empty() && Path::new(&log_path).exists();
            let content = std::fs::read_to_string(&log_path).unwrap_or_default();
            let has_header = content.contains("Transmutio 任务日志");
            let has_stderr = content.contains("--- ffmpeg stderr ---") && content.len() > 300;
            if j.status == "failed" && exists && has_header && has_stderr {
                pass(
                    &mut results,
                    "任务日志：失败任务落盘且含 ffmpeg stderr",
                    format!("{log_path}（{} 字节）", content.len()),
                );
            } else {
                fail(
                    &mut results,
                    "任务日志：失败任务落盘且含 ffmpeg stderr",
                    format!(
                        "status={} exists={exists} header={has_header} stderr={has_stderr} path={log_path:?}",
                        j.status
                    ),
                );
            }
        }
    }

    // ---------------- 本轮新增能力：真实产物断言 ----------------
    if ffprobe.is_none() {
        fail(
            &mut results,
            "新增能力检查",
            "缺少 ffprobe，无法断言产物".into(),
        );
    } else {
        let probe_bin = ffprobe.clone().unwrap_or_default();
        let merge_a = workspace.join("merge-a-4s.mp4");
        let merge_b = workspace.join("merge-b-4s.mp4");
        let merge_small = workspace.join("merge-small-4s.mp4");
        let audio_wav = workspace.join("audio-5s.wav");
        let exif_jpg = workspace.join("exif6-720p.jpg");
        let out_merge = workspace.join("out-merge");
        let out_mux = workspace.join("out-mux");
        let out_gif = workspace.join("out-gif");
        let out_exif = workspace.join("out-exif");
        let out_name = workspace.join("out-name");
        let out_after = workspace.join("out-after");
        for dir in [
            &out_merge, &out_mux, &out_gif, &out_exif, &out_name, &out_after,
        ] {
            let _ = std::fs::create_dir_all(dir);
        }
        let _ = make_source(&ffmpeg_path, &merge_a, 4);
        let _ = make_source(&ffmpeg_path, &merge_b, 4);
        let _ = make_source_sized(&ffmpeg_path, &merge_small, 4, 640, 360);
        let _ = make_audio_wav(&ffmpeg_path, &audio_wav, 5);
        let _ = run_ffmpeg(
            &ffmpeg_path,
            &[
                "-hide_banner".into(),
                "-loglevel".into(),
                "error".into(),
                "-y".into(),
                "-f".into(),
                "lavfi".into(),
                "-i".into(),
                "testsrc2=size=1280x720:rate=30".into(),
                "-frames:v".into(),
                "1".into(),
                "-q:v".into(),
                "2".into(),
                exif_jpg.to_string_lossy().to_string(),
            ],
        );
        if let Err(e) = inject_exif_orientation(&exif_jpg, 6) {
            fail(&mut results, "EXIF 合成样本", e);
        } else {
            pass(
                &mut results,
                "EXIF 合成样本",
                format!(
                    "{}（1280x720 JPEG，注入 Orientation=6；合成样本，非相机原图）",
                    exif_jpg.display()
                ),
            );
        }

        // 0) 文字水印：真实 drawtext（中文 + 系统字体 + filtergraph 特殊字符）
        let out_wm = workspace.join("out-watermark");
        let _ = std::fs::create_dir_all(&out_wm);
        // Windows / Linux（CI 安装 fonts-noto-cjk）/ macOS 的常见中文字体。
        const CJK_FONTS: [&str; 6] = [
            "C:\\Windows\\Fonts\\msyh.ttc",
            "C:\\Windows\\Fonts\\simhei.ttf",
            "/usr/share/fonts/opentype/noto/NotoSansCJK-Regular.ttc",
            "/usr/share/fonts/truetype/wqy/wqy-zenhei.ttc",
            "/System/Library/Fonts/PingFang.ttc",
            "/System/Library/Fonts/STHeiti Medium.ttc",
        ];
        let font = CJK_FONTS.into_iter().find(|p| Path::new(p).is_file());
        match font {
            None => fail(
                &mut results,
                "文字水印：中文 drawtext 真实烧入",
                format!("本机缺少中文字体（已查找 {}）", CJK_FONTS.join(" / ")),
            ),
            Some(font_path) => {
                let mut wm_params = video_params(false);
                wm_params["watermark"] = json!({
                    "enabled": true,
                    "mode": "text",
                    "text": "中文水印:测试[FF],2026",
                    "fontPath": font_path,
                    "fontSizePercent": 6,
                    "fontColor": "#FFFFFF",
                    "borderWidth": 2,
                    "borderColor": "#000000",
                    "position": "se",
                    "margin": 24,
                    "opacity": 0.9,
                    "scalePercent": 15,
                    "rotation": 0,
                    "x": 0,
                    "y": 0
                });
                let baseline = JobSpec {
                    kind: "video-mp4".into(),
                    input: src_short.to_string_lossy().to_string(),
                    inputs: Vec::new(),
                    output_dir: out_wm.to_string_lossy().to_string(),
                    output_stem: Some("水印-基线".into()),
                    name_template: None,
                    conflict: None,
                    params: video_params(false),
                };
                let watermarked = JobSpec {
                    output_stem: Some("水印-文字".into()),
                    params: wm_params,
                    ..baseline.clone()
                };
                match (
                    run_one(&ctx, baseline, 120).await,
                    run_one(&ctx, watermarked, 120).await,
                ) {
                    (Ok(base), Ok(wm)) if base.status == "done" && wm.status == "done" => {
                        let base_hash = first_frame_hash(&ffmpeg_path, Path::new(&base.output));
                        let wm_hash = first_frame_hash(&ffmpeg_path, Path::new(&wm.output));
                        match (base_hash, wm_hash) {
                            (Ok(b), Ok(w)) if b != w => pass(
                                &mut results,
                                "文字水印：中文 drawtext 真实烧入",
                                format!(
                                    "font={font_path}；首帧哈希 基线={b} 水印={w}（画面确已改变）；输出={}",
                                    wm.output
                                ),
                            ),
                            (Ok(b), Ok(_w)) => fail(
                                &mut results,
                                "文字水印：中文 drawtext 真实烧入",
                                format!("首帧哈希相同（{b}），水印未真正烧入；输出={}", wm.output),
                            ),
                            (a, b) => fail(
                                &mut results,
                                "文字水印：中文 drawtext 真实烧入",
                                format!("首帧哈希读取失败：{a:?} / {b:?}"),
                            ),
                        }
                    }
                    (a, b) => fail(
                        &mut results,
                        "文字水印：中文 drawtext 真实烧入",
                        format!(
                            "status={:?}/{:?} message={:?}/{:?}",
                            a.as_ref().map(|j| j.status.clone()),
                            b.as_ref().map(|j| j.status.clone()),
                            a.as_ref().ok().and_then(|j| j.message.clone()),
                            b.as_ref().ok().and_then(|j| j.message.clone())
                        ),
                    ),
                }

                // 缺字体必须在入队时就被拒绝，并给出可读原因（不能静默失败或产出一张原图）
                let missing_font = JobSpec {
                    output_stem: Some("水印-缺字体".into()),
                    params: json!({
                        "codec": "h264", "encoder": "auto", "rateMode": "quality", "quality": 26,
                        "bitrateKbps": 6000, "resolution": "720", "fps": "source",
                        "audioMode": "none", "audioBitrateKbps": 128,
                        "hwDecode": false, "faststart": true,
                        "watermark": {
                            "enabled": true, "mode": "text", "text": "缺字体",
                            "fontPath": out_wm.join("no-such-font.ttf").to_string_lossy(),
                            "fontSizePercent": 6, "fontColor": "#FFFFFF",
                            "borderWidth": 2, "borderColor": "#000000",
                            "position": "se", "margin": 24, "opacity": 0.9,
                            "scalePercent": 15, "rotation": 0, "x": 0, "y": 0
                        }
                    }),
                    ..JobSpec {
                        kind: "video-mp4".into(),
                        input: src_short.to_string_lossy().to_string(),
                        inputs: Vec::new(),
                        output_dir: out_wm.to_string_lossy().to_string(),
                        output_stem: None,
                        name_template: None,
                        conflict: None,
                        params: Value::Null,
                    }
                };
                match jobs::enqueue(&ctx, vec![missing_font]).await {
                    Err(e) if e.contains("字体文件不存在") => pass(
                        &mut results,
                        "文字水印：缺字体明确报错",
                        format!("入队即拒绝：{e}"),
                    ),
                    Err(e) => fail(
                        &mut results,
                        "文字水印：缺字体明确报错",
                        format!("被拒绝但原因不明确：{e}"),
                    ),
                    Ok(_) => fail(
                        &mut results,
                        "文字水印：缺字体明确报错",
                        "缺字体竟然入队成功（会静默失败）".into(),
                    ),
                }
            }
        }

        // 0.5) 视频「保持原始码率」：真实源码率探测 → 回填 -b:v → 产物 ffprobe 复核
        let br_src = workspace.join("bitrate-src-1200k.mp4");
        let out_br = workspace.join("out-bitrate");
        let _ = std::fs::create_dir_all(&out_br);
        match make_source_bitrate(&ffmpeg_path, &br_src, 8, 1200) {
            Err(e) => fail(
                &mut results,
                "视频保持原始码率：真实回填与产物复核",
                format!("生成固定码率样片失败：{e}"),
            ),
            Ok(_) => {
                let src_info =
                    crate::probe::probe_media(&probe_bin, &br_src.to_string_lossy()).await;
                let src_kbps = src_info.video_bitrate.unwrap_or(0) / 1000;
                if src_kbps < 800 {
                    fail(
                        &mut results,
                        "视频保持原始码率：真实回填与产物复核",
                        format!("样片源码率探测异常：{src_kbps} kbps（期望约 1200）"),
                    );
                } else {
                    let mut params = video_params(false);
                    params["rateMode"] = json!("source");
                    params["resolution"] = json!("source");
                    let spec = JobSpec {
                        kind: "video-mp4".into(),
                        input: br_src.to_string_lossy().to_string(),
                        inputs: Vec::new(),
                        output_dir: out_br.to_string_lossy().to_string(),
                        output_stem: Some("保持原始码率".into()),
                        name_template: None,
                        conflict: None,
                        params,
                    };
                    match run_one(&ctx, spec, 180).await {
                        Ok(job) if job.status == "done" => {
                            let info = crate::probe::probe_media(&probe_bin, &job.output).await;
                            let out_kbps = info.video_bitrate.unwrap_or(0) / 1000;
                            let low = src_kbps * 7 / 10;
                            let high = src_kbps * 3 / 2;
                            if out_kbps >= low && out_kbps <= high {
                                pass(
                                    &mut results,
                                    "视频保持原始码率：真实回填与产物复核",
                                    format!(
                                        "源 {src_kbps} kbps → 输出 {out_kbps} kbps（±30% 内）；输出={}",
                                        job.output
                                    ),
                                );
                            } else {
                                fail(
                                    &mut results,
                                    "视频保持原始码率：真实回填与产物复核",
                                    format!(
                                        "源 {src_kbps} kbps → 输出 {out_kbps} kbps，超出 [{low},{high}]"
                                    ),
                                );
                            }
                        }
                        Ok(job) => fail(
                            &mut results,
                            "视频保持原始码率：真实回填与产物复核",
                            format!(
                                "任务未完成：status={} message={:?}",
                                job.status, job.message
                            ),
                        ),
                        Err(e) => fail(
                            &mut results,
                            "视频保持原始码率：真实回填与产物复核",
                            format!("任务失败：{e}"),
                        ),
                    }
                }
            }
        }

        let merge_params = json!({
            "mode": "auto", "resolution": "source", "fps": "source",
            "audioMode": "none", "codec": "h264", "quality": 26, "bitrateKbps": 0
        });

        // 1) 同参合并 → concat copy
        let copy_spec = JobSpec {
            kind: "video-merge".into(),
            input: merge_a.to_string_lossy().to_string(),
            inputs: vec![merge_b.to_string_lossy().to_string()],
            output_dir: out_merge.to_string_lossy().to_string(),
            output_stem: Some("same-copy".into()),
            name_template: None,
            conflict: None,
            params: merge_params.clone(),
        };
        match run_one(&ctx, copy_spec, 180).await {
            Ok(job) => {
                let path = PathBuf::from(&job.output);
                let info_out = crate::probe::probe_media(&probe_bin, &path.to_string_lossy()).await;
                let duration = info_out.duration_sec.unwrap_or(0.0);
                let counts = stream_counts(&probe_bin, &path).unwrap_or((0, 0));
                let size_ok = info_out.width == Some(1280) && info_out.height == Some(720);
                if job.status == "done"
                    && (duration - 8.0).abs() < 0.6
                    && counts == (1, 0)
                    && size_ok
                {
                    pass(
                        &mut results,
                        "合并：同参 concat copy",
                        format!(
                            "duration={duration:.3}s video={} audio={} size={}x{} encoder={:?}",
                            counts.0,
                            counts.1,
                            info_out.width.unwrap_or(0),
                            info_out.height.unwrap_or(0),
                            job.encoder_used
                        ),
                    );
                } else {
                    fail(
                        &mut results,
                        "合并：同参 concat copy",
                        format!(
                            "status={} duration={duration:.3} streams={:?} size={:?} output={} message={}",
                            job.status,
                            counts,
                            (info_out.width, info_out.height),
                            job.output,
                            job.message.clone().unwrap_or_default()
                        ),
                    );
                }
            }
            Err(e) => fail(&mut results, "合并：同参 concat copy", e),
        }

        // 2) 不同参 → 自动统一转码
        let mixed_spec = JobSpec {
            kind: "video-merge".into(),
            input: merge_a.to_string_lossy().to_string(),
            inputs: vec![merge_small.to_string_lossy().to_string()],
            output_dir: out_merge.to_string_lossy().to_string(),
            output_stem: Some("mixed-transcode".into()),
            name_template: None,
            conflict: None,
            params: merge_params.clone(),
        };
        match run_one(&ctx, mixed_spec, 240).await {
            Ok(job) => {
                let path = PathBuf::from(&job.output);
                let info_out = crate::probe::probe_media(&probe_bin, &path.to_string_lossy()).await;
                let duration = info_out.duration_sec.unwrap_or(0.0);
                if job.status == "done"
                    && (duration - 8.0).abs() < 0.8
                    && info_out.width == Some(1280)
                    && info_out.height == Some(720)
                {
                    pass(
                        &mut results,
                        "合并：不同参统一转码",
                        format!(
                            "duration={duration:.3}s size={}x{} encoder={:?}",
                            info_out.width.unwrap_or(0),
                            info_out.height.unwrap_or(0),
                            job.encoder_used
                        ),
                    );
                } else {
                    fail(
                        &mut results,
                        "合并：不同参统一转码",
                        format!(
                            "status={} duration={duration:.3} size={:?} message={}",
                            job.status,
                            (info_out.width, info_out.height),
                            job.message.clone().unwrap_or_default()
                        ),
                    );
                }
            }
            Err(e) => fail(&mut results, "合并：不同参统一转码", e),
        }

        // 3) copy 模式遇到不同参 → 明确拒绝
        let reject_spec = JobSpec {
            kind: "video-merge".into(),
            input: merge_a.to_string_lossy().to_string(),
            inputs: vec![merge_small.to_string_lossy().to_string()],
            output_dir: out_merge.to_string_lossy().to_string(),
            output_stem: Some("reject-copy".into()),
            name_template: None,
            conflict: None,
            params: json!({
                "mode": "copy", "resolution": "source", "fps": "source",
                "audioMode": "none", "codec": "h264", "quality": 26, "bitrateKbps": 0
            }),
        };
        match run_one(&ctx, reject_spec, 60).await {
            Ok(job) => {
                let msg = job.message.clone().unwrap_or_default();
                let ok = job.status == "failed"
                    && job.error_kind.as_deref() == Some("invalid-params")
                    && msg.contains("不一致");
                if ok {
                    pass(&mut results, "合并：copy 模式拒绝不同参", msg);
                } else {
                    fail(
                        &mut results,
                        "合并：copy 模式拒绝不同参",
                        format!("status={} kind={:?} msg={msg}", job.status, job.error_kind),
                    );
                }
            }
            Err(e) => fail(&mut results, "合并：copy 模式拒绝不同参", e),
        }

        // 4) 混流：替换音轨（AAC）
        let mux_spec = JobSpec {
            kind: "video-mux".into(),
            input: src_short.to_string_lossy().to_string(),
            inputs: vec![audio_wav.to_string_lossy().to_string()],
            output_dir: out_mux.to_string_lossy().to_string(),
            output_stem: Some("replace-aac".into()),
            name_template: None,
            conflict: None,
            params: json!({"mode": "replace", "audioMode": "aac", "audioBitrateKbps": 192}),
        };
        match run_one(&ctx, mux_spec, 120).await {
            Ok(job) => {
                let path = PathBuf::from(&job.output);
                let counts = stream_counts(&probe_bin, &path).unwrap_or((0, 0));
                let info_out = crate::probe::probe_media(&probe_bin, &path.to_string_lossy()).await;
                let duration = info_out.duration_sec.unwrap_or(0.0);
                if job.status == "done" && counts == (1, 1) && (duration - 8.0).abs() < 0.4 {
                    pass(
                        &mut results,
                        "混流：替换音轨（AAC）",
                        format!(
                            "duration={duration:.3}s video={} audio={}",
                            counts.0, counts.1
                        ),
                    );
                } else {
                    fail(
                        &mut results,
                        "混流：替换音轨（AAC）",
                        format!(
                            "status={} duration={duration:.3} streams={:?}",
                            job.status, counts
                        ),
                    );
                }
            }
            Err(e) => fail(&mut results, "混流：替换音轨（AAC）", e),
        }

        // 4b) 混流：新增音轨（基线 1 条音轨 → 输出 2 条）
        let mux_base = out_mux.join("base-with-audio.mp4");
        let base_args = vec![
            "-hide_banner".into(),
            "-loglevel".into(),
            "error".into(),
            "-y".into(),
            "-i".into(),
            src_short.to_string_lossy().to_string(),
            "-i".into(),
            audio_wav.to_string_lossy().to_string(),
            "-map".into(),
            "0:v:0".into(),
            "-map".into(),
            "1:a:0".into(),
            "-c:v".into(),
            "copy".into(),
            "-c:a".into(),
            "aac".into(),
            "-movflags".into(),
            "+faststart".into(),
            "-f".into(),
            "mp4".into(),
            mux_base.to_string_lossy().to_string(),
        ];
        match run_ffmpeg(&ffmpeg_path, &base_args) {
            Ok(()) => {
                let mux_add_spec = JobSpec {
                    kind: "video-mux".into(),
                    input: mux_base.to_string_lossy().to_string(),
                    inputs: vec![audio_wav.to_string_lossy().to_string()],
                    output_dir: out_mux.to_string_lossy().to_string(),
                    output_stem: Some("add-aac".into()),
                    name_template: None,
                    conflict: None,
                    params: json!({"mode": "add", "audioMode": "aac", "audioBitrateKbps": 128}),
                };
                match run_one(&ctx, mux_add_spec, 120).await {
                    Ok(job) => {
                        let path = PathBuf::from(&job.output);
                        let counts = stream_counts(&probe_bin, &path).unwrap_or((0, 0));
                        let info_out =
                            crate::probe::probe_media(&probe_bin, &path.to_string_lossy()).await;
                        let duration = info_out.duration_sec.unwrap_or(0.0);
                        if job.status == "done" && counts == (1, 2) && (duration - 8.0).abs() < 0.4
                        {
                            pass(
                                &mut results,
                                "混流：新增第二条音轨",
                                format!(
                                    "duration={duration:.3}s video={} audio={}",
                                    counts.0, counts.1
                                ),
                            );
                        } else {
                            fail(
                                &mut results,
                                "混流：新增第二条音轨",
                                format!(
                                    "status={} duration={duration:.3} streams={:?}",
                                    job.status, counts
                                ),
                            );
                        }
                    }
                    Err(e) => fail(&mut results, "混流：新增第二条音轨", e),
                }
            }
            Err(e) => fail(&mut results, "混流：新增第二条音轨（基线素材）", e),
        }

        // 5) 混流：copy 遇到 PCM → 明确拒绝
        let mux_copy_spec = JobSpec {
            kind: "video-mux".into(),
            input: src_short.to_string_lossy().to_string(),
            inputs: vec![audio_wav.to_string_lossy().to_string()],
            output_dir: out_mux.to_string_lossy().to_string(),
            output_stem: Some("copy-pcm".into()),
            name_template: None,
            conflict: None,
            params: json!({"mode": "replace", "audioMode": "copy", "audioBitrateKbps": 0}),
        };
        match run_one(&ctx, mux_copy_spec, 60).await {
            Ok(job) => {
                let msg = job.message.clone().unwrap_or_default();
                if job.status == "failed" && msg.contains("不能直接复制") {
                    pass(&mut results, "混流：copy 拒绝不兼容音频", msg);
                } else {
                    fail(
                        &mut results,
                        "混流：copy 拒绝不兼容音频",
                        format!("status={} msg={msg}", job.status),
                    );
                }
            }
            Err(e) => fail(&mut results, "混流：copy 拒绝不兼容音频", e),
        }

        // 6) 音频合并：两种采样率/声道不同的素材，统一后按顺序拼接
        let merge_audio_a = workspace.join("audio-merge-a-2s.wav");
        let merge_audio_b = workspace.join("audio-merge-b-3s.wav");
        let make_merge_a: Vec<String> = vec![
            "-hide_banner".into(),
            "-loglevel".into(),
            "error".into(),
            "-y".into(),
            "-f".into(),
            "lavfi".into(),
            "-i".into(),
            "sine=frequency=440:sample_rate=44100:duration=2".into(),
            "-ar".into(),
            "44100".into(),
            "-ac".into(),
            "2".into(),
            merge_audio_a.to_string_lossy().to_string(),
        ];
        let make_merge_b: Vec<String> = vec![
            "-hide_banner".into(),
            "-loglevel".into(),
            "error".into(),
            "-y".into(),
            "-f".into(),
            "lavfi".into(),
            "-i".into(),
            "sine=frequency=660:sample_rate=48000:duration=3".into(),
            "-ar".into(),
            "48000".into(),
            "-ac".into(),
            "1".into(),
            merge_audio_b.to_string_lossy().to_string(),
        ];
        if let Err(e) = run_ffmpeg(&ffmpeg_path, &make_merge_a)
            .and_then(|_| run_ffmpeg(&ffmpeg_path, &make_merge_b))
        {
            fail(&mut results, "音频合并", format!("生成素材失败：{e}"));
        } else {
            let out_am = workspace.join("out-audio-merge");
            let _ = std::fs::create_dir_all(&out_am);
            let spec = JobSpec {
                kind: "audio-merge".into(),
                input: merge_audio_a.to_string_lossy().to_string(),
                inputs: vec![merge_audio_b.to_string_lossy().to_string()],
                output_dir: out_am.to_string_lossy().to_string(),
                output_stem: Some("合并音频".into()),
                name_template: None,
                conflict: None,
                params: json!({
                    "format": "mp3", "bitrateKbps": 128, "rateMode": "bitrate",
                    "sampleRate": "44100", "channels": "stereo", "normalize": false
                }),
            };
            match run_one(&ctx, spec, 120).await {
                Ok(job) => {
                    let info_out = crate::probe::probe_media(&probe_bin, &job.output).await;
                    let duration = info_out.duration_sec.unwrap_or(0.0);
                    let channels = info_out.channels.unwrap_or(0);
                    let rate = info_out.sample_rate.unwrap_or(0);
                    if job.status == "done"
                        && (duration - 5.0).abs() < 0.35
                        && channels == 2
                        && rate == 44100
                    {
                        pass(
                            &mut results,
                            "音频合并：统一采样率/声道后按顺序拼接",
                            format!(
                                "duration={duration:.3}s（期望 5.000）sample_rate={rate} channels={channels} codec={:?}",
                                info_out.audio_codec
                            ),
                        );
                    } else {
                        fail(
                            &mut results,
                            "音频合并：统一采样率/声道后按顺序拼接",
                            format!(
                                "status={} duration={duration:.3}s sample_rate={rate} channels={channels} msg={:?}",
                                job.status, job.message
                            ),
                        );
                    }
                }
                Err(e) => fail(&mut results, "音频合并", e),
            }
        }

        // 6c) 音频混音：两路同时混合，时长取最长的一路，等权衰减不削波
        let out_amix = workspace.join("out-audio-mix");
        let _ = std::fs::create_dir_all(&out_amix);
        let mix_spec = JobSpec {
            kind: "audio-mix".into(),
            input: merge_audio_a.to_string_lossy().to_string(),
            inputs: vec![merge_audio_b.to_string_lossy().to_string()],
            output_dir: out_amix.to_string_lossy().to_string(),
            output_stem: Some("混音音频".into()),
            name_template: None,
            conflict: None,
            params: json!({
                "format": "mp3", "bitrateKbps": 128, "rateMode": "bitrate",
                "sampleRate": "44100", "channels": "stereo", "normalize": false
            }),
        };
        match run_one(&ctx, mix_spec, 120).await {
            Ok(job) => {
                let info_out = crate::probe::probe_media(&probe_bin, &job.output).await;
                let duration = info_out.duration_sec.unwrap_or(0.0);
                let channels = info_out.channels.unwrap_or(0);
                let rate = info_out.sample_rate.unwrap_or(0);
                let peak = max_volume_db(&ffmpeg_path, Path::new(&job.output));
                let peak_ok = matches!(&peak, Ok(p) if *p > -30.0 && *p <= 0.5);
                let ok = job.status == "done"
                    && (duration - 3.0).abs() < 0.35
                    && channels == 2
                    && rate == 44100
                    && peak_ok;
                let detail = format!(
                    "duration={duration:.3}s（取最长一路，期望 3.000）sample_rate={rate} channels={channels} peak={peak:?} dB（应 > -30 且 ≤ 0）codec={:?}",
                    info_out.audio_codec
                );
                if ok {
                    pass(&mut results, "音频混音：等权混合取最长时长且不削波", detail);
                } else {
                    fail(&mut results, "音频混音：等权混合取最长时长且不削波", detail);
                }
            }
            Err(e) => fail(&mut results, "音频混音", e),
        }

        // 7) 音频截取：3 秒素材取 [0.5, 2.5)，期望 2.000 秒
        let out_at = workspace.join("out-audio-trim");
        let _ = std::fs::create_dir_all(&out_at);
        let trim_spec = JobSpec {
            kind: "audio-trim".into(),
            input: merge_audio_b.to_string_lossy().to_string(),
            inputs: Vec::new(),
            output_dir: out_at.to_string_lossy().to_string(),
            output_stem: Some("截取音频".into()),
            name_template: None,
            conflict: None,
            params: json!({
                "format": "mp3", "bitrateKbps": 128, "rateMode": "bitrate",
                "sampleRate": "source", "channels": "source", "normalize": false,
                "startSec": 0.5, "endSec": 2.5
            }),
        };
        match run_one(&ctx, trim_spec, 120).await {
            Ok(job) => {
                let info_out = crate::probe::probe_media(&probe_bin, &job.output).await;
                let duration = info_out.duration_sec.unwrap_or(0.0);
                if job.status == "done" && (duration - 2.0).abs() < 0.25 {
                    pass(
                        &mut results,
                        "音频截取：按时间区间准确截断",
                        format!(
                            "duration={duration:.3}s（期望 2.000）sample_rate={:?} codec={:?}",
                            info_out.sample_rate, info_out.audio_codec
                        ),
                    );
                } else {
                    fail(
                        &mut results,
                        "音频截取：按时间区间准确截断",
                        format!(
                            "status={} duration={duration:.3}s msg={:?}",
                            job.status, job.message
                        ),
                    );
                }
            }
            Err(e) => fail(&mut results, "音频截取", e),
        }

        // 8) GIF：调色板优化
        let gif_spec = JobSpec {
            kind: "video-gif".into(),
            input: src_short.to_string_lossy().to_string(),
            inputs: Vec::new(),
            output_dir: out_gif.to_string_lossy().to_string(),
            output_stem: Some("palette".into()),
            name_template: None,
            conflict: None,
            params: json!({
                "fps": 10, "width": 320, "startSec": 1.0, "durationSec": 2.0, "optimize": true
            }),
        };
        match run_one(&ctx, gif_spec, 120).await {
            Ok(job) => {
                let path = PathBuf::from(&job.output);
                let frames = frame_count(&probe_bin, &path).unwrap_or(0);
                let info_out = crate::probe::probe_media(&probe_bin, &path.to_string_lossy()).await;
                let width = info_out.width.unwrap_or(0);
                if job.status == "done" && (18..=22).contains(&frames) && width == 320 {
                    pass(
                        &mut results,
                        "GIF：调色板优化",
                        format!(
                            "frames={frames} width={width} size={}",
                            job.output_size.unwrap_or(0)
                        ),
                    );
                } else {
                    fail(
                        &mut results,
                        "GIF：调色板优化",
                        format!("status={} frames={frames} width={width}", job.status),
                    );
                }
            }
            Err(e) => fail(&mut results, "GIF：调色板优化", e),
        }

        // 6b) GIF：直接转换（无调色板两遍），证明两条路线都能产出合法产物
        let gif_direct_spec = JobSpec {
            kind: "video-gif".into(),
            input: src_short.to_string_lossy().to_string(),
            inputs: Vec::new(),
            output_dir: out_gif.to_string_lossy().to_string(),
            output_stem: Some("direct".into()),
            name_template: None,
            conflict: None,
            params: json!({
                "fps": 10, "width": 320, "startSec": 1.0, "durationSec": 2.0, "optimize": false
            }),
        };
        match run_one(&ctx, gif_direct_spec, 120).await {
            Ok(job) => {
                let path = PathBuf::from(&job.output);
                let frames = frame_count(&probe_bin, &path).unwrap_or(0);
                let info_out = crate::probe::probe_media(&probe_bin, &path.to_string_lossy()).await;
                let width = info_out.width.unwrap_or(0);
                if job.status == "done" && (18..=22).contains(&frames) && width == 320 {
                    pass(
                        &mut results,
                        "GIF：直接转换（无调色板优化）",
                        format!(
                            "frames={frames} width={width} size={}",
                            job.output_size.unwrap_or(0)
                        ),
                    );
                } else {
                    fail(
                        &mut results,
                        "GIF：直接转换（无调色板优化）",
                        format!("status={} frames={frames} width={width}", job.status),
                    );
                }
            }
            Err(e) => fail(&mut results, "GIF：直接转换（无调色板优化）", e),
        }

        // 7) 图片 EXIF 方向：自动校正一次（不叠加 ffmpeg 隐式旋转）
        let exif_on = JobSpec {
            kind: "image".into(),
            input: exif_jpg.to_string_lossy().to_string(),
            inputs: Vec::new(),
            output_dir: out_exif.to_string_lossy().to_string(),
            output_stem: Some("orient-on".into()),
            name_template: None,
            conflict: None,
            params: json!({"format": "jpg", "quality": 90, "maxEdge": 0, "rotation": "0", "autoOrient": true}),
        };
        match run_one(&ctx, exif_on, 60).await {
            Ok(job) => {
                let info_out = crate::probe::probe_media(&probe_bin, &job.output).await;
                if job.status == "done"
                    && info_out.width == Some(720)
                    && info_out.height == Some(1280)
                {
                    pass(
                        &mut results,
                        "图片 EXIF：自动校正为 720x1280（恰好一次 90°）",
                        format!(
                            "size={}x{}",
                            info_out.width.unwrap_or(0),
                            info_out.height.unwrap_or(0)
                        ),
                    );
                } else {
                    fail(
                        &mut results,
                        "图片 EXIF：自动校正为 720x1280（恰好一次 90°）",
                        format!(
                            "status={} size={:?}",
                            job.status,
                            (info_out.width, info_out.height)
                        ),
                    );
                }
            }
            Err(e) => fail(
                &mut results,
                "图片 EXIF：自动校正为 720x1280（恰好一次 90°）",
                e,
            ),
        }
        let exif_off = JobSpec {
            kind: "image".into(),
            input: exif_jpg.to_string_lossy().to_string(),
            inputs: Vec::new(),
            output_dir: out_exif.to_string_lossy().to_string(),
            output_stem: Some("orient-off".into()),
            name_template: None,
            conflict: None,
            params: json!({"format": "jpg", "quality": 90, "maxEdge": 0, "rotation": "0", "autoOrient": false}),
        };
        match run_one(&ctx, exif_off, 60).await {
            Ok(job) => {
                let info_out = crate::probe::probe_media(&probe_bin, &job.output).await;
                if job.status == "done"
                    && info_out.width == Some(1280)
                    && info_out.height == Some(720)
                {
                    pass(
                        &mut results,
                        "图片 EXIF：关闭自动校正时保留原始像素方向",
                        format!(
                            "size={}x{}",
                            info_out.width.unwrap_or(0),
                            info_out.height.unwrap_or(0)
                        ),
                    );
                } else {
                    fail(
                        &mut results,
                        "图片 EXIF：关闭自动校正时保留原始像素方向",
                        format!(
                            "status={} size={:?}",
                            job.status,
                            (info_out.width, info_out.height)
                        ),
                    );
                }
            }
            Err(e) => fail(&mut results, "图片 EXIF：关闭自动校正时保留原始像素方向", e),
        }

        // 7b) EXIF 自动校正 + 手动旋转同时存在：只应叠加一次（6=90° + 手动 90° = 180°）
        // 若 ffmpeg 隐式 autorotate 没被关掉，会变成 270°（720x1280），此断言即会失败。
        let exif_both = JobSpec {
            kind: "image".into(),
            input: exif_jpg.to_string_lossy().to_string(),
            inputs: Vec::new(),
            output_dir: out_exif.to_string_lossy().to_string(),
            output_stem: Some("orient-manual".into()),
            name_template: None,
            conflict: None,
            params: json!({"format": "jpg", "quality": 90, "maxEdge": 0, "rotation": "90", "autoOrient": true}),
        };
        match run_one(&ctx, exif_both, 60).await {
            Ok(job) => {
                let info_out = crate::probe::probe_media(&probe_bin, &job.output).await;
                if job.status == "done"
                    && info_out.width == Some(1280)
                    && info_out.height == Some(720)
                {
                    pass(
                        &mut results,
                        "图片 EXIF：自动校正 + 手动旋转只叠加一次",
                        format!(
                            "size={}x{}（EXIF 90° + 手动 90° = 180°，未发生二次旋转）",
                            info_out.width.unwrap_or(0),
                            info_out.height.unwrap_or(0)
                        ),
                    );
                } else {
                    fail(
                        &mut results,
                        "图片 EXIF：自动校正 + 手动旋转只叠加一次",
                        format!(
                            "status={} size={:?}（720x1280 说明被转了三次）",
                            job.status,
                            (info_out.width, info_out.height)
                        ),
                    );
                }
            }
            Err(e) => fail(&mut results, "图片 EXIF：自动校正 + 手动旋转只叠加一次", e),
        }

        // 8) 命名模板 + 三种冲突策略
        // 先用例自己的产物（上一轮残留或并发运行可能留下 模板-1.mp4），
        // 否则 {index} 会撞号变成「模板-1 (1).mp4」，把本地残留误判成功能缺陷。
        if let Ok(entries) = std::fs::read_dir(&out_name) {
            for entry in entries.flatten() {
                let name = entry.file_name().to_string_lossy().to_string();
                if name.starts_with("模板-") && name.ends_with(".mp4") {
                    let _ = std::fs::remove_file(entry.path());
                }
            }
        }
        match jobs::enqueue(
            &ctx,
            vec![
                JobSpec {
                    kind: "video-mp4".into(),
                    input: src_short.to_string_lossy().to_string(),
                    inputs: Vec::new(),
                    output_dir: out_name.to_string_lossy().to_string(),
                    output_stem: Some("模板".into()),
                    name_template: Some("{name}-{index}.{ext}".into()),
                    conflict: None,
                    params: video_params(false),
                },
                JobSpec {
                    kind: "video-mp4".into(),
                    input: src_short.to_string_lossy().to_string(),
                    inputs: Vec::new(),
                    output_dir: out_name.to_string_lossy().to_string(),
                    output_stem: Some("模板".into()),
                    name_template: Some("{name}-{index}.{ext}".into()),
                    conflict: None,
                    params: video_params(false),
                },
            ],
        )
        .await
        {
            Ok(created) => {
                let names: Vec<String> = created
                    .iter()
                    .map(|j| {
                        Path::new(&j.output)
                            .file_name()
                            .map(|n| n.to_string_lossy().to_string())
                            .unwrap_or_default()
                    })
                    .collect();
                if names == vec!["模板-1.mp4".to_string(), "模板-2.mp4".to_string()] {
                    pass(
                        &mut results,
                        "命名模板：{name}-{index}.{ext}",
                        names.join(" / "),
                    );
                } else {
                    fail(
                        &mut results,
                        "命名模板：{name}-{index}.{ext}",
                        names.join(" / "),
                    );
                }
            }
            Err(e) => fail(&mut results, "命名模板：{name}-{index}.{ext}", e),
        }

        // suffix：已存在同名文件时必须新增而不是覆盖
        let suffix_target = out_name.join("同名.mp4");
        std::fs::write(&suffix_target, b"sentinel-original").unwrap();
        if let Ok(created) = jobs::enqueue(
            &ctx,
            vec![JobSpec {
                kind: "video-mp4".into(),
                input: src_short.to_string_lossy().to_string(),
                inputs: Vec::new(),
                output_dir: out_name.to_string_lossy().to_string(),
                output_stem: Some("同名".into()),
                name_template: None,
                conflict: None,
                params: video_params(false),
            }],
        )
        .await
        {
            let out = created[0].output.clone();
            let preserved = std::fs::read(&suffix_target)
                .map(|b| b == b"sentinel-original")
                .unwrap_or(false);
            if out != suffix_target.to_string_lossy() && preserved {
                pass(
                    &mut results,
                    "冲突策略：suffix 不覆盖已有文件",
                    format!("新文件={out}；原文件内容保持不变"),
                );
            } else {
                fail(
                    &mut results,
                    "冲突策略：suffix 不覆盖已有文件",
                    format!("out={out} 已被覆盖={}", !preserved),
                );
            }
        }

        // skip：目标存在时直接跳过
        let skip_target = out_name.join("跳过.mp4");
        std::fs::write(&skip_target, b"sentinel-skip").unwrap();
        match jobs::enqueue(
            &ctx,
            vec![JobSpec {
                kind: "video-mp4".into(),
                input: src_short.to_string_lossy().to_string(),
                inputs: Vec::new(),
                output_dir: out_name.to_string_lossy().to_string(),
                output_stem: Some("跳过".into()),
                name_template: None,
                conflict: Some("skip".into()),
                params: video_params(false),
            }],
        )
        .await
        {
            Ok(created) => {
                let job = &created[0];
                let preserved = std::fs::read(&skip_target)
                    .map(|b| b == b"sentinel-skip")
                    .unwrap_or(false);
                if job.status == "skipped" && preserved {
                    pass(
                        &mut results,
                        "冲突策略：skip 跳过且不覆盖",
                        job.message.clone().unwrap_or_default(),
                    );
                } else {
                    fail(
                        &mut results,
                        "冲突策略：skip 跳过且不覆盖",
                        format!("status={} preserved={preserved}", job.status),
                    );
                }
            }
            Err(e) => fail(&mut results, "冲突策略：skip 跳过且不覆盖", e),
        }

        // overwrite：显式策略才替换
        let overwrite_target = out_name.join("替换.mp4");
        std::fs::write(&overwrite_target, b"sentinel-overwrite").unwrap();
        match run_one(
            &ctx,
            JobSpec {
                kind: "video-mp4".into(),
                input: src_short.to_string_lossy().to_string(),
                inputs: Vec::new(),
                output_dir: out_name.to_string_lossy().to_string(),
                output_stem: Some("替换".into()),
                name_template: None,
                conflict: Some("overwrite".into()),
                params: video_params(false),
            },
            120,
        )
        .await
        {
            Ok(job) => {
                let replaced = std::fs::read(&overwrite_target)
                    .map(|b| b != b"sentinel-overwrite")
                    .unwrap_or(false);
                let leftover = std::fs::read_dir(&out_name)
                    .map(|rd| {
                        rd.filter_map(|e| e.ok())
                            .any(|e| e.file_name().to_string_lossy().contains("ffbak"))
                    })
                    .unwrap_or(true);
                if job.status == "done" && replaced && !leftover {
                    pass(
                        &mut results,
                        "冲突策略：overwrite 显式替换且不留备份",
                        format!("{}（{} 字节）", job.output, job.output_size.unwrap_or(0)),
                    );
                } else {
                    fail(
                        &mut results,
                        "冲突策略：overwrite 显式替换且不留备份",
                        format!(
                            "status={} replaced={replaced} leftover={leftover} policy={} output={} message={}",
                            job.status,
                            job.conflict,
                            job.output,
                            job.message.clone().unwrap_or_default()
                        ),
                    );
                }
            }
            Err(e) => fail(&mut results, "冲突策略：overwrite 显式替换且不留备份", e),
        }

        // 9) 队列级暂停 / 继续
        jobs::set_concurrency(&ctx, 1).await;
        let long_spec = JobSpec {
            kind: "video-mp4".into(),
            input: src.to_string_lossy().to_string(),
            inputs: Vec::new(),
            output_dir: workspace.join("out-pause").to_string_lossy().to_string(),
            output_stem: Some("暂停-长".into()),
            name_template: None,
            conflict: None,
            params: video_params(false),
        };
        let short_spec = JobSpec {
            kind: "video-mp4".into(),
            input: src_short.to_string_lossy().to_string(),
            inputs: Vec::new(),
            output_dir: workspace.join("out-pause").to_string_lossy().to_string(),
            output_stem: Some("暂停-短".into()),
            name_template: None,
            conflict: None,
            params: video_params(false),
        };
        let _ = std::fs::create_dir_all(workspace.join("out-pause"));
        if let Ok(batch) = jobs::enqueue(&ctx, vec![long_spec, short_spec]).await {
            let long_id = batch[0].id.clone();
            let short_id = batch[1].id.clone();
            let _ = wait_until(
                &ctx,
                Duration::from_secs(30),
                |all| all.iter().any(|j| j.id == long_id && j.status == "running"),
                "长任务开始运行",
            )
            .await;
            jobs::set_paused(&ctx, true).await;
            tokio::time::sleep(Duration::from_millis(1200)).await;
            let snapshot = jobs::list(&ctx).await;
            let short_queued = snapshot
                .iter()
                .find(|j| j.id == short_id)
                .map(|j| j.status == "queued")
                .unwrap_or(false);
            let long_running = snapshot
                .iter()
                .find(|j| j.id == long_id)
                .map(|j| j.status == "running")
                .unwrap_or(false);
            let paused_ok = jobs::is_paused(&ctx).await;
            if short_queued && long_running && paused_ok {
                pass(
                    &mut results,
                    "暂停：暂停后不再调度新任务且运行中任务继续",
                    "短任务保持 queued；长任务保持 running".into(),
                );
            } else {
                fail(
                    &mut results,
                    "暂停：暂停后不再调度新任务且运行中任务继续",
                    format!(
                        "shortQueued={short_queued} longRunning={long_running} paused={paused_ok}"
                    ),
                );
            }
            // 取消长任务以缩短检查时长；暂停状态下短任务不应被启动。
            let _ = jobs::cancel(&ctx, &long_id).await;
            tokio::time::sleep(Duration::from_millis(600)).await;
            let still_queued = jobs::list(&ctx)
                .await
                .iter()
                .find(|j| j.id == short_id)
                .map(|j| j.status == "queued")
                .unwrap_or(false);
            jobs::set_paused(&ctx, false).await;
            let finished = wait_until(
                &ctx,
                Duration::from_secs(120),
                |all| {
                    all.iter()
                        .filter(|j| j.id == short_id)
                        .all(|j| j.status == "done" || j.status == "failed")
                },
                "继续后短任务结束",
            )
            .await;
            match finished {
                Ok(all) => {
                    let short_done = all
                        .iter()
                        .find(|j| j.id == short_id)
                        .map(|j| j.status == "done")
                        .unwrap_or(false);
                    if still_queued && short_done {
                        pass(
                            &mut results,
                            "继续：恢复后队列继续执行",
                            "暂停期间短任务未启动，恢复后完成".into(),
                        );
                    } else {
                        fail(
                            &mut results,
                            "继续：恢复后队列继续执行",
                            format!("stillQueued={still_queued} shortDone={short_done}"),
                        );
                    }
                }
                Err(e) => fail(&mut results, "继续：恢复后队列继续执行", e),
            }
        }
        jobs::set_concurrency(&ctx, 4).await;

        // 10) 完成后动作：open-folder 回调（只记录，不真正打开窗口）
        jobs::set_after_action(&ctx, "open-folder").await;
        match run_one(
            &ctx,
            JobSpec {
                kind: "video-mp4".into(),
                input: src_short.to_string_lossy().to_string(),
                inputs: Vec::new(),
                output_dir: out_after.to_string_lossy().to_string(),
                output_stem: Some("完成后动作".into()),
                name_template: None,
                conflict: None,
                params: video_params(false),
            },
            120,
        )
        .await
        {
            Ok(job) => {
                let mut calls = after_calls.lock().map(|g| g.clone()).unwrap_or_default();
                // 批次结束回调发生在 finish_slot 里，可能晚于 status=done 观测到；最多等 2s。
                for _ in 0..100 {
                    if calls.iter().any(|(action, dir)| {
                        action == "open-folder" && dir == &out_after.to_string_lossy().to_string()
                    }) {
                        break;
                    }
                    tokio::time::sleep(std::time::Duration::from_millis(20)).await;
                    calls = after_calls.lock().map(|g| g.clone()).unwrap_or_default();
                }
                let hit = calls.iter().rev().find(|(action, dir)| {
                    action == "open-folder" && dir == &out_after.to_string_lossy().to_string()
                });
                if job.status == "done" && hit.is_some() {
                    pass(
                        &mut results,
                        "完成后动作：open-folder 回调（注入记录器）",
                        format!(
                            "action={:?} dir={:?}",
                            hit.map(|h| h.0.clone()),
                            hit.map(|h| h.1.clone())
                        ),
                    );
                } else {
                    fail(
                        &mut results,
                        "完成后动作：open-folder 回调（注入记录器）",
                        format!("status={} calls={calls:?}", job.status),
                    );
                }
            }
            Err(e) => fail(
                &mut results,
                "完成后动作：open-folder 回调（注入记录器）",
                e,
            ),
        }
        jobs::set_after_action(&ctx, "none").await;

        // 11) 关机：只做命令构造断言，绝不执行
        let shutdown = crate::after::shutdown_command(60);
        // Windows 走 `shutdown /s /t 60`；Linux 走 `shutdown -h +1`（分秒语义不同，断言按平台分开）。
        let built = if cfg!(windows) {
            shutdown.program == "shutdown"
                && shutdown.args.iter().any(|a| a == "/s")
                && shutdown.args.iter().any(|a| a == "60")
        } else {
            shutdown.program == "shutdown" && shutdown.args.iter().any(|a| a == "-h")
        };
        if built && !crate::after::shutdown_enabled() {
            pass(
                &mut results,
                "完成后动作：关机仅构造命令（未执行）",
                format!(
                    "{} {:?}；环境变量未开启，绝不执行",
                    shutdown.program, shutdown.args
                ),
            );
        } else {
            fail(
                &mut results,
                "完成后动作：关机仅构造命令（未执行）",
                format!(
                    "program={} args={:?} enabled={}",
                    shutdown.program,
                    shutdown.args,
                    crate::after::shutdown_enabled()
                ),
            );
        }
    }

    finish(report_path, info, results).await
}

trait JobHelpers {
    fn queue_like(&self) -> bool;
}

impl JobHelpers for Job {
    fn queue_like(&self) -> bool {
        self.status == "queued" || self.status == "running"
    }
}

async fn finish(report_path: &str, info: tools::ToolsInfo, results: Vec<Check>) -> i32 {
    let passed = results.iter().filter(|c| c.status == "pass").count();
    let failed = results.iter().filter(|c| c.status == "fail").count();
    let report = Report {
        generated_at: now_ms(),
        level: LEVEL.into(),
        environment: json!({
            "platform": info.platform,
            "arch": info.arch,
            "ffmpeg": info.ffmpeg.as_ref().map(|t| t.path.clone()),
            "ffmpegVersion": info.ffmpeg_version,
            "gpl": info.build.gpl,
            "nonfree": info.build.nonfree,
            "note": "headless 检查使用系统 ffmpeg（开发机为 GPL 构建）；能力表为手工注入，真实探测由 verify-mvp 覆盖"
        }),
        summary: Summary {
            passed,
            failed,
            total: results.len(),
        },
        results,
    };
    if let Err(e) = write_report(report_path, &report) {
        eprintln!("写报告失败：{e}");
        return 1;
    }
    println!(
        "headless 检查完成：{} PASS / {} FAIL（报告：{report_path}）",
        passed, failed
    );
    if failed > 0 {
        1
    } else {
        0
    }
}

fn write_report(path: &str, report: &Report) -> std::io::Result<()> {
    if let Some(parent) = Path::new(path).parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)?;
        }
    }
    std::fs::write(path, serde_json::to_string_pretty(report)?)
}
