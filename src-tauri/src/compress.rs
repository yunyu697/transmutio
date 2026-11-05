//! 图片压缩：质量量纲映射、目标字节二分驱动、编码参数拼装与真实执行。
//!
//! 尺寸永不改动：所有滤镜（裁剪 → 旋转 → 缩放 → 水印）都沿用
//! [`crate::args::build_image`] 现成的链，压缩只替换编码参数并反复重编码。

use crate::args::{self, CompressParams, ImageParams};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

/// JPEG 的质量下限：再低只会更糊，收益有限。
pub const JPEG_FLOOR: u32 = 40;
/// WebP 的质量下限（libwebp 的 `-quality`）。
pub const WEBP_FLOOR: u32 = 35;
/// 单次压缩最多尝试次数（含首轮），保证绝不死循环。
pub const MAX_ATTEMPTS: u32 = 8;
/// 收敛判据：|bytes - target| / target ≤ 5%。
pub const TOLERANCE: f64 = 0.05;

/// 任务输出格式归一化：`jpeg` 等别名统一成 jpg / png / webp / bmp / tiff。
pub fn format_key(format: &str) -> &'static str {
    match format.trim().to_ascii_lowercase().as_str() {
        "png" => "png",
        "webp" => "webp",
        "bmp" => "bmp",
        "tiff" | "tif" => "tiff",
        _ => "jpg",
    }
}

/// 1–100 的人类质量 → mjpeg 的 `-q:v` 2–31。质量越高 q 越小（单调不增）。
pub fn jpeg_qscale(quality: u32) -> u32 {
    let quality = quality.clamp(1, 100);
    let q = 31.0 - (quality as f64 - 1.0) / 99.0 * 29.0;
    (q.round() as u32).clamp(2, 31)
}

/// PNG 没有质量档位，把 1–100 线性映射到 zlib 的 `-compression_level` 0–9。
pub fn png_compression_level(quality: u32) -> u32 {
    let quality = quality.clamp(1, 100);
    ((quality as f64 - 1.0) / 99.0 * 9.0).round() as u32
}

/// 各格式允许的最低压缩质量；PNG/BMP/TIFF 不参与二分。
pub fn floor_quality(format: &str) -> Option<u32> {
    match format_key(format) {
        "jpg" => Some(JPEG_FLOOR),
        "webp" => Some(WEBP_FLOOR),
        _ => None,
    }
}

fn mode_of(c: &CompressParams) -> &str {
    if c.mode.trim().is_empty() {
        "target"
    } else {
        c.mode.as_str()
    }
}

fn target_mode_of(c: &CompressParams) -> &str {
    if c.target_mode.trim().is_empty() {
        "absolute"
    } else {
        c.target_mode.as_str()
    }
}

/// target 模式的目标字节数；quality 模式返回 None（没有「目标」可比）。
pub fn target_bytes(original_bytes: u64, c: &CompressParams) -> Option<u64> {
    if mode_of(c) != "target" {
        return None;
    }
    match target_mode_of(c) {
        "percent" => {
            let bytes = original_bytes as f64 * c.target_percent / 100.0;
            Some((bytes.round() as u64).max(1))
        }
        _ => Some((c.target_kb as u64).saturating_mul(1024).max(1)),
    }
}

pub fn within_tolerance(bytes: u64, target: u64) -> bool {
    if target == 0 {
        return false;
    }
    let diff = bytes.abs_diff(target) as f64;
    diff / target as f64 <= TOLERANCE
}

#[derive(Debug, Clone, PartialEq)]
pub struct SearchOutcome {
    pub quality: u32,
    pub bytes: u64,
    pub iterations: u32,
    pub met: bool,
    /// 每轮尝试的 (质量, 字节)。
    pub log: Vec<(u32, u64)>,
}

