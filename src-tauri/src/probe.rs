use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::path::PathBuf;
use std::process::Stdio;
use std::time::Duration;
use tauri::Manager;
use tokio::process::Command;

use crate::tools::ToolsInfo;

/// 许可分级：默认安装包只允许 lgpl。
const LICENSE_LGPL: &str = "lgpl";
const LICENSE_GPL: &str = "gpl";
const LICENSE_NONFREE: &str = "nonfree";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EncoderCap {
    pub id: String,
    pub codec: String,
    pub label: String,
    pub vendor: String,
    pub hardware: bool,
    pub license: String,
    pub allowed: bool,
    pub compiled: bool,
    pub available: bool,
    pub reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DecodeCap {
    pub id: String,
    pub label: String,
    pub available: bool,
    pub reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Capabilities {
    pub probed_at: i64,
    pub cache_hit: bool,
    pub fingerprint: String,
    pub platform: String,
    pub arch: String,
    pub ffmpeg_version: String,
    pub build_gpl: bool,
    pub build_nonfree: bool,
    pub video_encoders: Vec<EncoderCap>,
    pub audio_encoders: Vec<EncoderCap>,
    pub decode_accels: Vec<DecodeCap>,
    pub software_h264_available: bool,
    pub notes: Vec<String>,
}

struct Candidate {
    id: &'static str,
    codec: &'static str,
    label: &'static str,
    vendor: &'static str,
    hardware: bool,
    license: &'static str,
}

const VIDEO_CANDIDATES: &[Candidate] = &[
    Candidate {
        id: "h264_videotoolbox",
        codec: "h264",
        label: "Apple VideoToolbox · H.264",
        vendor: "videotoolbox",
        hardware: true,
        license: LICENSE_LGPL,
    },
    Candidate {
        id: "hevc_videotoolbox",
        codec: "hevc",
        label: "Apple VideoToolbox · HEVC",
        vendor: "videotoolbox",
        hardware: true,
        license: LICENSE_LGPL,
    },
    Candidate {
        id: "h264_nvenc",
        codec: "h264",
        label: "NVIDIA NVENC · H.264",
        vendor: "nvenc",
        hardware: true,
        license: LICENSE_LGPL,
    },
    Candidate {
        id: "hevc_nvenc",
        codec: "hevc",
        label: "NVIDIA NVENC · HEVC",
        vendor: "nvenc",
        hardware: true,
        license: LICENSE_LGPL,
    },
    Candidate {
        id: "av1_nvenc",
        codec: "av1",
        label: "NVIDIA NVENC · AV1",
        vendor: "nvenc",
        hardware: true,
        license: LICENSE_LGPL,
    },
    Candidate {
        id: "h264_amf",
        codec: "h264",
        label: "AMD AMF · H.264",
        vendor: "amf",
        hardware: true,
        license: LICENSE_LGPL,
    },
    Candidate {
        id: "hevc_amf",
        codec: "hevc",
        label: "AMD AMF · HEVC",
        vendor: "amf",
        hardware: true,
        license: LICENSE_LGPL,
    },
    Candidate {
        id: "av1_amf",
        codec: "av1",
        label: "AMD AMF · AV1",
        vendor: "amf",
        hardware: true,
        license: LICENSE_LGPL,
    },
    Candidate {
        id: "h264_qsv",
        codec: "h264",
        label: "Intel Quick Sync · H.264",
        vendor: "qsv",
        hardware: true,
        license: LICENSE_LGPL,
    },
    Candidate {
        id: "hevc_qsv",
        codec: "hevc",
        label: "Intel Quick Sync · HEVC",
        vendor: "qsv",
        hardware: true,
        license: LICENSE_LGPL,
    },
    Candidate {
        id: "av1_qsv",
        codec: "av1",
        label: "Intel Quick Sync · AV1",
        vendor: "qsv",
        hardware: true,
        license: LICENSE_LGPL,
    },
    Candidate {
        id: "h264_vaapi",
        codec: "h264",
        label: "VAAPI · H.264",
        vendor: "vaapi",
        hardware: true,
        license: LICENSE_LGPL,
    },
    Candidate {
        id: "hevc_vaapi",
        codec: "hevc",
        label: "VAAPI · HEVC",
        vendor: "vaapi",
        hardware: true,
        license: LICENSE_LGPL,
    },
    Candidate {
        id: "av1_vaapi",
        codec: "av1",
        label: "VAAPI · AV1",
        vendor: "vaapi",
        hardware: true,
        license: LICENSE_LGPL,
    },
    Candidate {
        id: "h264_d3d12va",
        codec: "h264",
        label: "D3D12 Video · H.264",
        vendor: "d3d12",
        hardware: true,
        license: LICENSE_LGPL,
    },
    Candidate {
        id: "hevc_d3d12va",
        codec: "hevc",
        label: "D3D12 Video · HEVC",
        vendor: "d3d12",
        hardware: true,
        license: LICENSE_LGPL,
    },
    Candidate {
        id: "libopenh264",
        codec: "h264",
        label: "Cisco OpenH264 · 软件",
        vendor: "openh264",
        hardware: false,
        license: LICENSE_LGPL,
    },
    Candidate {
        id: "libvpx-vp9",
        codec: "vp9",
        label: "libvpx · VP9",
        vendor: "vpx",
        hardware: false,
        license: LICENSE_LGPL,
    },
    Candidate {
        id: "libvpx",
        codec: "vp8",
        label: "libvpx · VP8",
        vendor: "vpx",
        hardware: false,
        license: LICENSE_LGPL,
    },
    Candidate {
        id: "libsvtav1",
        codec: "av1",
        label: "SVT-AV1",
        vendor: "svtav1",
        hardware: false,
        license: LICENSE_LGPL,
    },
    Candidate {
        id: "libaom-av1",
        codec: "av1",
        label: "AOMedia AV1",
        vendor: "aom",
        hardware: false,
        license: LICENSE_LGPL,
    },
    Candidate {
        id: "mpeg4",
        codec: "mpeg4",
        label: "MPEG-4 Part 2 · 软件",
        vendor: "native",
        hardware: false,
        license: LICENSE_LGPL,
    },
    Candidate {
        id: "libx264",
        codec: "h264",
        label: "x264 · 软件（GPL）",
        vendor: "x264",
        hardware: false,
        license: LICENSE_GPL,
    },
    Candidate {
        id: "libx265",
        codec: "hevc",
        label: "x265 · 软件（GPL）",
        vendor: "x265",
        hardware: false,
        license: LICENSE_GPL,
    },
];

const AUDIO_CANDIDATES: &[Candidate] = &[
    Candidate {
        id: "aac",
        codec: "aac",
        label: "FFmpeg 内置 AAC",
        vendor: "native",
        hardware: false,
        license: LICENSE_LGPL,
    },
    Candidate {
        id: "libmp3lame",
        codec: "mp3",
        label: "LAME · MP3",
        vendor: "lame",
        hardware: false,
        license: LICENSE_LGPL,
    },
    Candidate {
        id: "libshine",
        codec: "mp3",
        label: "Shine · MP3",
        vendor: "shine",
        hardware: false,
        license: LICENSE_LGPL,
    },
    Candidate {
        id: "libfdk_aac",
        codec: "aac",
        label: "Fraunhofer FDK AAC（nonfree）",
        vendor: "fdk",
        hardware: false,
        license: LICENSE_NONFREE,
    },
    Candidate {
        id: "flac",
        codec: "flac",
        label: "FLAC · 无损",
        vendor: "native",
        hardware: false,
        license: LICENSE_LGPL,
    },
    Candidate {
        id: "pcm_s16le",
        codec: "pcm_s16le",
        label: "PCM 16-bit · 无损",
        vendor: "native",
        hardware: false,
        license: LICENSE_LGPL,
    },
    Candidate {
        id: "libvorbis",
        codec: "vorbis",
        label: "libvorbis · OGG",
        vendor: "vorbis",
        hardware: false,
        license: LICENSE_LGPL,
    },
    Candidate {
        id: "libopus",
        codec: "opus",
        label: "libopus · Opus",
        vendor: "opus",
        hardware: false,
        license: LICENSE_LGPL,
    },
];

fn decode_candidates() -> Vec<(&'static str, &'static str)> {
    if cfg!(target_os = "macos") {
        vec![("videotoolbox", "VideoToolbox 硬解")]
    } else if cfg!(target_os = "windows") {
        vec![
            ("d3d11va", "D3D11VA 硬解"),
            ("dxva2", "DXVA2 硬解"),
            ("cuda", "CUDA / NVDEC 硬解"),
        ]
    } else {
        vec![("vaapi", "VAAPI 硬解"), ("cuda", "CUDA / NVDEC 硬解")]
    }
}

/// 带超时的子进程执行，返回 (成功, 合并的 stderr/stdout 摘要)。
async fn exec_with_timeout(program: &str, args: Vec<String>, timeout: Duration) -> (bool, String) {
    let mut cmd = Command::new(program);
    cmd.args(&args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    #[cfg(windows)]
    {
        // tokio::process::Command 自带 Windows 的 creation_flags，避免弹控制台窗口。
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    let fut = async {
        match cmd.output().await {
            Ok(out) => {
                let mut text = String::from_utf8_lossy(&out.stderr).to_string();
                if text.trim().is_empty() {
                    text = String::from_utf8_lossy(&out.stdout).to_string();
                }
                (out.status.success(), text)
            }
            Err(err) => (false, format!("无法启动 ffmpeg: {err}")),
        }
    };
    match tokio::time::timeout(timeout, fut).await {
        Ok(result) => result,
        Err(_) => (false, "探测超时".into()),
    }
}

fn compact_error(text: &str) -> String {
    let mut lines: Vec<String> = text
        .lines()
        .map(|l| l.trim().to_string())
        .filter(|l| !l.is_empty())
        .collect();
    if lines.is_empty() {
        return "未知原因".into();
    }
    let keep = lines.len().saturating_sub(2);
    let picked = lines.split_off(keep);
    let joined = picked.join(" · ");
    if joined.chars().count() > 220 {
        joined.chars().take(220).collect::<String>() + "…"
    } else {
        joined
    }
}

fn parse_encoder_list(text: &str) -> HashSet<String> {
    let mut set = HashSet::new();
    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || !trimmed.contains("(codec ") {
            continue;
        }
        let parts: Vec<&str> = trimmed.split_whitespace().collect();
        if parts.len() >= 2 && parts[0].len() >= 5 {
            set.insert(parts[1].to_string());
        }
    }
    set
}

fn parse_hwaccels(text: &str) -> HashSet<String> {
    let mut set = HashSet::new();
    for line in text.lines() {
        let t = line.trim();
        if t.is_empty() || t.contains(' ') || t.starts_with("Hardware") {
            continue;
        }
        set.insert(t.to_string());
    }
    set
}

fn fingerprint_of(info: &ToolsInfo) -> String {
    format!(
        "{}|{}|{}|{}|{}",
        info.ffmpeg_version.clone().unwrap_or_default(),
        info.build.configuration.len(),
        if info.build.gpl { "gpl" } else { "lgpl" },
        info.platform,
        info.arch
    )
}

fn cache_path(app: &tauri::AppHandle) -> Option<PathBuf> {
    app.path()
        .app_data_dir()
        .ok()
        .map(|dir| dir.join("capabilities.json"))
}

#[derive(Serialize, Deserialize)]
struct CacheFile {
    fingerprint: String,
    probed_at: i64,
    capabilities: Capabilities,
}

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

const CACHE_TTL_MS: i64 = 7 * 24 * 60 * 60 * 1000;

fn read_cache(app: &tauri::AppHandle, fingerprint: &str) -> Option<Capabilities> {
    let path = cache_path(app)?;
    let raw = std::fs::read_to_string(path).ok()?;
    let parsed: CacheFile = serde_json::from_str(&raw).ok()?;
    if parsed.fingerprint != fingerprint {
        return None;
    }
    if now_ms() - parsed.probed_at > CACHE_TTL_MS {
        return None;
    }
    let mut caps = parsed.capabilities;
    caps.cache_hit = true;
    Some(caps)
}

fn write_cache(app: &tauri::AppHandle, caps: &Capabilities) {
    if let Some(path) = cache_path(app) {
        if let Some(dir) = path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        let payload = CacheFile {
            fingerprint: caps.fingerprint.clone(),
            probed_at: caps.probed_at,
            capabilities: caps.clone(),
        };
        if let Ok(text) = serde_json::to_string_pretty(&payload) {
            let _ = std::fs::write(path, text);
        }
    }
}

/// 试编一小段：这是"实际可用"与"仅编译进来"的分界线。
async fn trial_video(ffmpeg: &str, id: &str) -> (bool, String) {
    let mut args: Vec<String> = vec![
        "-hide_banner".into(),
        "-loglevel".into(),
        "error".into(),
        "-nostdin".into(),
        "-y".into(),
    ];
    // VAAPI 必须显式建设备并上传帧，否则试编必然失败，会造成「Linux 有 GPU 却探测为不可用」的误判。
    let vaapi_device = "/dev/dri/renderD128";
    let vaapi_ok = id.ends_with("_vaapi") && std::path::Path::new(vaapi_device).exists();
    if vaapi_ok {
        args.push("-vaapi_device".into());
        args.push(vaapi_device.into());
    }
    args.extend([
        "-f".into(),
        "lavfi".into(),
        "-i".into(),
        "testsrc2=size=320x240:rate=10:duration=0.6".into(),
    ]);
    if vaapi_ok {
        args.push("-vf".into());
        args.push("format=nv12,hwupload".into());
    }
    args.extend([
        "-c:v".into(),
        id.to_string(),
        "-f".into(),
        "null".into(),
        "-".into(),
    ]);
    exec_with_timeout(ffmpeg, args, Duration::from_secs(25)).await
}

async fn trial_audio(ffmpeg: &str, id: &str) -> (bool, String) {
    let args = vec![
        "-hide_banner".into(),
        "-loglevel".into(),
        "error".into(),
        "-nostdin".into(),
        "-y".into(),
        "-f".into(),
        "lavfi".into(),
        "-i".into(),
        "sine=frequency=440:duration=0.6".into(),
        "-c:a".into(),
        id.to_string(),
        "-f".into(),
        "null".into(),
        "-".into(),
    ];
    exec_with_timeout(ffmpeg, args, Duration::from_secs(20)).await
}

async fn make_decode_sample(ffmpeg: &str, encoder: &str, dir: &PathBuf) -> Option<PathBuf> {
    let _ = std::fs::create_dir_all(dir);
    let target = dir.join("__ff_decode_probe.mp4");
    let args = vec![
        "-hide_banner".into(),
        "-loglevel".into(),
        "error".into(),
        "-nostdin".into(),
        "-y".into(),
        "-f".into(),
        "lavfi".into(),
        "-i".into(),
        "testsrc2=size=1280x720:rate=25:duration=1.5".into(),
        "-c:v".into(),
        encoder.to_string(),
        "-pix_fmt".into(),
        "yuv420p".into(),
        "-movflags".into(),
        "+faststart".into(),
        target.to_string_lossy().to_string(),
    ];
    let (ok, _) = exec_with_timeout(ffmpeg, args, Duration::from_secs(30)).await;
    if ok && target.is_file() {
        Some(target)
    } else {
        None
    }
}

async fn trial_decode(ffmpeg: &str, accel: &str, sample: &PathBuf) -> (bool, String) {
    let mut args = vec![
        "-hide_banner".into(),
        "-loglevel".into(),
        "error".into(),
        "-nostdin".into(),
    ];
    args.push("-hwaccel".into());
    args.push(accel.to_string());
    if accel == "vaapi" || accel == "qsv" {
        let dev = "/dev/dri/renderD128";
        if std::path::Path::new(dev).exists() {
            args.push("-hwaccel_device".into());
            args.push(dev.into());
        }
    }
    args.push("-i".into());
    args.push(sample.to_string_lossy().to_string());
    args.push("-f".into());
    args.push("null".into());
    args.push("-".into());
    exec_with_timeout(ffmpeg, args, Duration::from_secs(25)).await
}

pub async fn detect(
    app: &tauri::AppHandle,
    info: &ToolsInfo,
    allow_gpl: bool,
    force: bool,
) -> Result<Capabilities, String> {
    let ffmpeg = info
        .ffmpeg
        .as_ref()
        .map(|t| t.path.clone())
        .ok_or_else(|| "未找到 ffmpeg，无法探测编码能力".to_string())?;
    let fingerprint = fingerprint_of(info);
    if !force {
        if let Some(cached) = read_cache(app, &fingerprint) {
            return Ok(cached);
        }
    }

    // 1) 编译进构建的编码器清单
    let (encoders_ok, encoders_text) = exec_with_timeout(
        &ffmpeg,
        vec!["-hide_banner".into(), "-encoders".into()],
        Duration::from_secs(20),
    )
    .await;
    if !encoders_ok {
        return Err(format!("读取 ffmpeg -encoders 失败：{encoders_text}"));
    }
    let compiled = parse_encoder_list(&encoders_text);

    // 2) 硬件解码方法（仅构建支持，真正的可用性下面用真实样本验证）
    let (hw_ok, hw_text) = exec_with_timeout(
        &ffmpeg,
        vec!["-hide_banner".into(), "-hwaccels".into()],
        Duration::from_secs(15),
    )
    .await;
    let hw_listed = if hw_ok {
        parse_hwaccels(&hw_text)
    } else {
        HashSet::new()
    };

    let mut notes: Vec<String> = Vec::new();
    let mut video_encoders = Vec::new();
    let mut audio_encoders = Vec::new();

    for cand in VIDEO_CANDIDATES {
        let compiled_flag = compiled.contains(cand.id);
        let allowed = match cand.license {
            LICENSE_GPL | LICENSE_NONFREE => allow_gpl,
            _ => true,
        };
        let mut reason: Option<String> = None;
        let mut available = false;
        if !compiled_flag {
            reason = Some("当前 FFmpeg 构建未包含该编码器".into());
        } else if !allowed {
            reason = Some(format!(
                "许可策略已禁用（{}）：默认安装包不包含该组件",
                if cand.license == LICENSE_GPL {
                    "GPL"
                } else {
                    "nonfree"
                }
            ));
        } else {
            let (ok, err) = trial_video(&ffmpeg, cand.id).await;
            available = ok;
            if !ok {
                reason = Some(format!("试编失败：{}", compact_error(&err)));
            }
        }
        video_encoders.push(EncoderCap {
            id: cand.id.into(),
            codec: cand.codec.into(),
            label: cand.label.into(),
            vendor: cand.vendor.into(),
            hardware: cand.hardware,
            license: cand.license.into(),
            allowed,
            compiled: compiled_flag,
            available,
            reason,
        });
    }

    for cand in AUDIO_CANDIDATES {
        let compiled_flag = compiled.contains(cand.id);
        let allowed = match cand.license {
            LICENSE_GPL | LICENSE_NONFREE => allow_gpl,
            _ => true,
        };
        let mut reason: Option<String> = None;
        let mut available = false;
        if !compiled_flag {
            reason = Some("当前 FFmpeg 构建未包含该编码器".into());
        } else if !allowed {
            reason = Some("许可策略已禁用（nonfree）".into());
        } else {
            let (ok, err) = trial_audio(&ffmpeg, cand.id).await;
            available = ok;
            if !ok {
                reason = Some(format!("试编失败：{}", compact_error(&err)));
            }
        }
        audio_encoders.push(EncoderCap {
            id: cand.id.into(),
            codec: cand.codec.into(),
            label: cand.label.into(),
            vendor: cand.vendor.into(),
            hardware: cand.hardware,
            license: cand.license.into(),
            allowed,
            compiled: compiled_flag,
            available,
            reason,
        });
    }

    // 3) 解码加速：先用当前可用的视频编码器生成一个真实样本，再逐项 -hwaccel 解码验证
    let decode_enc = video_encoders
        .iter()
        .find(|e| e.available && e.hardware && e.codec == "h264")
        .or_else(|| {
            video_encoders
                .iter()
                .find(|e| e.available && e.codec == "h264")
        })
        .or_else(|| video_encoders.iter().find(|e| e.available))
        .map(|e| e.id.clone());
    let mut decode_accels = Vec::new();
    let sample_dir = app
        .path()
        .app_data_dir()
        .map(|d| d.join("probe"))
        .unwrap_or_else(|_| std::env::temp_dir());
    match decode_enc {
        Some(enc) => {
            let sample = make_decode_sample(&ffmpeg, &enc, &sample_dir).await;
            for (id, label) in decode_candidates() {
                let compiled_flag = hw_listed.contains(id);
                if !compiled_flag {
                    decode_accels.push(DecodeCap {
                        id: id.to_string(),
                        label: label.to_string(),
                        available: false,
                        reason: Some("当前 FFmpeg 构建未包含该硬件解码方法".into()),
                    });
                    continue;
                }
                match &sample {
                    Some(path) => {
                        let (ok, err) = trial_decode(&ffmpeg, id, path).await;
                        decode_accels.push(DecodeCap {
                            id: id.to_string(),
                            label: label.to_string(),
                            available: ok,
                            reason: if ok {
                                None
                            } else {
                                Some(format!("试解失败：{}", compact_error(&err)))
                            },
                        });
                    }
                    None => decode_accels.push(DecodeCap {
                        id: id.to_string(),
                        label: label.to_string(),
                        available: false,
                        reason: Some("缺少可用于解码验证的样本（未能生成测试视频）".into()),
                    }),
                }
            }
            if let Some(path) = sample {
                let _ = std::fs::remove_file(path);
            }
        }
        None => {
            notes.push("没有任何可用的视频编码器，硬件解码未验证".into());
            for (id, label) in decode_candidates() {
                decode_accels.push(DecodeCap {
                    id: id.to_string(),
                    label: label.to_string(),
                    available: false,
                    reason: Some("无可用视频编码器，无法生成解码样本".into()),
                });
            }
        }
    }

    let software_h264_available = video_encoders
        .iter()
        .any(|e| e.codec == "h264" && !e.hardware && e.available && e.allowed);

    if info.build.gpl || info.build.nonfree {
        notes.push("当前二进制为 GPL/nonfree 构建：编码能力表仅代表本机开发环境，不代表可分发包的许可状态。".into());
    }
    if !software_h264_available {
        notes.push(
            "默认（LGPL）路线下没有可用的软件 H.264 编码器：需要 LGPL 构建并包含 libopenh264，或使用硬件编码。"
                .into(),
        );
    }

    let caps = Capabilities {
        probed_at: now_ms(),
        cache_hit: false,
        fingerprint,
        platform: info.platform.clone(),
        arch: info.arch.clone(),
        ffmpeg_version: info.ffmpeg_version.clone().unwrap_or_default(),
        build_gpl: info.build.gpl,
        build_nonfree: info.build.nonfree,
        video_encoders,
        audio_encoders,
        decode_accels,
        software_h264_available,
        notes,
    };
    write_cache(app, &caps);
    Ok(caps)
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct MediaInfo {
    pub path: String,
    pub name: String,
    pub ext: String,
    pub size_bytes: u64,
    /// video | audio | image | other
    pub kind: String,
    pub duration_sec: Option<f64>,
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub video_codec: Option<String>,
    pub audio_codec: Option<String>,
    pub pix_fmt: Option<String>,
    pub bitrate: Option<u64>,
    pub fps: Option<f64>,
    pub sample_rate: Option<u32>,
    pub channels: Option<u32>,
    pub error: Option<String>,
}

fn ext_of(path: &str) -> String {
    std::path::Path::new(path)
        .extension()
        .map(|e| e.to_string_lossy().to_lowercase())
        .unwrap_or_default()
}

fn is_image_ext(ext: &str) -> bool {
    matches!(
        ext,
        "jpg" | "jpeg" | "png" | "webp" | "bmp" | "tif" | "tiff" | "gif" | "avif" | "heic"
    )
}

pub async fn probe_media(ffprobe: &str, path: &str) -> MediaInfo {
    let name = std::path::Path::new(path)
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| path.to_string());
    let size_bytes = std::fs::metadata(path).map(|m| m.len()).unwrap_or(0);
    let ext = ext_of(path);
    let mut info = MediaInfo {
        path: path.to_string(),
        name,
        ext: ext.clone(),
        size_bytes,
        kind: if is_image_ext(&ext) {
            "image".into()
        } else {
            "other".into()
        },
        ..Default::default()
    };

    if size_bytes == 0 {
        info.error = Some("文件为 0 字节或不可读".into());
        return info;
    }

    let args = vec![
        "-hide_banner".into(),
        "-v".into(),
        "error".into(),
        "-print_format".into(),
        "json".into(),
        "-show_format".into(),
        "-show_streams".into(),
        path.to_string(),
    ];
    let (ok, text) = exec_with_timeout(ffprobe, args, Duration::from_secs(25)).await;
    if !ok {
        info.error = Some(format!("ffprobe 无法解析：{}", compact_error(&text)));
        return info;
    }
    let json: serde_json::Value = match serde_json::from_str(&text) {
        Ok(v) => v,
        Err(e) => {
            info.error = Some(format!("ffprobe 输出解析失败：{e}"));
            return info;
        }
    };

    let format_name = json
        .get("format")
        .and_then(|f| f.get("format_name"))
        .and_then(|v| v.as_str())
        .unwrap_or("");
    info.bitrate = json
        .get("format")
        .and_then(|f| f.get("bit_rate"))
        .and_then(|v| v.as_str())
        .and_then(|s| s.parse::<u64>().ok());

    let streams = json
        .get("streams")
        .and_then(|s| s.as_array())
        .cloned()
        .unwrap_or_default();
    let mut has_video = false;
    let mut has_audio = false;
    for stream in streams {
        let codec_type = stream
            .get("codec_type")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        match codec_type {
            "video" => {
                if stream
                    .get("disposition")
                    .and_then(|d| d.get("attached_pic"))
                    .and_then(|v| v.as_i64())
                    == Some(1)
                {
                    continue;
                }
                has_video = true;
                if info.video_codec.is_none() {
                    info.video_codec = stream
                        .get("codec_name")
                        .and_then(|v| v.as_str())
                        .map(str::to_string);
                    info.width = stream
                        .get("width")
                        .and_then(|v| v.as_i64())
                        .map(|v| v as u32);
                    info.height = stream
                        .get("height")
                        .and_then(|v| v.as_i64())
                        .map(|v| v as u32);
                    info.pix_fmt = stream
                        .get("pix_fmt")
                        .and_then(|v| v.as_str())
                        .map(str::to_string);
                    info.fps = stream
                        .get("avg_frame_rate")
                        .and_then(|v| v.as_str())
                        .and_then(parse_rate)
                        .or_else(|| {
                            stream
                                .get("r_frame_rate")
                                .and_then(|v| v.as_str())
                                .and_then(parse_rate)
                        });
                    if info.duration_sec.is_none() {
                        info.duration_sec = stream
                            .get("duration")
                            .and_then(|v| v.as_str())
                            .and_then(|s| s.parse::<f64>().ok());
                    }
                }
            }
            "audio" => {
                has_audio = true;
                if info.audio_codec.is_none() {
                    info.audio_codec = stream
                        .get("codec_name")
                        .and_then(|v| v.as_str())
                        .map(str::to_string);
                    info.sample_rate = stream
                        .get("sample_rate")
                        .and_then(|v| v.as_str())
                        .and_then(|s| s.parse::<u32>().ok());
                    info.channels = stream
                        .get("channels")
                        .and_then(|v| v.as_i64())
                        .map(|v| v as u32);
                }
            }
            _ => {}
        }
    }

    info.duration_sec = info.duration_sec.or_else(|| {
        json.get("format")
            .and_then(|f| f.get("duration"))
            .and_then(|v| v.as_str())
            .and_then(|s| s.parse::<f64>().ok())
    });

    info.kind = if has_video {
        if is_image_ext(&ext) || format_name.contains("_pipe") || format_name.contains("image2") {
            "image".into()
        } else {
            "video".into()
        }
    } else if has_audio {
        "audio".into()
    } else {
        "other".into()
    };
    if info.kind == "image" {
        info.duration_sec = None;
    }
    if !has_video && !has_audio {
        info.error
            .get_or_insert_with(|| "没有可用的音视频流".into());
    }
    info
}

fn parse_rate(text: &str) -> Option<f64> {
    if let Some((num, den)) = text.split_once('/') {
        let n: f64 = num.parse().ok()?;
        let d: f64 = den.parse().ok()?;
        if d == 0.0 {
            return None;
        }
        let value = n / d;
        if value.is_finite() && value > 0.0 {
            Some(value)
        } else {
            None
        }
    } else {
        text.parse::<f64>().ok()
    }
}