/// 在 `[floor, 100]` 内二分找「字节 ≤ 目标」的最高质量。
/// 最多 [`MAX_ATTEMPTS`] 次；若 floor 仍超目标则停止压，`met=false`、quality=floor。
pub fn search_target<F>(size_for_q: F, target: u64, floor: u32) -> Result<SearchOutcome, String>
where
    F: FnMut(u32) -> Result<u64, String>,
{
    search_range(size_for_q, target, floor, 100)
}

/// 与 [`search_target`] 相同，但可显式指定上下限（测试迭代上限用）。
pub fn search_range<F>(
    mut size_for_q: F,
    target: u64,
    floor: u32,
    ceiling: u32,
) -> Result<SearchOutcome, String>
where
    F: FnMut(u32) -> Result<u64, String>,
{
    let mut lo = floor;
    let mut hi = ceiling.max(floor);
    let mut best: Option<(u32, u64)> = None;
    let mut last: Option<(u32, u64)> = None;
    let mut log: Vec<(u32, u64)> = Vec::new();
    let mut iterations = 0u32;

    while iterations < MAX_ATTEMPTS && lo <= hi {
        let mid = lo + (hi - lo) / 2;
        let bytes = size_for_q(mid)?;
        iterations += 1;
        log.push((mid, bytes));
        last = Some((mid, bytes));

        if bytes <= target {
            // 可行解：先记下，再尝试更高质量；落在 5% 内直接收敛。
            best = Some((mid, bytes));
            if within_tolerance(bytes, target) {
                break;
            }
            lo = mid + 1;
        } else if mid == lo {
            // 已经到下限仍超目标：不再继续压，保留下限产物。
            break;
        } else {
            hi = mid - 1;
        }
    }

    let chosen = best
        .or(last)
        .ok_or_else(|| "未能产生任何压缩尝试".to_string())?;
    Ok(SearchOutcome {
        quality: chosen.0,
        bytes: chosen.1,
        iterations,
        met: chosen.1 <= target,
        log,
    })
}

fn kb_text(bytes: u64) -> String {
    format!("{:.1}", bytes as f64 / 1024.0)
}

/// 压缩结果的人类可读文案。未达标时明确说明「质量下限/收益有限」，前端据此标未达标。
pub fn report_message(format: &str, outcome: &SearchOutcome) -> String {
    let kb = kb_text(outcome.bytes);
    if outcome.met {
        return format!(
            "已压缩到 {} KB（质量 {}，{} 次尝试）；压缩在裁剪/旋转/缩放/水印之后",
            kb, outcome.quality, outcome.iterations
        );
    }
    if format_key(format) == "png" {
        format!(
            "PNG 重编码收益有限，已压到最高压缩仍未达到目标大小，输出为 {} KB；压缩在裁剪/旋转/缩放/水印之后",
            kb
        )
    } else {
        format!(
            "已压到质量 {} 仍未达到目标大小，输出为 {} KB；压缩在裁剪/旋转/缩放/水印之后",
            outcome.quality, kb
        )
    }
}

/// 固定质量模式的文案（没有目标大小可比）。
pub fn quality_message(bytes: u64, quality: u32, lossless: bool) -> String {
    format!(
        "已按{}{}质量 {} 重新编码，输出为 {} KB；压缩在裁剪/旋转/缩放/水印之后",
        if lossless { "无损" } else { "" },
        if lossless { "模式" } else { "" },
        quality,
        kb_text(bytes)
    )
}

fn set_flag(argv: &mut Vec<String>, flag: &str, value: &str) {
    if let Some(pos) = argv.iter().position(|a| a == flag) {
        if pos + 1 < argv.len() {
            argv[pos + 1] = value.to_string();
            return;
        }
    }
    insert_before_frames(argv, &[flag.to_string(), value.to_string()]);
}

fn insert_before_frames(argv: &mut Vec<String>, tokens: &[String]) {
    let pos = argv
        .iter()
        .rposition(|a| a == "-frames:v")
        .unwrap_or(argv.len().saturating_sub(1));
    for (index, token) in tokens.iter().enumerate() {
        argv.insert(pos + index, token.clone());
    }
}

#[cfg(test)]
fn flag_value(argv: &[String], flag: &str) -> Option<String> {
    let pos = argv.iter().position(|a| a == flag)?;
    argv.get(pos + 1).cloned()
}

/// 构造单次压缩的 ffmpeg 参数：完整复用 build_image 的滤镜链，
/// 只把编码参数换成目标质量，并补上元数据开关。
pub fn compression_args(
    input: &str,
    output: &str,
    p: &ImageParams,
    quality: u32,
    png_palette: bool,
) -> Result<Vec<String>, String> {
    let format = format_key(&p.format);
    if !matches!(format, "jpg" | "webp" | "png") {
        return Err(format!("{format} 压缩不适用"));
    }
    let mut image = p.clone();
    image.quality = quality.clamp(1, 100);
    let mut argv = args::build_image(input, output, &image)?;

    match format {
        "jpg" => {
            // mjpeg 的 qscale 是反向量纲，必须用统一映射覆盖 build_image 的默认值。
            set_flag(&mut argv, "-q:v", &jpeg_qscale(quality).to_string());
            set_flag(&mut argv, "-pix_fmt", "yuvj420p");
        }
        "webp" => {
            // libwebp 的 -quality 直接用 1..100，不做反向映射。
            set_flag(&mut argv, "-quality", &quality.clamp(1, 100).to_string());
            if image.compress.lossless {
                set_flag(&mut argv, "-lossless", "1");
            }
        }
        "png" => {
            set_flag(
                &mut argv,
                "-compression_level",
                &png_compression_level(quality).to_string(),
            );
            if png_palette {
                set_flag(&mut argv, "-pix_fmt", "pal8");
            }
        }
        _ => {}
    }

    let metadata = if image.compress.keep_metadata {
        "0"
    } else {
        "-1"
    };
    insert_before_frames(
        &mut argv,
        &["-map_metadata".to_string(), metadata.to_string()],
    );
    Ok(argv)
}

fn run_ffmpeg(program: &str, argv: &[String]) -> Result<(), String> {
    let mut cmd = Command::new(program);
    cmd.args(argv)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    #[cfg(windows)]
    {
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    let output = cmd.output().map_err(|e| format!("无法启动 ffmpeg：{e}"))?;
    if !output.status.success() {
        let tail = String::from_utf8_lossy(&output.stderr)
            .lines()
            .rev()
            .take(6)
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect::<Vec<_>>()
            .join("；");
        return Err(format!(
            "压缩失败（退出码 {:?}）：{tail}",
            output.status.code()
        ));
    }
    Ok(())
}

#[derive(Debug, Clone)]
pub struct CompressReport {
    pub quality: u32,
    pub bytes: u64,
    pub iterations: u32,
    pub target_bytes: Option<u64>,
    pub target_met: Option<bool>,
    pub log: Vec<(u32, u64)>,
    pub message: String,
}

pub struct CompressRequest<'a> {
    pub ffmpeg: &'a str,
    pub input: &'a str,
    /// 最终临时输出路径（同目录，便于原子改名）。
    pub output: &'a str,
    pub work_dir: &'a Path,
    pub job_id: &'a str,
    pub params: &'a ImageParams,
    pub original_bytes: u64,
}

fn attempt_path(work_dir: &Path, job_id: &str, quality: u32, ext: &str) -> PathBuf {
    work_dir.join(format!(".ff-tmp-{job_id}-c{quality}.{ext}"))
}

/// 执行压缩：target 模式二分（PNG 单次最大压缩），quality 模式单次重编码。
/// 每轮写独立临时文件并用 `std::fs::metadata` 量字节，最后只保留选中那次。
pub fn compress_image(req: &CompressRequest) -> Result<CompressReport, String> {
    let format = format_key(&req.params.format);
    if !matches!(format, "jpg" | "webp" | "png") {
        return Err(format!("{format} 压缩不适用：请改用 JPEG / WebP / PNG"));
    }
    let c = &req.params.compress;
    let target = target_bytes(req.original_bytes, c);
    let target_mode = target.is_some();
    let quality = c.quality.clamp(1, 100);
    let png_palette = c.png_palette && format == "png";

    let run_one = |q: u32| -> Result<u64, String> {
        let path = attempt_path(req.work_dir, req.job_id, q, format);
        let _ = std::fs::remove_file(&path);
        let argv = compression_args(
            req.input,
            path.to_string_lossy().as_ref(),
            req.params,
            q,
            png_palette,
        )?;
        run_ffmpeg(req.ffmpeg, &argv)?;
        let bytes = std::fs::metadata(&path)
            .map_err(|e| format!("读取压缩结果大小失败：{e}"))?
            .len();
        if bytes == 0 {
            return Err("编码器输出了空文件".into());
        }
        Ok(bytes)
    };

    let outcome = match (target, format) {
        (Some(target), "jpg" | "webp") => {
            let floor = floor_quality(format).unwrap_or(JPEG_FLOOR);
            search_target(run_one, target, floor)?
        }
        (Some(target), "png") => {
            // PNG 不做二分：只跑一次最高压缩（调色板量化会降色深，属已知代价）。
            let bytes = run_one(100)?;
            SearchOutcome {
                quality: 100,
                bytes,
                iterations: 1,
                met: bytes <= target,
                log: vec![(100, bytes)],
            }
        }
        _ => {
            let bytes = run_one(quality)?;
            SearchOutcome {
                quality,
                bytes,
                iterations: 1,
                met: bytes <= target.unwrap_or(u64::MAX),
                log: vec![(quality, bytes)],
            }
        }
    };

    // 只保留选中质量的那次产物，其余尝试文件删除。
    let chosen = attempt_path(req.work_dir, req.job_id, outcome.quality, format);
    for (q, _) in &outcome.log {
        if *q != outcome.quality {
            let _ = std::fs::remove_file(attempt_path(req.work_dir, req.job_id, *q, format));
        }
    }
    let _ = std::fs::remove_file(req.output);
    std::fs::rename(&chosen, req.output).map_err(|e| {
        format!(
            "写出压缩结果失败（{} → {}）：{e}",
            chosen.display(),
            req.output
        )
    })?;

    let message = if target_mode {
        report_message(format, &outcome)
    } else {
        quality_message(outcome.bytes, outcome.quality, c.lossless)
    };
    Ok(CompressReport {
        quality: outcome.quality,
        bytes: outcome.bytes,
        iterations: outcome.iterations,
        target_bytes: target,
        target_met: if target_mode { Some(outcome.met) } else { None },
        log: outcome.log,
        message,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn image(format: &str, compress: CompressParams) -> ImageParams {
        ImageParams {
            format: format.into(),
            compress,
            ..Default::default()
        }
    }

    #[test]
    fn jpeg_qscale_boundaries_and_monotonic() {
        assert_eq!(jpeg_qscale(100), 2);
        assert_eq!(jpeg_qscale(1), 31);
        let mut previous = 31;
        for quality in 1..=100 {
            let q = jpeg_qscale(quality);
            assert!((2..=31).contains(&q), "质量 {quality} → qscale {q} 越界");
            assert!(
                q <= previous,
                "质量升高时 qscale 不应增大：{q} > {previous}"
            );
            previous = q;
        }
        assert!(
            jpeg_qscale(90) < jpeg_qscale(10),
            "质量越高 qscale 必须越小"
        );
    }

    #[test]
    fn search_chooses_highest_feasible_quality() {
        // 阶跃序列：q ≤ 60 为 800 字节（≤ 目标 1000），q > 60 为 1200 字节。
        let outcome =
            search_target(|q| Ok(if q <= 60 { 800 } else { 1200 }), 1000, JPEG_FLOOR).unwrap();
        assert_eq!(outcome.quality, 60, "应选可行范围内的最高质量");
        assert_eq!(outcome.bytes, 800);
        assert!(outcome.met);
        assert!(outcome.iterations <= MAX_ATTEMPTS);
        assert_eq!(outcome.log.len() as u32, outcome.iterations);
        assert!(outcome.log.iter().any(|(q, b)| *q == 60 && *b == 800));
    }

    #[test]
    fn convergence_stops_inside_five_percent_and_continues_outside() {
        // 首次探测 70 → 960，|960-1000|/1000 = 4%：立即收敛。
        let inside = search_target(|_| Ok(960), 1000, JPEG_FLOOR).unwrap();
        assert_eq!(inside.iterations, 1);
        assert!(inside.met);

        // 首次 1200（超出 20%）继续；第二次 1000 命中目标后收敛。
        let mut calls = 0u32;
        let mut sizes = vec![1200u64, 1000u64].into_iter();
        let outside = search_target(
            |_| {
                calls += 1;
                Ok(sizes.next().unwrap_or(1000))
            },
            1000,
            JPEG_FLOOR,
        )
        .unwrap();
        assert_eq!(calls, 2, "超出容差必须继续尝试");
        assert_eq!(outside.iterations, 2);
        assert_eq!(outside.quality, 54, "70 超目标后应探测下半区中点");
        assert!(outside.met);
        assert!(within_tolerance(1050, 1000), "5% 边界应判定为收敛");
        assert!(!within_tolerance(1051, 1000), "超出 5% 不应收敛");
    }

    #[test]
    fn iteration_cap_returns_after_eight_attempts() {
        let mut calls = 0u32;
        let result = search_range(
            |_| {
                calls += 1;
                Ok(100_000)
            },
            4096,
            0,
            255,
        )
        .unwrap();
        assert_eq!(result.iterations, MAX_ATTEMPTS, "必须恰好耗尽 8 次后返回");
        assert_eq!(calls, MAX_ATTEMPTS);
        assert_eq!(result.log.len() as u32, MAX_ATTEMPTS);
        assert!(!result.met);
        assert_eq!(result.bytes, 100_000);
    }

    #[test]
    fn floor_protection_keeps_lowest_attempt_and_message() {
        let outcome = search_target(|_| Ok(20_000), 4096, JPEG_FLOOR).unwrap();
        assert_eq!(outcome.quality, JPEG_FLOOR);
        assert_eq!(outcome.bytes, 20_000, "下限产物必须被保留");
        assert!(!outcome.met);
        assert!(outcome.iterations <= MAX_ATTEMPTS);
        let message = report_message("jpg", &outcome);
        assert!(message.contains("仍未达到目标大小"), "{message}");
        assert!(message.contains("质量 40"), "{message}");
    }

    #[test]
    fn webp_quality_passthrough_and_jpeg_qscale_reverse() {
        let webp = compression_args(
            "in.jpg",
            "out.webp",
            &image(
                "webp",
                CompressParams {
                    mode: "quality".into(),
                    quality: 80,
                    ..Default::default()
                },
            ),
            80,
            false,
        )
        .unwrap();
        assert_eq!(flag_value(&webp, "-quality").as_deref(), Some("80"));
        assert_eq!(flag_value(&webp, "-q:v"), None);

        let jpg = compression_args(
            "in.jpg",
            "out.jpg",
            &image(
                "jpg",
                CompressParams {
                    mode: "quality".into(),
                    quality: 80,
                    ..Default::default()
                },
            ),
            80,
            false,
        )
        .unwrap();
        assert_eq!(
            flag_value(&jpg, "-q:v").as_deref(),
            Some(jpeg_qscale(80).to_string().as_str())
        );
        assert_eq!(flag_value(&jpg, "-pix_fmt").as_deref(), Some("yuvj420p"));
    }

    #[test]
    fn metadata_flag_follows_keep_metadata() {
        let stripped = compression_args(
            "in.jpg",
            "out.jpg",
            &image("jpg", CompressParams::default()),
            80,
            false,
        )
        .unwrap();
        assert_eq!(
            flag_value(&stripped, "-map_metadata").as_deref(),
            Some("-1")
        );

        let kept = compression_args(
            "in.jpg",
            "out.jpg",
            &image(
                "jpg",
                CompressParams {
                    keep_metadata: true,
                    ..Default::default()
                },
            ),
            80,
            false,
        )
        .unwrap();
        assert_eq!(flag_value(&kept, "-map_metadata").as_deref(), Some("0"));
        assert!(!kept.iter().any(|a| a == "-1"), "保留模式不应出现剥离值");
    }

    #[test]
    fn png_target_is_single_pass_and_reports_limited_gain() {
        let args = compression_args(
            "in.png",
            "out.png",
            &image("png", CompressParams::default()),
            100,
            true,
        )
        .unwrap();
        assert_eq!(
            flag_value(&args, "-compression_level").as_deref(),
            Some("9")
        );
        assert_eq!(flag_value(&args, "-pix_fmt").as_deref(), Some("pal8"));
        assert!(!args.iter().any(|a| a == "-q:v"));

        let outcome = SearchOutcome {
            quality: 100,
            bytes: 900_000,
            iterations: 1,
            met: false,
            log: vec![(100, 900_000)],
        };
        let message = report_message("png", &outcome);
        assert!(message.contains("收益有限"), "{message}");
        assert!(!outcome.met);
    }

    #[test]
    fn png_quality_maps_to_compression_level() {
        assert_eq!(png_compression_level(1), 0);
        assert_eq!(png_compression_level(100), 9);
        let args = compression_args(
            "in.png",
            "out.png",
            &image(
                "png",
                CompressParams {
                    mode: "quality".into(),
                    quality: 100,
                    png_palette: false,
                    ..Default::default()
                },
            ),
            100,
            false,
        )
        .unwrap();
        assert_eq!(
            flag_value(&args, "-compression_level").as_deref(),
            Some("9")
        );
    }

    #[test]
    fn validate_rejects_bmp_tiff_and_lossless_target() {
        let bmp = image("bmp", CompressParams::default());
        assert!(args::validate_compress(&bmp)
            .unwrap_err()
            .contains("不适用"));
        let tiff = image("tiff", CompressParams::default());
        assert!(args::validate_compress(&tiff)
            .unwrap_err()
            .contains("不适用"));

        let lossless_target = image(
            "webp",
            CompressParams {
                mode: "target".into(),
                lossless: true,
                ..Default::default()
            },
        );
        assert!(args::validate_compress(&lossless_target)
            .unwrap_err()
            .contains("无损"));
        let lossless_quality = image(
            "webp",
            CompressParams {
                mode: "quality".into(),
                lossless: true,
                ..Default::default()
            },
        );
        assert!(args::validate_compress(&lossless_quality).is_ok());
    }

    #[test]
    fn target_bytes_supports_absolute_and_percent() {
        let absolute = CompressParams {
            mode: "target".into(),
            target_mode: "absolute".into(),
            target_kb: 500,
            ..Default::default()
        };
        assert_eq!(target_bytes(1_000_000, &absolute), Some(500 * 1024));
        let percent = CompressParams {
            mode: "target".into(),
            target_mode: "percent".into(),
            target_percent: 50.0,
            ..Default::default()
        };
        assert_eq!(target_bytes(1_000_000, &percent), Some(500_000));
        let quality = CompressParams {
            mode: "quality".into(),
            ..Default::default()
        };
        assert_eq!(target_bytes(1_000_000, &quality), None);
    }

    #[test]
    fn compress_defaults_match_spec() {
        let c = CompressParams::default();
        assert_eq!(c.mode, "target");
        assert_eq!(c.target_mode, "absolute");
        assert_eq!(c.target_kb, 500);
        assert_eq!(c.target_percent, 60.0);
        assert_eq!(c.quality, 80);
        assert!(!c.keep_metadata);
        assert!(c.png_palette);
        assert!(!c.lossless);
    }
}
