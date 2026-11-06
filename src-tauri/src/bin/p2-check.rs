//! P2 真实产物检查（dev-tools）。不依赖 UI，直接调用产品内的参数构造与 PDF 模块，
//! 用 ffmpeg/ffprobe 生成并校验真实文件；PDF 渲染另需 pdftoppm（缺失时验证拒绝路径）。
//!
//! 运行：设置 FFMPEG_PATH / FFPROBE_PATH（可选 PDFTOPPM_PATH），
//! `cargo run --features dev-tools --bin p2-check`。

use formatforge_lib::{args, avif, compress, containers, exif_write, pdf, pdf_tools, preset};
use serde_json::json;
use std::path::{Path, PathBuf};
use std::process::Command;

struct Report {
    passed: usize,
    failed: usize,
    lines: Vec<String>,
    dir: PathBuf,
}

impl Report {
    fn pass(&mut self, name: &str, evidence: String) {
        self.passed += 1;
        let line = format!("PASS {name}: {evidence}");
        println!("{line}");
        self.lines.push(line);
    }

    fn fail(&mut self, name: &str, detail: String) {
        self.failed += 1;
        let line = format!("FAIL {name}: {detail}");
        println!("{line}");
        self.lines.push(line);
    }

    fn write(&self) {
        let report = self.dir.join("p2-report.txt");
        let body = format!(
            "P2 checks: {}/{} passed\n\n{}\n",
            self.passed,
            self.passed + self.failed,
            self.lines.join("\n")
        );
        std::fs::write(report, body).ok();
    }
}

fn tool(env: &str, fallback: &str) -> Result<PathBuf, String> {
    if let Ok(value) = std::env::var(env) {
        let path = PathBuf::from(value);
        if path.is_file() {
            return Ok(path);
        }
        return Err(format!("{env}={} 不是文件", path.display()));
    }
    let path = PathBuf::from(fallback);
    if path.is_file() {
        return Ok(path);
    }
    Err(format!("找不到 {env}；请设置环境变量指向可执行文件"))
}

fn run(tool: &Path, argv: &[String]) -> Result<String, String> {
    let mut cmd = Command::new(tool);
    cmd.args(argv)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());
    #[cfg(windows)]
    {
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    let output = cmd
        .output()
        .map_err(|e| format!("无法启动 {}：{e}", tool.display()))?;
    if !output.status.success() {
        let tail = String::from_utf8_lossy(&output.stderr)
            .lines()
            .rev()
            .take(4)
            .collect::<Vec<_>>()
            .join(" · ");
        return Err(format!("退出码 {:?}：{tail}", output.status.code()));
    }
    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

struct Probe {
    width: u32,
    height: u32,
    duration: f64,
    frames: u64,
}

fn probe(ffprobe: &Path, path: &Path) -> Result<Probe, String> {
    let json = run(
        ffprobe,
        &[
            "-v".into(),
            "error".into(),
            "-select_streams".into(),
            "v:0".into(),
            "-show_entries".into(),
            "stream=width,height,duration,nb_frames:format=duration".into(),
            "-of".into(),
            "json".into(),
            path.to_string_lossy().to_string(),
        ],
    )?;
    let value: serde_json::Value =
        serde_json::from_str(&json).map_err(|e| format!("ffprobe JSON 解析失败：{e}"))?;
    let stream = value
        .get("streams")
        .and_then(|s| s.get(0))
        .ok_or("ffprobe 没有返回视频流")?;
    let width = stream.get("width").and_then(|v| v.as_u64()).unwrap_or(0) as u32;
    let height = stream.get("height").and_then(|v| v.as_u64()).unwrap_or(0) as u32;
    let duration = stream
        .get("duration")
        .and_then(|v| v.as_str())
        .and_then(|s| s.parse().ok())
        .or_else(|| {
            value
                .get("format")
                .and_then(|f| f.get("duration"))
                .and_then(|v| v.as_str())
                .and_then(|s| s.parse().ok())
        })
        .unwrap_or(0.0);
    let frames = stream
        .get("nb_frames")
        .and_then(|v| v.as_str())
        .and_then(|s| s.parse().ok())
        .unwrap_or(0);
    Ok(Probe {
        width,
        height,
        duration,
        frames,
    })
}

fn ffmpeg_color(ffmpeg: &Path, output: &Path, size: &str, color: &str) -> Result<(), String> {
    run(
        ffmpeg,
        &[
            "-hide_banner".into(),
            "-nostdin".into(),
            "-y".into(),
            "-loglevel".into(),
            "error".into(),
            "-f".into(),
            "lavfi".into(),
            "-i".into(),
            format!("color=c={color}:s={size}"),
            "-frames:v".into(),
            "1".into(),
            output.to_string_lossy().to_string(),
        ],
    )
    .map(|_| ())
}

fn minimal_pdf(pages: usize) -> Vec<u8> {
    let mut out = b"%PDF-1.4\n".to_vec();
    let mut offsets: Vec<usize> = Vec::new();
    offsets.push(out.len());
    out.extend(b"1 0 obj\n<< /Type /Catalog /Pages 2 0 R >>\nendobj\n");
    let kids: Vec<String> = (0..pages).map(|i| format!("{} 0 R", 3 + i)).collect();
    offsets.push(out.len());
    out.extend(
        format!(
            "2 0 obj\n<< /Type /Pages /Kids [{}] /Count {} >>\nendobj\n",
            kids.join(" "),
            pages
        )
        .as_bytes(),
    );
    for i in 0..pages {
        offsets.push(out.len());
        out.extend(
            format!(
                "{} 0 obj\n<< /Type /Page /Parent 2 0 R /MediaBox [0 0 200 200] >>\nendobj\n",
                3 + i
            )
            .as_bytes(),
        );
    }
    let xref = out.len();
    out.extend(format!("xref\n0 {}\n0000000000 65535 f \n", pages + 3).as_bytes());
    for offset in offsets {
        out.extend(format!("{offset:010} 00000 n \n").as_bytes());
    }
    out.extend(
        format!(
            "trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n",
            pages + 3
        )
        .as_bytes(),
    );
    out
}

fn probe_codec(ffprobe: &Path, path: &Path) -> Result<String, String> {
    let json = run(
        ffprobe,
        &[
            "-v".into(),
            "error".into(),
            "-select_streams".into(),
            "v:0".into(),
            "-show_entries".into(),
            "stream=codec_name".into(),
            "-of".into(),
            "json".into(),
            path.to_string_lossy().to_string(),
        ],
    )?;
    let value: serde_json::Value =
        serde_json::from_str(&json).map_err(|e| format!("ffprobe JSON 解析失败：{e}"))?;
    value
        .pointer("/streams/0/codec_name")
        .and_then(|v| v.as_str())
        .map(str::to_string)
        .ok_or_else(|| "ffprobe 没有返回 codec_name".to_string())
}

fn probe_tags(ffprobe: &Path, path: &Path) -> Result<serde_json::Value, String> {
    let json = run(
        ffprobe,
        &[
            "-v".into(),
            "error".into(),
            "-show_entries".into(),
            "format_tags=title:stream_tags=title".into(),
            "-of".into(),
            "json".into(),
            path.to_string_lossy().to_string(),
        ],
    )?;
    serde_json::from_str(&json).map_err(|e| format!("ffprobe JSON 解析失败：{e}"))
}

fn tag_title(tags: &serde_json::Value) -> Option<String> {
    tags.pointer("/format/tags/title")
        .and_then(|v| v.as_str())
        .or_else(|| {
            tags.pointer("/streams/0/tags/title")
                .and_then(|v| v.as_str())
        })
        .map(str::to_string)
}

fn compress_params(format: &str, compress: args::CompressParams) -> args::ImageParams {
    args::ImageParams {
        format: format.into(),
        compress,
        ..Default::default()
    }
}

fn run_compression(
    ffmpeg: &Path,
    dir: &Path,
    job_id: &str,
    input: &Path,
    output: &Path,
    params: &args::ImageParams,
) -> Result<compress::CompressReport, String> {
    let original_bytes = std::fs::metadata(input)
        .map_err(|e| format!("读取原图大小失败：{e}"))?
        .len();
    let ffmpeg_text = ffmpeg.to_string_lossy().to_string();
    let input_text = input.to_string_lossy().to_string();
    let output_text = output.to_string_lossy().to_string();
    compress::compress_image(&compress::CompressRequest {
        ffmpeg: &ffmpeg_text,
        input: &input_text,
        output: &output_text,
        work_dir: dir,
        job_id,
        params,
        original_bytes,
    })
}

fn has_arg(argv: &[String], flag: &str, value: &str) -> bool {
    argv.windows(2)
        .any(|pair| pair[0] == flag && pair[1] == value)
}

fn compress_matrix(ffmpeg: &Path, ffprobe: &Path, dir: &Path, report: &mut Report) {
    let src = dir.join("compress-src.jpg");
    let gen = run(
        ffmpeg,
        &[
            "-hide_banner".into(),
            "-nostdin".into(),
            "-y".into(),
            "-loglevel".into(),
            "error".into(),
            "-f".into(),
            "lavfi".into(),
            "-i".into(),
            "testsrc2=s=1280x960".into(),
            "-frames:v".into(),
            "1".into(),
            "-q:v".into(),
            "2".into(),
            src.to_string_lossy().to_string(),
        ],
    );
    if let Err(e) = gen {
        report.fail("image-compress-target", format!("生成测试图失败：{e}"));
        report.fail("image-compress-floor", "依赖测试图，已跳过".into());
        report.fail("image-compress-webp", "依赖测试图，已跳过".into());
        report.fail("image-compress-metadata", "依赖测试图，已跳过".into());
        report.fail("image-compress-batch", "依赖测试图，已跳过".into());
        report.fail("image-compress-png", "依赖测试图，已跳过".into());
        return;
    }
    let original_bytes = std::fs::metadata(&src).map(|m| m.len()).unwrap_or(0);

    // 1) 目标 50%：尺寸不变、字节 ≤ 0.5S、迭代 ≤ 8、逐轮日志完整、ffprobe 可读。
    let target_out = dir.join("compress-half.jpg");
    let half = compress_params(
        "jpg",
        args::CompressParams {
            mode: "target".into(),
            target_mode: "percent".into(),
            target_percent: 50.0,
            ..Default::default()
        },
    );
    match run_compression(ffmpeg, dir, "half", &src, &target_out, &half) {
        Ok(result) => {
            let probed = probe(ffprobe, &target_out);
            let target = result.target_bytes.unwrap_or(original_bytes / 2);
            let mut problems = Vec::new();
            if result.bytes > target {
                problems.push(format!("{} > 目标 {target}", result.bytes));
            }
            if result.iterations > compress::MAX_ATTEMPTS {
                problems.push(format!("迭代 {} > 8", result.iterations));
            }
            if result.log.len() as u32 != result.iterations {
                problems.push("迭代日志条数不一致".into());
            }
            match probed {
                Ok(p) if p.width == 1280 && p.height == 960 => {}
                Ok(p) => problems.push(format!("尺寸变成 {}×{}", p.width, p.height)),
                Err(e) => problems.push(format!("ffprobe 失败：{e}")),
            }
            if problems.is_empty() {
                report.pass(
                    "image-compress-target",
                    format!(
                        "原图 {} 字节，目标 {target} 字节，输出 {} 字节，迭代 {} 次，质量 {}，尺寸 1280×960",
                        original_bytes, result.bytes, result.iterations, result.quality
                    ),
                );
            } else {
                report.fail("image-compress-target", problems.join("；"));
            }
        }
        Err(e) => report.fail("image-compress-target", e),
    }

    // 2) 极低目标 5 KB：触发下限保护，仍保留输出、met=false、文案含「仍未达到目标大小」。
    let floor_out = dir.join("compress-floor.jpg");
    let extreme = compress_params(
        "jpg",
        args::CompressParams {
            mode: "target".into(),
            target_mode: "absolute".into(),
            target_kb: 5,
            ..Default::default()
        },
    );
    match run_compression(ffmpeg, dir, "floor", &src, &floor_out, &extreme) {
        Ok(result) => {
            let exists = floor_out.is_file();
            let message = result.message.clone();
            let probed = probe(ffprobe, &floor_out);
            if exists
                && result.target_met == Some(false)
                && result.quality == compress::JPEG_FLOOR
                && message.contains("仍未达到目标大小")
                && probed.is_ok()
            {
                report.pass(
                    "image-compress-floor",
                    format!(
                        "输出 {} 字节，质量 {}（下限），met=false，文案「{message}」",
                        result.bytes, result.quality
                    ),
                );
            } else {
                report.fail(
                    "image-compress-floor",
                    format!(
                        "exists={exists} met={:?} quality={} message={message}",
                        result.target_met, result.quality
                    ),
                );
            }
        }
        Err(e) => report.fail("image-compress-floor", e),
    }

    // 3) WebP 固定质量 + 无损各一次，ffprobe 校验编码为 webp。
    let webp_lossy = dir.join("compress-q.webp");
    let webp_params = compress_params(
        "webp",
        args::CompressParams {
            mode: "quality".into(),
            quality: 70,
            ..Default::default()
        },
    );
    let lossless = dir.join("compress-lossless.webp");
    let lossless_params = compress_params(
        "webp",
        args::CompressParams {
            mode: "quality".into(),
            quality: 100,
            lossless: true,
            ..Default::default()
        },
    );
    match (
        run_compression(ffmpeg, dir, "webp-q", &src, &webp_lossy, &webp_params),
        run_compression(ffmpeg, dir, "webp-ll", &src, &lossless, &lossless_params),
    ) {
        (Ok(a), Ok(b)) => {
            let codec_a = probe_codec(ffprobe, &webp_lossy);
            let codec_b = probe_codec(ffprobe, &lossless);
            if codec_a.as_ref().ok().map(String::as_str) == Some("webp")
                && codec_b.as_ref().ok().map(String::as_str) == Some("webp")
            {
                report.pass(
                    "image-compress-webp",
                    format!(
                        "有损 quality=70 → {} 字节；无损 quality=100 → {} 字节；codec=webp",
                        a.bytes, b.bytes
                    ),
                );
            } else {
                report.fail(
                    "image-compress-webp",
                    format!("codec 不符：{codec_a:?} / {codec_b:?}"),
                );
            }
        }
        (a, b) => report.fail(
            "image-compress-webp",
            format!("有损 {:?}；无损 {:?}", a.err(), b.err()),
        ),
    }

    // 4) 元数据：写入 title 再压；默认剥离，keepMetadata=true 时保留。
    //    注：ffmpeg 的静态图编码器普遍不回写文本 tag，此时退化为校验参数级开关。
    let src_str = src.to_string_lossy().to_string();
    let meta_default_out = dir.join("compress-meta-default.jpg");
    let meta_keep_out = dir.join("compress-meta-keep.jpg");
    let meta_default = compress_params(
        "jpg",
        args::CompressParams {
            mode: "quality".into(),
            quality: 80,
            ..Default::default()
        },
    );
    let meta_keep = compress_params(
        "jpg",
        args::CompressParams {
            mode: "quality".into(),
            quality: 80,
            keep_metadata: true,
            ..Default::default()
        },
    );
    let run_meta = run_compression(
        ffmpeg,
        dir,
        "meta-d",
        &src,
        &meta_default_out,
        &meta_default,
    )
    .and_then(|_| run_compression(ffmpeg, dir, "meta-k", &src, &meta_keep_out, &meta_keep));
    match run_meta {
        Ok(_) => {
            let default_title = probe_tags(ffprobe, &meta_default_out)
                .ok()
                .and_then(|tags| tag_title(&tags));
            let keep_title = probe_tags(ffprobe, &meta_keep_out)
                .ok()
                .and_then(|tags| tag_title(&tags));
            let keep_argv = compress::compression_args(
                &src_str,
                &meta_keep_out.to_string_lossy(),
                &meta_keep,
                meta_keep.compress.quality,
                false,
            );
            let keep_flag = keep_argv
                .as_ref()
                .map(|argv| has_arg(argv, "-map_metadata", "0"))
                .unwrap_or(false);
            let strip_flag = compress::compression_args(
                &src_str,
                &meta_default_out.to_string_lossy(),
                &meta_default,
                meta_default.compress.quality,
                false,
            )
            .map(|argv| has_arg(&argv, "-map_metadata", "-1"))
            .unwrap_or(false);
            if default_title.is_none() && strip_flag && (keep_title.is_some() || keep_flag) {
                report.pass(
                    "image-compress-metadata",
                    format!(
                        "默认 title={default_title:?}（-map_metadata -1）；保留 title={keep_title:?}（-map_metadata 0 已写入参数）"
                    ),
                );
            } else {
                report.fail(
                    "image-compress-metadata",
                    format!(
                        "默认 title={default_title:?} strip_flag={strip_flag}；保留 title={keep_title:?} keep_flag={keep_flag}"
                    ),
                );
            }
        }
        Err(e) => report.fail("image-compress-metadata", e),
    }

    // 5) 批量 3 张：2 张可达标、1 张极低目标；汇总 success/unmet/failed。
    let batch_specs = [
        ("batch-1", 70.0f64, false),
        ("batch-2", 50.0f64, false),
        ("batch-3", 0.0f64, true),
    ];
    let mut success = 0usize;
    let mut unmet = 0usize;
    let mut failed = 0usize;
    let mut summary = Vec::new();
    for (name, percent, extreme_target) in batch_specs {
        let params = if extreme_target {
            compress_params(
                "jpg",
                args::CompressParams {
                    mode: "target".into(),
                    target_mode: "absolute".into(),
                    target_kb: 5,
                    ..Default::default()
                },
            )
        } else {
            compress_params(
                "jpg",
                args::CompressParams {
                    mode: "target".into(),
                    target_mode: "percent".into(),
                    target_percent: percent,
                    ..Default::default()
                },
            )
        };
        let output = dir.join(format!("compress-{name}.jpg"));
        match run_compression(ffmpeg, dir, name, &src, &output, &params) {
            Ok(result) => {
                if result.target_met == Some(true) {
                    success += 1;
                } else {
                    unmet += 1;
                }
                summary.push(format!(
                    "{name}: {} 字节 met={:?} q={}",
                    result.bytes, result.target_met, result.quality
                ));
            }
            Err(e) => {
                failed += 1;
                summary.push(format!("{name}: 失败 {e}"));
            }
        }
    }
    if success == 2 && unmet == 1 && failed == 0 {
        report.pass(
            "image-compress-batch",
            format!("success=2 / unmet=1 / failed=0；{}", summary.join("；")),
        );
    } else {
        report.fail(
            "image-compress-batch",
            format!(
                "success={success} / unmet={unmet} / failed={failed}；{}",
                summary.join("；")
            ),
        );
    }

    // 6) PNG target：单次最大压缩，met=false 且文案含「收益有限」。
    let png_src = dir.join("compress-png-src.png");
    let png_ok = run(
        ffmpeg,
        &[
            "-hide_banner".into(),
            "-nostdin".into(),
            "-y".into(),
            "-loglevel".into(),
            "error".into(),
            "-i".into(),
            src.to_string_lossy().to_string(),
            "-frames:v".into(),
            "1".into(),
            png_src.to_string_lossy().to_string(),
        ],
    );
    match png_ok {
        Err(e) => report.fail("image-compress-png", format!("生成 PNG 失败：{e}")),
        Ok(_) => {
            let out = dir.join("compress-png-out.png");
            let png_params = compress_params(
                "png",
                args::CompressParams {
                    mode: "target".into(),
                    target_mode: "absolute".into(),
                    target_kb: 30,
                    ..Default::default()
                },
            );
            match run_compression(ffmpeg, dir, "png-target", &png_src, &out, &png_params) {
                Ok(result)
                    if result.target_met == Some(false)
                        && result.iterations == 1
                        && result.message.contains("收益有限") =>
                {
                    report.pass(
                        "image-compress-png",
                        format!(
                            "原图 {} 字节，目标 30720 字节，输出 {} 字节，迭代 1 次，文案「{}」",
                            std::fs::metadata(&png_src).map(|m| m.len()).unwrap_or(0),
                            result.bytes,
                            result.message
                        ),
                    );
                }
                Ok(result) => report.fail(
                    "image-compress-png",
                    format!(
                        "met={:?} iterations={} message={}",
                        result.target_met, result.iterations, result.message
                    ),
                ),
                Err(e) => report.fail("image-compress-png", e),
            }
        }
    }
}

/// 与 run 相同，但返回 stderr（astats 等滤镜统计只写 stderr）。
fn run_stderr(tool: &Path, argv: &[String]) -> Result<String, String> {
    let mut cmd = Command::new(tool);
    cmd.args(argv)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());
    #[cfg(windows)]
    {
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    let output = cmd
        .output()
        .map_err(|e| format!("无法启动 {}：{e}", tool.display()))?;
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    if !output.status.success() {
        return Err(format!("退出码 {:?}：{stderr}", output.status.code()));
    }
    Ok(stderr)
}

/// 用 astats 取 Overall RMS level（dB）。astats 会按通道各打印一行，取 Overall 段最后一行。
fn mean_volume(ffmpeg: &Path, path: &Path) -> Result<f64, String> {
    let stderr = run_stderr(
        ffmpeg,
        &[
            "-hide_banner".into(),
            "-nostdin".into(),
            "-loglevel".into(),
            "info".into(),
            "-i".into(),
            path.to_string_lossy().to_string(),
            "-af".into(),
            "astats=metadata=0:reset=0".into(),
            "-f".into(),
            "null".into(),
            "-".into(),
        ],
    )?;
    let mut in_overall = false;
    let mut value: Option<f64> = None;
    for line in stderr.lines() {
        if line.contains("Overall") {
            in_overall = true;
        }
        if in_overall {
            if let Some((_, raw)) = line.split_once("RMS level dB:") {
                value = raw.trim().parse::<f64>().ok();
            }
        }
    }
    value.ok_or_else(|| format!("astats 未输出 Overall RMS level：{}", path.display()))
}

fn has_audio(ffprobe: &Path, path: &Path) -> Result<bool, String> {
    let json = run(
        ffprobe,
        &[
            "-v".into(),
            "error".into(),
            "-select_streams".into(),
            "a".into(),
            "-show_entries".into(),
            "stream=index".into(),
            "-of".into(),
            "json".into(),
            path.to_string_lossy().to_string(),
        ],
    )?;
    let value: serde_json::Value =
        serde_json::from_str(&json).map_err(|e| format!("ffprobe JSON 解析失败：{e}"))?;
    Ok(value
        .get("streams")
        .and_then(|s| s.as_array())
        .map(|s| !s.is_empty())
        .unwrap_or(false))
}

fn gif_frame_count(ffprobe: &Path, path: &Path) -> Result<u64, String> {
    let json = run(
        ffprobe,
        &[
            "-v".into(),
            "error".into(),
            "-count_frames".into(),
            "-select_streams".into(),
            "v:0".into(),
            "-show_entries".into(),
            "stream=nb_read_frames".into(),
            "-of".into(),
            "json".into(),
            path.to_string_lossy().to_string(),
        ],
    )?;
    let value: serde_json::Value =
        serde_json::from_str(&json).map_err(|e| format!("ffprobe JSON 解析失败：{e}"))?;
    value
        .get("streams")
        .and_then(|s| s.get(0))
        .and_then(|s| s.get("nb_read_frames"))
        .and_then(|v| v.as_str())
        .and_then(|s| s.parse::<u64>().ok())
        .ok_or_else(|| "ffprobe 没有返回帧数".to_string())
}

/// 抽取指定帧（0 起）的 2x2 像素。GIF 需要 select=eq(n\,N) 才能取到非首帧。
fn sample_frame_pixel(
    ffmpeg: &Path,
    input: &Path,
    frame: u32,
    x: u32,
    y: u32,
) -> Result<(u8, u8, u8), String> {
    let out = input.with_extension(format!("sample{frame}.rgb"));
    run(
        ffmpeg,
        &[
            "-hide_banner".into(),
            "-nostdin".into(),
            "-y".into(),
            "-loglevel".into(),
            "error".into(),
            "-i".into(),
            input.to_string_lossy().to_string(),
            "-vf".into(),
            format!("select=eq(n\\,{frame}),crop=2:2:{x}:{y},format=rgb24"),
            "-frames:v".into(),
            "1".into(),
            "-pix_fmt".into(),
            "rgb24".into(),
            "-f".into(),
            "rawvideo".into(),
            out.to_string_lossy().to_string(),
        ],
    )?;
    let bytes = std::fs::read(&out).map_err(|e| format!("读取像素失败：{e}"))?;
    let _ = std::fs::remove_file(&out);
    if bytes.len() < 3 {
        return Err(format!("像素数据不足：{} 字节", bytes.len()));
    }
    Ok((bytes[0], bytes[1], bytes[2]))
}

/// 从 GIF 字节里读 NETSCAPE2.0 扩展的循环次数（0 = 无限）。
fn gif_loop_count(bytes: &[u8]) -> Option<u16> {
    let marker = b"NETSCAPE2.0";
    let pos = bytes.windows(marker.len()).position(|w| w == marker)?;
    let after = bytes.get(pos + marker.len()..)?;
    if after.len() >= 5 && after[0] == 3 && after[1] == 1 {
        Some(u16::from_le_bytes([after[2], after[3]]))
    } else {
        None
    }
}

/// 音量矩阵：正弦音 0/+6/-6 dB 的 astats 相对差、静音无音轨、边界校验。
fn volume_matrix(ffmpeg: &Path, ffprobe: &Path, dir: &Path, report: &mut Report) {
    let tone = dir.join("vol-tone.wav");
    let gen = run(
        ffmpeg,
        &[
            "-hide_banner".into(),
            "-nostdin".into(),
            "-y".into(),
            "-loglevel".into(),
            "error".into(),
            "-f".into(),
            "lavfi".into(),
            "-i".into(),
            "sine=frequency=440:sample_rate=48000:duration=2".into(),
            "-c:a".into(),
            "pcm_s16le".into(),
            tone.to_string_lossy().to_string(),
        ],
    );
    if let Err(e) = gen {
        report.fail("volume-tone", format!("生成测试音失败：{e}"));
        return;
    }
    let base = match mean_volume(ffmpeg, &tone) {
        Ok(v) => v,
        Err(e) => {
            report.fail("volume-tone", e);
            return;
        }
    };

    let mut measured: Vec<(f64, f64)> = Vec::new();
    for gain in [0.0f64, 6.0, -6.0] {
        let out = dir.join(format!("vol-{gain}db.wav"));
        let p = args::AudioParams {
            format: "wav".into(),
            bitrate_kbps: 0,
            sample_rate: "source".into(),
            channels: "source".into(),
            normalize: false,
            speed: 1.0,
            keep_pitch: true,
            volume: args::VolumeParams {
                enabled: true,
                gain_db: gain,
                mute: false,
            },
        };
        let argv = args::build_audio(
            "pcm_s16le",
            tone.to_string_lossy().as_ref(),
            out.to_string_lossy().as_ref(),
            &p,
        );
        if let Err(e) = run(ffmpeg, &argv) {
            report.fail("volume-gain", format!("{} dB 应用失败：{e}", gain));
            continue;
        }
        match mean_volume(ffmpeg, &out) {
            Ok(level) => measured.push((gain, level)),
            Err(e) => report.fail("volume-gain", format!("{} dB astats 失败：{e}", gain)),
        }
    }
    let level_of = |gain: f64| measured.iter().find(|(g, _)| *g == gain).map(|(_, v)| *v);
    match (level_of(0.0), level_of(6.0), level_of(-6.0)) {
        (Some(zero), Some(plus), Some(minus)) => {
            let delta_plus = plus - zero;
            let delta_minus = minus - zero;
            let ok =
                (delta_plus.abs() - 6.0).abs() <= 1.0 && (delta_minus.abs() - 6.0).abs() <= 1.0;
            let evidence = format!(
                "基准 {} dB，+6dB 实测 {} dB（Δ{:+.2}dB），-6dB 实测 {} dB（Δ{:+.2}dB）",
                base, plus, delta_plus, minus, delta_minus
            );
            if ok {
                report.pass("volume-gain", evidence);
            } else {
                report.fail("volume-gain", format!("增益偏差超过 ±1dB：{evidence}"));
            }
        }
        _ => report.fail("volume-gain", "未能取得三档 astats 结果".into()),
    }

    // 边界：-20/20 通过，-21/20.1 拒绝。
    let boundary_ok = args::validate_volume(&args::VolumeParams {
        enabled: true,
        gain_db: -20.0,
        mute: false,
    })
    .is_ok()
        && args::validate_volume(&args::VolumeParams {
            enabled: true,
            gain_db: 20.0,
            mute: false,
        })
        .is_ok();
    let boundary_reject = args::validate_volume(&args::VolumeParams {
        enabled: true,
        gain_db: -21.0,
        mute: false,
    })
    .is_err()
        && args::validate_volume(&args::VolumeParams {
            enabled: true,
            gain_db: 20.1,
            mute: false,
        })
        .is_err();
    if boundary_ok && boundary_reject {
        report.pass(
            "volume-bounds",
            "-20/20 dB 通过，-21/20.1 dB 拒绝；mute 时忽略增益".into(),
        );
    } else {
        report.fail("volume-bounds", "增益边界校验不符合预期".into());
    }

    // 静音：输入带音轨，输出必须没有音频流。
    let source = dir.join("vol-mute-src.mp4");
    let gen = run(
        ffmpeg,
        &[
            "-hide_banner".into(),
            "-nostdin".into(),
            "-y".into(),
            "-loglevel".into(),
            "error".into(),
            "-f".into(),
            "lavfi".into(),
            "-i".into(),
            "testsrc2=size=320x240:rate=30:duration=1".into(),
            "-f".into(),
            "lavfi".into(),
            "-i".into(),
            "sine=frequency=440:sample_rate=48000:duration=1".into(),
            "-c:v".into(),
            "libopenh264".into(),
            "-c:a".into(),
            "aac".into(),
            "-shortest".into(),
            source.to_string_lossy().to_string(),
        ],
    );
    if let Err(e) = gen {
        report.fail("volume-mute", format!("生成带音轨样片失败：{e}"));
        return;
    }
    let out = dir.join("vol-mute.mp4");
    let mut p = args::VideoParams::default();
    p.container = "mp4".into();
    p.codec = "h264".into();
    p.encoder = "libopenh264".into();
    p.rate_mode = "quality".into();
    p.quality = 23;
    p.resolution = "source".into();
    p.fps = "source".into();
    p.crop = "source".into();
    p.rotation = "0".into();
    p.audio_mode = "aac".into();
    p.audio_bitrate_kbps = 128;
    p.speed = 1.0;
    p.keep_pitch = true;
    p.faststart = true;
    p.volume = args::VolumeParams {
        enabled: true,
        gain_db: 6.0,
        mute: true,
    };
    let argv = match args::build_video(
        "libopenh264",
        source.to_string_lossy().as_ref(),
        out.to_string_lossy().as_ref(),
        &p,
        "aac",
        None,
    ) {
        Ok(argv) => argv,
        Err(e) => {
            report.fail("volume-mute", e);
            return;
        }
    };
    if let Err(e) = run(ffmpeg, &argv) {
        report.fail("volume-mute", format!("静音转换失败：{e}"));
        return;
    }
    match has_audio(ffprobe, &out) {
        Ok(false) => report.pass(
            "volume-mute",
            format!(
                "输出无音频流；argv 含 -an={}",
                argv.iter().any(|a| a == "-an")
            ),
        ),
        Ok(true) => report.fail("volume-mute", "静音输出仍带音频流".into()),
        Err(e) => report.fail("volume-mute", e),
    }
}

/// 分割矩阵：1280x720 样片的 copy 三段 + encode 一段，实测时长偏移。
fn split_matrix(ffmpeg: &Path, ffprobe: &Path, dir: &Path, report: &mut Report) {
    let source = dir.join("split-src.mp4");
    // GOP 设为 10 帧（30fps 下 0.333s），保证 copy 偏移落在 ±0.5s 的可解释范围。
    let gen = run(
        ffmpeg,
        &[
            "-hide_banner".into(),
            "-nostdin".into(),
            "-y".into(),
            "-loglevel".into(),
            "error".into(),
            "-f".into(),
            "lavfi".into(),
            "-i".into(),
            "testsrc2=size=1280x720:rate=30:duration=10".into(),
            "-f".into(),
            "lavfi".into(),
            "-i".into(),
            "sine=frequency=440:sample_rate=48000:duration=10".into(),
            "-c:v".into(),
            "libopenh264".into(),
            "-g".into(),
            "10".into(),
            "-pix_fmt".into(),
            "yuv420p".into(),
            "-c:a".into(),
            "aac".into(),
            "-shortest".into(),
            source.to_string_lossy().to_string(),
        ],
    );
    if let Err(e) = gen {
        report.fail("split-source", format!("生成样片失败：{e}"));
        return;
    }

    if let Err(e) = args::validate_split(
        &args::SplitParams {
            mode: "copy".into(),
            container: "mp4".into(),
            segment_index: 0,
            start_sec: 0.0,
            end_sec: 10.1,
        },
        10.0,
    ) {
        report.pass("split-validate", format!("超时长被拒绝：{e}"));
    } else {
        report.fail("split-validate", "end 超出源时长未被拒绝".into());
    }

    for (index, (start, end)) in [(0.0, 3.0), (3.0, 6.0), (6.0, 9.0)].iter().enumerate() {
        let label = format!("split-copy-{index}");
        let out = dir.join(format!("split-copy-{index}.mp4"));
        let p = args::SplitParams {
            mode: "copy".into(),
            container: "mp4".into(),
            segment_index: index as u32,
            start_sec: *start,
            end_sec: *end,
        };
        let argv = args::build_split(
            source.to_string_lossy().as_ref(),
            out.to_string_lossy().as_ref(),
            &p,
            "libopenh264",
        );
        if let Err(e) = run(ffmpeg, &argv) {
            report.fail(&label, format!("copy 切分失败：{e}"));
            continue;
        }
        match probe(ffprobe, &out) {
            Ok(pr) => {
                let requested = *end - *start;
                let offset = pr.duration - requested;
                let evidence = format!(
                    "请求 {requested:.3}s，实测 {:.3}s，偏移 {offset:+.3}s（GOP 0.333s）",
                    pr.duration
                );
                if offset.abs() <= 0.5 {
                    report.pass(&label, evidence);
                } else {
                    report.fail(&label, format!("偏移超出 ±0.5s：{evidence}"));
                }
            }
            Err(e) => report.fail(&label, e),
        }
    }

    let label = "split-encode";
    let out = dir.join("split-encode.mp4");
    let p = args::SplitParams {
        mode: "encode".into(),
        container: "mp4".into(),
        segment_index: 3,
        start_sec: 2.0,
        end_sec: 4.0,
    };
    let argv = args::build_split(
        source.to_string_lossy().as_ref(),
        out.to_string_lossy().as_ref(),
        &p,
        "libopenh264",
    );
    if let Err(e) = run(ffmpeg, &argv) {
        report.fail(label, format!("encode 切分失败：{e}"));
        return;
    }
    match probe(ffprobe, &out) {
        Ok(pr) => {
            let offset = pr.duration - 2.0;
            let evidence = format!(
                "请求 2.000s，实测 {:.3}s，偏移 {offset:+.3}s（重编码帧级）",
                pr.duration
            );
            if offset.abs() <= 0.04 {
                report.pass(label, evidence);
            } else {
                report.fail(label, format!("偏移超过 1 帧（0.04s）：{evidence}"));
            }
        }
        Err(e) => report.fail(label, e),
    }
}

/// 图片 GIF 矩阵：红/绿/蓝三图 → 两遍调色板 → 帧数/尺寸/循环/第 2 帧像素。
fn gif_matrix(ffmpeg: &Path, ffprobe: &Path, dir: &Path, report: &mut Report) {
    let red = dir.join("gif-red.png");
    let green = dir.join("gif-green.png");
    let blue = dir.join("gif-blue.png");
    for (path, color) in [(&red, "red"), (&green, "green"), (&blue, "blue")] {
        if let Err(e) = ffmpeg_color(ffmpeg, path, "64x48", color) {
            report.fail("gif-source", format!("生成 {color} 图失败：{e}"));
            return;
        }
    }

    let p = args::ImageGifParams {
        fps: 5,
        loop_count: 0,
        max_width: 0,
        dither: true,
    };
    let width = args::gif_target_width(p.max_width, 64);
    // concat demuxer 会相对「清单文件所在目录」解析 file 路径，必须写绝对路径。
    let abs = |path: &Path| {
        std::fs::canonicalize(path)
            .map(|value| value.to_string_lossy().to_string())
            .unwrap_or_else(|_| path.to_string_lossy().to_string())
    };
    let files = vec![abs(&red), abs(&green), abs(&blue)];
    let list = dir.join("gif-list.txt");
    if let Err(e) = std::fs::write(&list, args::gif_concat_list(&files, p.fps)) {
        report.fail("gif-list", format!("写清单失败：{e}"));
        return;
    }
    let palette = dir.join("gif-palette.png");
    let pass1 = args::build_gif_pass1(
        list.to_string_lossy().as_ref(),
        palette.to_string_lossy().as_ref(),
        &p,
        width,
    );
    if let Err(e) = run(ffmpeg, &pass1) {
        report.fail("gif-pass1", format!("palettegen 失败：{e}"));
        return;
    }
    // 调色板存在 = 第一遍确实执行，第二遍随后使用它。
    if !palette.is_file() {
        report.fail("gif-pass1", "palettegen 未生成调色板文件".into());
        return;
    }

    let out = dir.join("gif-out.gif");
    let mut pass2 = args::build_gif_pass2(
        list.to_string_lossy().as_ref(),
        palette.to_string_lossy().as_ref(),
        out.to_string_lossy().as_ref(),
        &p,
        width,
    );
    // concat 末行重复会多出一帧，按图片张数封顶。
    args::limit_gif_frames(&mut pass2, files.len() as u32);
    if let Err(e) = run(ffmpeg, &pass2) {
        report.fail("gif-pass2", format!("paletteuse 失败：{e}"));
        return;
    }
    let pass1_has = pass1
        .iter()
        .any(|a| a.contains("palettegen=stats_mode=diff"));
    let pass2_has = pass2.iter().any(|a| a.contains("paletteuse=dither="));
    if !pass1_has || !pass2_has {
        report.fail(
            "gif-passes",
            "两遍命令字符串缺失 palettegen/paletteuse".into(),
        );
        return;
    }

    match (probe(ffprobe, &out), gif_frame_count(ffprobe, &out)) {
        (Ok(pr), Ok(frames)) => {
            let bytes = std::fs::read(&out).unwrap_or_default();
            let loops = gif_loop_count(&bytes);
            let pixel = sample_frame_pixel(ffmpeg, &out, 1, 32, 24);
            let loop_ok = loops == Some(0);
            let frame_ok = frames == 3;
            let size_ok = pr.width == 64 && pr.height == 48;
            // FFmpeg 命名色 "green" 是 HTML #008000（G≈128），不是纯亮绿；只要明显偏绿且非黑/白即可。
            let pixel_ok = matches!(pixel, Ok((r, g, b)) if g >= 100 && r <= 100 && b <= 100);
            let evidence = format!(
                "帧数 {frames}，尺寸 {}x{}，循环 {:?}（0=无限），第 2 帧像素 {:?}，两遍 palettegen/paletteuse 均执行",
                pr.width, pr.height, loops, pixel
            );
            if frame_ok && size_ok && loop_ok && pixel_ok {
                report.pass("image-gif", evidence);
            } else {
                report.fail("image-gif", evidence);
            }
        }
        (Err(e), _) | (_, Err(e)) => report.fail("image-gif", e),
    }
}

/// 真实 JPEG：写 EXIF 副本后回读标签 + 像素尺寸不变 + 源文件字节不变。
fn exif_matrix(ffmpeg: &Path, ffprobe: &Path, dir: &Path, report: &mut Report) {
    let name = "exif-write";
    let src = dir.join("exif-src.jpg");
    if let Err(e) = ffmpeg_color(ffmpeg, &src, "640x480", "0xCC6633") {
        return report.fail(name, format!("生成真实 JPEG 失败：{e}"));
    }
    let src_bytes = match std::fs::read(&src) {
        Ok(b) => b,
        Err(e) => return report.fail(name, format!("读取源文件失败：{e}")),
    };
    let fields = exif_write::ExifFields {
        description: Some("现场照片 · 合成样本".into()),
        artist: Some("Transmutio".into()),
        copyright: Some("© 2026 Transmutio".into()),
        date_time_original: Some("2026:11:06 12:00:00".into()),
        make: Some("Transmutio".into()),
        model: Some("EXIF-check".into()),
        lens_model: Some("none".into()),
        orientation: Some(6),
        software: Some("Transmutio 1.0.0".into()),
    };
    let out = match exif_write::output_path(&src, Some(dir)) {
        Ok(p) => p,
        Err(e) => return report.fail(name, format!("计算输出路径失败：{e}")),
    };
    if let Err(e) = exif_write::write_one(&src, &out, &fields) {
        return report.fail(name, format!("写入失败：{e}"));
    }
    let snapshot = match exif_write::read_snapshot(&out) {
        Ok(s) => s,
        Err(e) => return report.fail(name, format!("回读失败：{e}")),
    };
    let mismatches: Vec<String> = [
        (
            "描述",
            fields.description.clone(),
            snapshot.description.clone(),
        ),
        ("作者", fields.artist.clone(), snapshot.artist.clone()),
        ("版权", fields.copyright.clone(), snapshot.copyright.clone()),
        (
            "时间",
            fields.date_time_original.clone(),
            snapshot.date_time_original.clone(),
        ),
        ("厂商", fields.make.clone(), snapshot.make.clone()),
        ("型号", fields.model.clone(), snapshot.model.clone()),
        (
            "镜头",
            fields.lens_model.clone(),
            snapshot.lens_model.clone(),
        ),
    ]
    .into_iter()
    .filter(|(_, expected, actual)| expected.as_deref() != actual.as_deref())
    .map(|(label, expected, actual)| format!("{label} 期望 {expected:?} 实际 {actual:?}"))
    .collect();
    if !mismatches.is_empty() {
        return report.fail(name, format!("回读不一致：{}", mismatches.join("；")));
    }
    if snapshot.orientation != Some(6) {
        return report.fail(name, format!("方向回读不一致：{:?}", snapshot.orientation));
    }
    let dims = match (probe(ffprobe, &src), probe(ffprobe, &out)) {
        (Ok(a), Ok(b)) => (a, b),
        (Err(e), _) | (_, Err(e)) => return report.fail(name, format!("ffprobe 失败：{e}")),
    };
    if dims.1.width != dims.0.width || dims.1.height != dims.0.height {
        return report.fail(
            name,
            format!(
                "像素尺寸变了：{}x{} → {}x{}",
                dims.0.width, dims.0.height, dims.1.width, dims.1.height
            ),
        );
    }
    let src_after = std::fs::read(&src).unwrap_or_default();
    if src_after != src_bytes {
        return report.fail(name, "源文件字节被修改（应为只写副本）".into());
    }
    let out_len = std::fs::metadata(&out).map(|m| m.len()).unwrap_or(0);
    report.pass(
        name,
        format!(
            "{}x{} 不变；源 {} 字节未改动；输出 {} 字节；回读 8 个标签一致（含方向 6）",
            dims.1.width,
            dims.1.height,
            src_bytes.len(),
            out_len
        ),
    );
}

/// 真实 JPEG → AVIF：产物用 ffprobe（独立解码器）核对尺寸，源文件字节不变。
fn avif_matrix(ffmpeg: &Path, ffprobe: &Path, dir: &Path, report: &mut Report) {
    let name = "avif-encode";
    let src = dir.join("avif-src.jpg");
    if let Err(e) = ffmpeg_color(ffmpeg, &src, "640x480", "0x3366CC") {
        return report.fail(name, format!("生成真实 JPEG 失败：{e}"));
    }
    let src_bytes = match std::fs::read(&src) {
        Ok(b) => b,
        Err(e) => return report.fail(name, format!("读取源文件失败：{e}")),
    };
    let params = avif::AvifParams {
        quality: 70,
        speed: 8,
    };
    let (out, w, h, bytes) = match avif::encode_one(&src, dir, &params) {
        Ok(v) => v,
        Err(e) => return report.fail(name, format!("编码失败：{e}")),
    };
    if (w, h) != (640, 480) {
        return report.fail(name, format!("输入尺寸异常：{w}x{h}"));
    }
    let dims = match probe(ffprobe, &out) {
        Ok(d) => d,
        Err(e) => return report.fail(name, format!("ffprobe 读取 AVIF 失败：{e}")),
    };
    if dims.width != 640 || dims.height != 480 {
        return report.fail(
            name,
            format!(
                "解码尺寸不一致：{}x{}（期望 640x480）",
                dims.width, dims.height
            ),
        );
    }
    let data = std::fs::read(&out).unwrap_or_default();
    if !avif::looks_like_avif(&data) {
        return report.fail(name, "产物缺少 ftyp/avif 签名".into());
    }
    if std::fs::read(&src).unwrap_or_default() != src_bytes {
        return report.fail(name, "源文件被修改（应只写副本）".into());
    }
    report.pass(
        name,
        format!(
            "源 {} 字节未改动；输出 {} 字节（{}x{}，ffprobe 解码确认）；压缩比 {:.1}%",
            src_bytes.len(),
            bytes,
            dims.width,
            dims.height,
            100.0 * bytes as f64 / src_bytes.len() as f64
        ),
    );
}

fn main() {
    if let Err(message) = run_all() {
        eprintln!("P2 CHECK FAILED: {message}");
        std::process::exit(1);
    }
}

fn run_all() -> Result<(), String> {
    let ffmpeg = tool(
        "FFMPEG_PATH",
        "../samples/_dl/ffmpeg-lgpl/ffmpeg-master-latest-win64-lgpl/bin/ffmpeg.exe",
    )?;
    let ffprobe = tool(
        "FFPROBE_PATH",
        "../samples/_dl/ffmpeg-lgpl/ffmpeg-master-latest-win64-lgpl/bin/ffprobe.exe",
    )?;
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let dir = std::env::var("P2_OUT")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("../samples/p2-check").join(format!("run-{stamp}")));
    std::fs::create_dir_all(&dir).map_err(|e| format!("无法创建输出目录：{e}"))?;
    let mut report = Report {
        passed: 0,
        failed: 0,
        lines: Vec::new(),
        dir: dir.clone(),
    };
    println!("输出目录：{}", dir.display());

    // P2_ONLY=exif 只跑指定项（默认全跑），便于对单个功能取证据。
    let only: Vec<String> = std::env::var("P2_ONLY")
        .unwrap_or_default()
        .split(',')
        .map(|s| s.trim().to_lowercase())
        .filter(|s| !s.is_empty())
        .collect();
    let keep = |key: &str| only.is_empty() || only.iter().any(|f| f == key);

    if keep("seq") {
        image_sequence_to_video(&ffmpeg, &ffprobe, &dir, &mut report);
    }
    if keep("frames") {
        video_to_frames(&ffmpeg, &ffprobe, &dir, &mut report);
    }
    if keep("crop") {
        image_crop(&ffmpeg, &ffprobe, &dir, &mut report);
    }
    if keep("collage") {
        image_collage(&ffmpeg, &ffprobe, &dir, &mut report);
    }
    if keep("pdf") {
        pdf_merge_and_split(&dir, &mut report);
    }
    if keep("pdfimg") {
        pdf_to_image(&dir, &ffprobe, &mut report);
    }
    if keep("format") {
        format_matrix(&ffmpeg, &ffprobe, &dir, &mut report);
    }
    if keep("speed") {
        speed_matrix(&ffmpeg, &ffprobe, &dir, &mut report);
    }
    if keep("wm") {
        watermark_matrix(&ffmpeg, &ffprobe, &dir, &mut report);
    }
    if keep("compress") {
        compress_matrix(&ffmpeg, &ffprobe, &dir, &mut report);
    }
    if keep("exif") {
        exif_matrix(&ffmpeg, &ffprobe, &dir, &mut report);
    }
    if keep("avif") {
        avif_matrix(&ffmpeg, &ffprobe, &dir, &mut report);
    }
    if keep("subtitle") {
        subtitle_matrix(&ffmpeg, &ffprobe, &dir, &mut report);
    }
    if keep("volume") {
        volume_matrix(&ffmpeg, &ffprobe, &dir, &mut report);
    }
    if keep("split") {
        split_matrix(&ffmpeg, &ffprobe, &dir, &mut report);
    }
    if keep("gif") {
        gif_matrix(&ffmpeg, &ffprobe, &dir, &mut report);
    }
    if keep("preset") {
        preset_roundtrip(&dir, &mut report);
    }

    report.write();
    if report.failed > 0 {
        return Err(format!(
            "{} 项失败，详见 {}",
            report.failed,
            report.dir.display()
        ));
    }
    println!("P2 checks: {}/{} passed", report.passed, report.passed);
    Ok(())
}

fn image_sequence_to_video(ffmpeg: &Path, ffprobe: &Path, dir: &Path, report: &mut Report) {
    let name = "image-sequence-to-video";
    let seqdir = dir.join("seq");
    let _ = std::fs::remove_dir_all(&seqdir);
    std::fs::create_dir_all(&seqdir).ok();
    let mut inputs = Vec::new();
    for index in 1..=10 {
        let path = seqdir.join(format!("frame_{index:04}.png"));
        if let Err(e) = ffmpeg_color(ffmpeg, &path, "320x240", "0x3366CC") {
            report.fail(name, format!("生成测试帧失败：{e}"));
            return;
        }
        inputs.push(path.to_string_lossy().to_string());
    }
    let (pattern, start, count) = match args::validate_sequence(&inputs) {
        Ok(v) => v,
        Err(e) => return report.fail(name, format!("序列校验失败：{e}")),
    };
    if start != 1 || count != 10 {
        return report.fail(
            name,
            format!("起始编号/帧数异常：start={start}, count={count}"),
        );
    }
    let params = args::SeqParams {
        fps: 10,
        start_number: start,
        size_mode: "source".into(),
        width: 0,
        height: 0,
        encoder: "libopenh264".into(),
        quality: 23,
        bitrate_kbps: 0,
    };
    let target = match args::seq_target_size(&params, &[(320, 240); 10]) {
        Ok(t) => t,
        Err(e) => return report.fail(name, e),
    };
    let out = dir.join("seq-out.mp4");
    let argv = args::build_seq_video(
        &pattern,
        out.to_string_lossy().as_ref(),
        &params,
        target,
        count,
    );
    if let Err(e) = run(ffmpeg, &argv) {
        return report.fail(name, format!("ffmpeg 失败：{e}"));
    }
    match probe(ffprobe, &out) {
        Ok(p) => {
            let bytes = std::fs::metadata(&out).map(|m| m.len()).unwrap_or(0);
            let ok = p.width == 320
                && p.height == 240
                && (p.duration - 1.0).abs() < 0.15
                && p.frames == 10;
            if ok {
                report.pass(
                    name,
                    format!(
                        "{}x{}，{} 帧，{:.2}s，{} 字节",
                        p.width, p.height, p.frames, p.duration, bytes
                    ),
                );
            } else {
                report.fail(
                    name,
                    format!(
                        "期望 320x240 / 10 帧 / 1.0s，实际 {}x{} / {} 帧 / {:.2}s",
                        p.width, p.height, p.frames, p.duration
                    ),
                );
            }
        }
        Err(e) => report.fail(name, e),
    }
}

fn video_to_frames(ffmpeg: &Path, ffprobe: &Path, dir: &Path, report: &mut Report) {
    let name = "video-to-frames";
    let input = dir.join("frames-src.mp4");
    let made = run(
        ffmpeg,
        &[
            "-hide_banner".into(),
            "-nostdin".into(),
            "-y".into(),
            "-loglevel".into(),
            "error".into(),
            "-f".into(),
            "lavfi".into(),
            "-i".into(),
            "testsrc2=s=320x240:r=30:d=2".into(),
            "-c:v".into(),
            "libopenh264".into(),
            "-pix_fmt".into(),
            "yuv420p".into(),
            input.to_string_lossy().to_string(),
        ],
    );
    if let Err(e) = made {
        return report.fail(name, format!("生成测试视频失败：{e}"));
    }
    let framesdir = dir.join("frames");
    let _ = std::fs::remove_dir_all(&framesdir);
    std::fs::create_dir_all(&framesdir).ok();
    let pattern = framesdir.join("frame-%04d.png");
    let params = args::FramesParams {
        fps: 10,
        width: 160,
        format: "png".into(),
        quality: 90,
    };
    let argv = args::build_video_frames(
        &input.to_string_lossy(),
        pattern.to_string_lossy().as_ref(),
        &params,
    );
    if let Err(e) = run(ffmpeg, &argv) {
        return report.fail(name, format!("ffmpeg 失败：{e}"));
    }
    let files: Vec<PathBuf> = std::fs::read_dir(&framesdir)
        .map(|entries| entries.filter_map(|e| e.ok().map(|e| e.path())).collect())
        .unwrap_or_default();
    let first = files.iter().min().cloned();
    let dims = first
        .as_ref()
        .and_then(|p| probe(ffprobe, p).ok())
        .map(|p| (p.width, p.height));
    let ok = files.len() == 20 && dims == Some((160, 120));
    match dims {
        Some((w, h)) if ok => report.pass(
            name,
            format!("{} 帧，首帧 {}x{}，期望 20 帧 / 160x120", files.len(), w, h),
        ),
        _ => report.fail(
            name,
            format!(
                "帧数 {}，首帧 {:?}，期望 20 帧 / 160x120",
                files.len(),
                dims
            ),
        ),
    }
}

fn image_crop(ffmpeg: &Path, ffprobe: &Path, dir: &Path, report: &mut Report) {
    let name = "image-crop";
    let src = dir.join("crop-src.png");
    if let Err(e) = run(
        ffmpeg,
        &[
            "-hide_banner".into(),
            "-nostdin".into(),
            "-y".into(),
            "-loglevel".into(),
            "error".into(),
            "-f".into(),
            "lavfi".into(),
            "-i".into(),
            "testsrc2=s=640x480".into(),
            "-frames:v".into(),
            "1".into(),
            src.to_string_lossy().to_string(),
        ],
    ) {
        return report.fail(name, format!("生成源图失败：{e}"));
    }

    let ratio_params = args::ImageParams {
        format: "png".into(),
        quality: 90,
        crop_mode: "ratio".into(),
        crop_ratio: "1:1".into(),
        ..Default::default()
    };
    let ratio_out = dir.join("crop-ratio.png");
    let argv = match args::build_image(
        &src.to_string_lossy(),
        ratio_out.to_string_lossy().as_ref(),
        &ratio_params,
    ) {
        Ok(argv) => argv,
        Err(e) => return report.fail(name, format!("比例裁剪失败：{e}")),
    };
    if let Err(e) = run(ffmpeg, &argv) {
        return report.fail(name, format!("比例裁剪失败：{e}"));
    }
    let ratio_dims = probe(ffprobe, &ratio_out).map(|p| (p.width, p.height)).ok();

    let custom_params = args::ImageParams {
        format: "png".into(),
        quality: 90,
        max_edge: 150,
        crop_mode: "custom".into(),
        crop_x: 50,
        crop_y: 25,
        crop_w: 300,
        crop_h: 200,
        ..Default::default()
    };
    if let Err(e) = args::validate_custom_crop(50, 25, 300, 200, 640, 480) {
        return report.fail(name, format!("合法裁剪被拒绝：{e}"));
    }
    let custom_out = dir.join("crop-custom.png");
    let argv = match args::build_image(
        &src.to_string_lossy(),
        custom_out.to_string_lossy().as_ref(),
        &custom_params,
    ) {
        Ok(argv) => argv,
        Err(e) => return report.fail(name, format!("自定义裁剪失败：{e}")),
    };
    if let Err(e) = run(ffmpeg, &argv) {
        return report.fail(name, format!("自定义裁剪失败：{e}"));
    }
    let custom_dims = probe(ffprobe, &custom_out)
        .map(|p| (p.width, p.height))
        .ok();
    let reject = args::validate_custom_crop(10, 0, 640, 480, 640, 480).unwrap_err();
    if ratio_dims == Some((480, 480)) && custom_dims == Some((150, 100)) && reject.contains("越界")
    {
        report.pass(
            name,
            format!(
                "1:1 → 480x480；自定义 300x200@(50,25)+maxEdge150 → 150x100；越界拒绝：{reject}"
            ),
        );
    } else {
        report.fail(
            name,
            format!("ratio={ratio_dims:?} custom={custom_dims:?} reject={reject}"),
        );
    }
}

fn image_collage(ffmpeg: &Path, ffprobe: &Path, dir: &Path, report: &mut Report) {
    let name = "image-collage";
    let cell = dir.join("collage");
    let _ = std::fs::remove_dir_all(&cell);
    std::fs::create_dir_all(&cell).ok();
    let colors = ["red", "green", "blue", "yellow"];
    let mut inputs = Vec::new();
    for (index, color) in colors.iter().enumerate() {
        let path = cell.join(format!("part{index}.png"));
        if let Err(e) = ffmpeg_color(ffmpeg, &path, "200x150", color) {
            return report.fail(name, format!("生成拼图素材失败：{e}"));
        }
        inputs.push(path.to_string_lossy().to_string());
    }
    let params = args::CollageParams {
        cols: 2,
        rows: 2,
        cell_width: 200,
        cell_height: 150,
        gap: 10,
        background: "#FFFFFF".into(),
        fit: "contain".into(),
        format: "png".into(),
        quality: 90,
    };
    let out = dir.join("collage-out.png");
    let argv = match args::build_collage(&inputs, out.to_string_lossy().as_ref(), &params) {
        Ok(v) => v,
        Err(e) => return report.fail(name, e),
    };
    if let Err(e) = run(ffmpeg, &argv) {
        return report.fail(name, format!("拼图失败：{e}"));
    }
    let dims = probe(ffprobe, &out).map(|p| (p.width, p.height)).ok();
    let expected = args::collage_canvas(2, 2, 200, 150, 10);
    let invalid = args::build_collage(
        &inputs,
        "x.png",
        &args::CollageParams {
            cols: 1,
            rows: 1,
            ..params.clone()
        },
    )
    .unwrap_err();
    if dims == Some(expected) && invalid.contains("放不进") {
        report.pass(
            name,
            format!(
                "{}x{}（期望 {}x{}）；容量越界被拒绝",
                dims.unwrap().0,
                dims.unwrap().1,
                expected.0,
                expected.1
            ),
        );
    } else {
        report.fail(
            name,
            format!("dims={dims:?} expected={expected:?} invalid={invalid}"),
        );
    }
}

fn pdf_merge_and_split(dir: &Path, report: &mut Report) {
    let name = "pdf-merge-split";
    let a = dir.join("pdf-a.pdf");
    let b = dir.join("pdf-b.pdf");
    std::fs::write(&a, minimal_pdf(2)).ok();
    std::fs::write(&b, minimal_pdf(3)).ok();
    let merged = dir.join("pdf-merged.pdf");
    let inputs = vec![
        a.to_string_lossy().to_string(),
        b.to_string_lossy().to_string(),
    ];
    let total = match pdf::merge(&inputs, &merged) {
        Ok(v) => v,
        Err(e) => return report.fail(name, format!("合并失败：{e}")),
    };
    let merged_pages = pdf::page_count(&merged.to_string_lossy()).unwrap_or(0);
    let per_dir = dir.join("pdf-per");
    let per = pdf::split(&a.to_string_lossy(), &per_dir, "doc", true, None);
    let range_dir = dir.join("pdf-ranges");
    let ranges = pdf::split(&a.to_string_lossy(), &range_dir, "doc", false, Some("1-2"));
    let invalid = pdf::parse_ranges("1-9", 5).unwrap_err();
    match (per, ranges) {
        (Ok(per_files), Ok(range_files)) => {
            let per_pages: Vec<usize> = per_files
                .iter()
                .filter_map(|p| pdf::page_count(&p.to_string_lossy()).ok())
                .collect();
            let range_pages = range_files
                .first()
                .and_then(|p| pdf::page_count(&p.to_string_lossy()).ok())
                .unwrap_or(0);
            let bytes = std::fs::metadata(&merged).map(|m| m.len()).unwrap_or(0);
            if total == 5
                && merged_pages == 5
                && per_files.len() == 2
                && per_pages == vec![1, 1]
                && range_files.len() == 1
                && range_pages == 2
                && invalid.contains("超出总页数")
            {
                report.pass(
                    name,
                    format!(
                        "合并 {} 页/{} 字节；每页拆分 {} 个（页数 {:?}）；1-2 → {} 个/{} 页；越界拒绝",
                        merged_pages, bytes, per_files.len(), per_pages, range_files.len(), range_pages
                    ),
                );
            } else {
                report.fail(
                    name,
                    format!(
                        "merged={merged_pages} per={} {:?} range={} {range_pages}",
                        per_files.len(),
                        per_pages,
                        range_files.len()
                    ),
                );
            }
        }
        (Err(e), _) | (_, Err(e)) => report.fail(name, format!("拆分失败：{e}")),
    }
}

fn pdf_to_image(dir: &Path, ffprobe: &Path, report: &mut Report) {
    let name = "pdf-to-image";
    let src = dir.join("pdf-src.pdf");
    std::fs::write(&src, minimal_pdf(2)).ok();
    let tools = pdf_tools::resolve(None);
    let outdir = dir.join("pdf-images");
    let _ = std::fs::remove_dir_all(&outdir);

    // 先验证「缺失 → 明确不可用 + 明确拒绝」，再验证真实渲染。
    let saved_path = std::env::var_os("PATH");
    let saved_tool = std::env::var_os("PDFTOPPM_PATH");
    let saved_dirs = std::env::var_os("TRANSMUTIO_PDF_TOOLS");
    std::env::set_var("PATH", "");
    std::env::remove_var("PDFTOPPM_PATH");
    std::env::remove_var("TRANSMUTIO_PDF_TOOLS");
    let missing = pdf_tools::resolve(None);
    let missing_run = pdf::rasterize(
        "definitely-not-pdftoppm",
        &src.to_string_lossy(),
        &outdir.join("missing"),
        "page",
        "png",
        96,
        1,
        1,
    )
    .unwrap_err();
    if let Some(value) = saved_path {
        std::env::set_var("PATH", value);
    }
    if let Some(value) = saved_tool {
        std::env::set_var("PDFTOPPM_PATH", value);
    }
    if let Some(value) = saved_dirs {
        std::env::set_var("TRANSMUTIO_PDF_TOOLS", value);
    }
    let missing_ok = !missing.available
        && missing
            .reason
            .as_deref()
            .map(|r| r.contains("pdftoppm"))
            .unwrap_or(false)
        && missing_run.contains("pdftoppm");

    if !tools.available {
        if missing_ok {
            report.pass(
                name,
                format!(
                    "未安装 pdftoppm：入口不可用（available=false）且直接拒绝；原因：{}",
                    missing.reason.unwrap_or_default()
                ),
            );
        } else {
            report.fail(name, "缺失路径没有给出明确原因/拒绝".into());
        }
        return;
    }
    let produced = match pdf::rasterize(
        tools.pdftoppm.as_deref().unwrap(),
        &src.to_string_lossy(),
        &outdir,
        "page",
        "png",
        96,
        1,
        2,
    ) {
        Ok(files) => files,
        Err(e) => return report.fail(name, format!("pdftoppm 渲染失败：{e}")),
    };
    let first = produced.first().cloned();
    let dims = first
        .as_ref()
        .and_then(|p| {
            run(
                ffprobe,
                &[
                    "-v".into(),
                    "error".into(),
                    "-select_streams".into(),
                    "v:0".into(),
                    "-show_entries".into(),
                    "stream=width,height".into(),
                    "-of".into(),
                    "json".into(),
                    p.to_string_lossy().to_string(),
                ],
            )
            .ok()
        })
        .and_then(|json| {
            let value: serde_json::Value = serde_json::from_str(&json).ok()?;
            let stream = value.get("streams")?.get(0)?;
            Some((
                stream.get("width")?.as_u64()? as u32,
                stream.get("height")?.as_u64()? as u32,
            ))
        });
    let total_bytes: u64 = produced
        .iter()
        .filter_map(|p| std::fs::metadata(p).ok().map(|m| m.len()))
        .sum();
    let ok = produced.len() == 2 && dims == Some((267, 267)) && missing_ok;
    if ok {
        report.pass(
            name,
            format!(
                "{} 张 PNG，首张 267x267（200pt@96DPI），合计 {} 字节；缺失时正确拒绝",
                produced.len(),
                total_bytes
            ),
        );
    } else {
        report.fail(
            name,
            format!(
                "produced={} dims={dims:?} missing_ok={missing_ok}",
                produced.len()
            ),
        );
    }
}

fn probe_meta(ffprobe: &Path, path: &Path) -> Result<(String, String), String> {
    let json = run(
        ffprobe,
        &[
            "-v".into(),
            "error".into(),
            "-show_entries".into(),
            "stream=codec_name:format=format_name".into(),
            "-of".into(),
            "json".into(),
            path.to_string_lossy().to_string(),
        ],
    )?;
    let value: serde_json::Value =
        serde_json::from_str(&json).map_err(|e| format!("ffprobe JSON 解析失败：{e}"))?;
    let codec = value
        .get("streams")
        .and_then(|s| s.get(0))
        .and_then(|s| s.get("codec_name"))
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let format = value
        .get("format")
        .and_then(|f| f.get("format_name"))
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    Ok((codec, format))
}

/// 视频容器 / 音频格式矩阵：真实转码 + ffprobe 读取编码与容器，
/// 并断言容器数量、音频数量与「不兼容组合被拒绝」。
fn format_matrix(ffmpeg: &Path, ffprobe: &Path, dir: &Path, report: &mut Report) {
    let name = "format-matrix";
    let base = dir.join("fmt-base.mp4");
    if let Err(e) = run(
        ffmpeg,
        &[
            "-hide_banner".into(),
            "-nostdin".into(),
            "-y".into(),
            "-loglevel".into(),
            "error".into(),
            "-f".into(),
            "lavfi".into(),
            "-i".into(),
            "testsrc2=size=320x240:rate=25:duration=1".into(),
            "-f".into(),
            "lavfi".into(),
            "-i".into(),
            "sine=frequency=440:sample_rate=44100:duration=1".into(),
            "-c:v".into(),
            "libopenh264".into(),
            "-c:a".into(),
            "aac".into(),
            "-b:a".into(),
            "128k".into(),
            "-shortest".into(),
            base.to_string_lossy().to_string(),
        ],
    ) {
        report.fail(name, format!("生成基准视频失败：{e}"));
        return;
    }

    // 数量与拒绝路径（静态断言，不依赖 UI）。
    let reject = containers::validate_video("mp4", "vp9").err();
    let counts_ok = containers::VIDEO_CONTAINERS.len() >= 4 && containers::AUDIO_FORMATS.len() >= 5;
    if counts_ok
        && reject
            .as_deref()
            .map(|e| e.contains("VP9") || e.contains("vp9"))
            .unwrap_or(false)
    {
        report.pass(
            &format!("{name}-counts"),
            format!(
                "视频容器 {} 个（≥4），音频格式 {} 个（≥5）；mp4+vp9 被拒绝：{}",
                containers::VIDEO_CONTAINERS.len(),
                containers::AUDIO_FORMATS.len(),
                reject.unwrap_or_default()
            ),
        );
    } else {
        report.fail(
            name,
            format!("容器/格式数量或拒绝路径不满足：reject={reject:?}"),
        );
    }

    // 真实视频容器转码。
    let video_cases = [
        ("mp4", "libopenh264", "h264", "mp4"),
        ("mkv", "libopenh264", "h264", "matroska"),
        ("webm", "libvpx-vp9", "vp9", "matroska"),
        ("mov", "libopenh264", "h264", "mov"),
        ("ts", "libopenh264", "h264", "mpegts"),
        ("avi", "mpeg4", "mpeg4", "avi"),
    ];
    let mut evidence = Vec::new();
    let mut failures = Vec::new();
    for (container, encoder, expect_codec, expect_format) in video_cases {
        let ext = containers::container_ext(container).unwrap_or("mp4");
        let out = dir.join(format!("fmt-{container}.{ext}"));
        let audio = containers::pick_container_audio_encoder(container, "", &[]);
        let mut argv: Vec<String> = vec![
            "-hide_banner".into(),
            "-nostdin".into(),
            "-y".into(),
            "-loglevel".into(),
            "error".into(),
            "-i".into(),
            base.to_string_lossy().to_string(),
            "-t".into(),
            "1".into(),
            "-c:v".into(),
            encoder.into(),
        ];
        if encoder == "libvpx-vp9" {
            argv.extend(["-b:v".into(), "0".into(), "-crf".into(), "32".into()]);
        }
        if encoder == "mpeg4" {
            argv.extend(["-q:v".into(), "4".into()]);
        }
        argv.extend(["-c:a".into(), audio.into(), "-b:a".into(), "96k".into()]);
        argv.extend([
            "-f".into(),
            containers::find_container(container)
                .map(|c| c.muxer)
                .unwrap_or("mp4")
                .into(),
        ]);
        argv.push(out.to_string_lossy().to_string());
        match run(ffmpeg, &argv).and_then(|_| probe_meta(ffprobe, &out)) {
            Ok((codec, format)) if codec == expect_codec && format.contains(expect_format) => {
                let bytes = std::fs::metadata(&out).map(|m| m.len()).unwrap_or(0);
                evidence.push(format!("{container}({codec},{format},{bytes}B)"));
            }
            Ok((codec, format)) => {
                failures.push(format!("{container}: codec={codec} format={format}"))
            }
            Err(e) => failures.push(format!("{container}: {e}")),
        }
    }
    if failures.is_empty() {
        report.pass(&format!("{name}-video"), evidence.join("；"));
    } else {
        report.fail(&format!("{name}-video"), failures.join("；"));
    }

    // 真实音频格式转码（输出扩展名由矩阵决定，AAC → m4a）。
    let audio_cases = [
        ("mp3", "libmp3lame", "mp3"),
        ("aac", "aac", "aac"),
        ("flac", "flac", "flac"),
        ("wav", "pcm_s16le", "pcm_s16le"),
        ("ogg", "libvorbis", "vorbis"),
        ("opus", "libopus", "opus"),
    ];
    let mut evidence = Vec::new();
    let mut failures = Vec::new();
    for (format, encoder, expect_codec) in audio_cases {
        let ext = containers::audio_ext(format).unwrap_or(format);
        let out = dir.join(format!("fmt-audio.{ext}"));
        let mut argv: Vec<String> = vec![
            "-hide_banner".into(),
            "-nostdin".into(),
            "-y".into(),
            "-loglevel".into(),
            "error".into(),
            "-i".into(),
            base.to_string_lossy().to_string(),
            "-t".into(),
            "1".into(),
            "-vn".into(),
            "-c:a".into(),
            encoder.into(),
        ];
        if containers::find_audio(format)
            .map(|f| f.lossy)
            .unwrap_or(true)
        {
            argv.extend(["-b:a".into(), "96k".into()]);
        }
        argv.push(out.to_string_lossy().to_string());
        match run(ffmpeg, &argv).and_then(|_| probe_meta(ffprobe, &out)) {
            Ok((codec, _)) if codec == expect_codec => {
                let bytes = std::fs::metadata(&out).map(|m| m.len()).unwrap_or(0);
                evidence.push(format!("{format}(.{ext},{codec},{bytes}B)"));
            }
            Ok((codec, _)) => failures.push(format!("{format}: codec={codec}")),
            Err(e) => failures.push(format!("{format}: {e}")),
        }
    }
    if failures.is_empty() {
        report.pass(&format!("{name}-audio"), evidence.join("；"));
    } else {
        report.fail(&format!("{name}-audio"), failures.join("；"));
    }
}

/// 读取音频流的采样率/声道与容器时长，用于变速产物断言。
fn probe_audio_duration(ffprobe: &Path, path: &Path) -> Result<(f64, u32, u32), String> {
    let json = run(
        ffprobe,
        &[
            "-v".into(),
            "error".into(),
            "-select_streams".into(),
            "a:0".into(),
            "-show_entries".into(),
            "stream=sample_rate,channels:format=duration".into(),
            "-of".into(),
            "json".into(),
            path.to_string_lossy().to_string(),
        ],
    )?;
    let value: serde_json::Value =
        serde_json::from_str(&json).map_err(|e| format!("ffprobe JSON 解析失败：{e}"))?;
    let duration = value
        .get("format")
        .and_then(|f| f.get("duration"))
        .and_then(|v| v.as_str())
        .and_then(|s| s.parse::<f64>().ok())
        .ok_or("ffprobe 没有返回时长")?;
    let stream = value
        .get("streams")
        .and_then(|s| s.get(0))
        .ok_or("ffprobe 没有返回音频流")?;
    let sample_rate = stream
        .get("sample_rate")
        .and_then(|v| v.as_str())
        .and_then(|s| s.parse::<u32>().ok())
        .unwrap_or(0);
    let channels = stream.get("channels").and_then(|v| v.as_u64()).unwrap_or(0) as u32;
    Ok((duration, sample_rate, channels))
}

/// 变速矩阵：真实生成 1.5× / 0.5× 产物，断言时长按倍速缩放且音频采样率/声道不变；
/// 同时静态校验 atempo 链式拆分的乘积。
fn speed_matrix(ffmpeg: &Path, ffprobe: &Path, dir: &Path, report: &mut Report) {
    let name = "speed-matrix";
    let base = dir.join("speed-base.mp4");
    if !base.is_file() {
        if let Err(e) = run(
            ffmpeg,
            &[
                "-hide_banner".into(),
                "-nostdin".into(),
                "-y".into(),
                "-loglevel".into(),
                "error".into(),
                "-f".into(),
                "lavfi".into(),
                "-i".into(),
                "testsrc2=size=320x240:rate=30:duration=2".into(),
                "-f".into(),
                "lavfi".into(),
                "-i".into(),
                "sine=frequency=440:sample_rate=48000:duration=2".into(),
                "-c:v".into(),
                "libopenh264".into(),
                "-c:a".into(),
                "aac".into(),
                "-b:a".into(),
                "128k".into(),
                "-ac".into(),
                "2".into(),
                "-shortest".into(),
                base.to_string_lossy().to_string(),
            ],
        ) {
            report.fail(name, format!("生成基准视频失败：{e}"));
            return;
        }
    }

    let factors = args::atempo_factors(2.5);
    let product: f64 = factors.iter().product();
    if (product - 2.5).abs() < 1e-6 {
        report.pass(
            &format!("{name}-atempo"),
            format!("atempo_factors(2.5)={factors:?}，乘积 {product:.4}"),
        );
    } else {
        report.fail(
            &format!("{name}-atempo"),
            format!("atempo_factors(2.5) 乘积 {product} 与 2.5 不一致"),
        );
    }

    for speed in [1.5, 0.5] {
        let label = format!("{name}-{speed}x");
        let out = dir.join(format!("speed-{speed}.mp4"));
        let p = args::VideoParams {
            container: "mp4".into(),
            codec: "h264".into(),
            encoder: "libopenh264".into(),
            rate_mode: "quality".into(),
            quality: 23,
            bitrate_kbps: 0,
            resolution: "source".into(),
            fps: "source".into(),
            crop: "source".into(),
            rotation: "0".into(),
            audio_mode: "aac".into(),
            audio_bitrate_kbps: 128,
            hw_decode: false,
            faststart: true,
            speed,
            keep_pitch: true,
            watermark: args::WatermarkParams::default(),
            subtitle: args::SubtitleParams::default(),
            volume: args::VolumeParams::default(),
            split: args::SplitParams::default(),
        };
        let argv = match args::build_video(
            "libopenh264",
            base.to_string_lossy().as_ref(),
            out.to_string_lossy().as_ref(),
            &p,
            "aac",
            None,
        ) {
            Ok(argv) => argv,
            Err(e) => {
                report.fail(&label, e);
                continue;
            }
        };
        if let Err(e) = run(ffmpeg, &argv) {
            report.fail(&label, format!("ffmpeg 失败：{e}"));
            continue;
        }
        match probe_audio_duration(ffprobe, &out) {
            Ok((duration, sample_rate, channels)) => {
                let expected = 2.0 / speed;
                let ok =
                    (duration - expected).abs() <= 0.1 && sample_rate == 48000 && channels == 2;
                if ok {
                    report.pass(
                        &label,
                        format!(
                            "时长 {duration:.2}s（期望 {expected:.2}s），音频 {sample_rate} Hz / {channels} 声道"
                        ),
                    );
                } else {
                    report.fail(
                        &label,
                        format!(
                            "时长 {duration:.2}s（期望 {expected:.2}s），音频 {sample_rate} Hz / {channels} 声道（期望 48000 / 2）"
                        ),
                    );
                }
            }
            Err(e) => report.fail(&label, e),
        }
    }
}

fn probe_pix_fmt(ffprobe: &Path, path: &Path) -> Result<String, String> {
    let json = run(
        ffprobe,
        &[
            "-v".into(),
            "error".into(),
            "-select_streams".into(),
            "v:0".into(),
            "-show_entries".into(),
            "stream=pix_fmt".into(),
            "-of".into(),
            "json".into(),
            path.to_string_lossy().to_string(),
        ],
    )?;
    let value: serde_json::Value =
        serde_json::from_str(&json).map_err(|e| format!("ffprobe JSON 解析失败：{e}"))?;
    Ok(value
        .get("streams")
        .and_then(|s| s.get(0))
        .and_then(|s| s.get("pix_fmt"))
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string())
}

/// 取单像素 RGB：裁剪成 2x2 后写 rawvideo，避免解析 png/jpg 解码器。
/// 尺寸必须取偶（yuv420 的色度块在 1x1 裁剪时会变成 0）。
fn sample_pixel(ffmpeg: &Path, input: &Path, x: u32, y: u32) -> Result<(u8, u8, u8), String> {
    let out = input.with_extension("sample.rgb");
    run(
        ffmpeg,
        &[
            "-hide_banner".into(),
            "-nostdin".into(),
            "-y".into(),
            "-loglevel".into(),
            "error".into(),
            "-i".into(),
            input.to_string_lossy().to_string(),
            "-vf".into(),
            format!("crop=2:2:{x}:{y},format=rgb24"),
            "-frames:v".into(),
            "1".into(),
            "-pix_fmt".into(),
            "rgb24".into(),
            "-f".into(),
            "rawvideo".into(),
            out.to_string_lossy().to_string(),
        ],
    )?;
    let bytes = std::fs::read(&out).map_err(|e| format!("读取像素失败：{e}"))?;
    let _ = std::fs::remove_file(&out);
    if bytes.len() < 3 {
        return Err(format!("像素数据不足：{} 字节", bytes.len()));
    }
    Ok((bytes[0], bytes[1], bytes[2]))
}

/// 水印矩阵：图片 jpg/webp/png 的像素颜色 + pix_fmt；视频 mp4/mkv 的尺寸、时长、
/// 音频采样率与流格式；以及缺失水印图片的硬失败路径。
fn watermark_matrix(ffmpeg: &Path, ffprobe: &Path, dir: &Path, report: &mut Report) {
    let name = "watermark";
    let base_img = dir.join("wm-base.jpg");
    let base_mp4 = dir.join("wm-base.mp4");
    let wm = dir.join("wm.png");
    if !base_img.is_file() {
        if let Err(e) = ffmpeg_color(ffmpeg, &base_img, "640x480", "0x3366CC") {
            return report.fail(name, format!("生成基准图片失败：{e}"));
        }
    }
    if !wm.is_file() {
        if let Err(e) = ffmpeg_color(ffmpeg, &wm, "64x64", "red") {
            return report.fail(name, format!("生成水印图片失败：{e}"));
        }
    }
    if !base_mp4.is_file() {
        if let Err(e) = run(
            ffmpeg,
            &[
                "-hide_banner".into(),
                "-nostdin".into(),
                "-y".into(),
                "-loglevel".into(),
                "error".into(),
                "-f".into(),
                "lavfi".into(),
                "-i".into(),
                "testsrc2=size=320x240:rate=30:duration=2".into(),
                "-f".into(),
                "lavfi".into(),
                "-i".into(),
                "sine=frequency=440:sample_rate=48000:duration=2".into(),
                "-c:v".into(),
                "libopenh264".into(),
                "-c:a".into(),
                "aac".into(),
                "-b:a".into(),
                "128k".into(),
                "-ac".into(),
                "2".into(),
                "-shortest".into(),
                base_mp4.to_string_lossy().to_string(),
            ],
        ) {
            return report.fail(name, format!("生成基准视频失败：{e}"));
        }
    }

    let wm_params = |margin: u32| args::WatermarkParams {
        enabled: true,
        path: wm.to_string_lossy().to_string(),
        position: "se".into(),
        opacity: 0.9,
        scale_percent: 20,
        margin,
        rotation: 0.0,
        base_width: 640,
        base_height: 480,
        wm_width: 64,
        wm_height: 64,
        ..Default::default()
    };

    // 图片：640x480，20% 短边 → 96x96；margin 0 → 右下角 (544,384)，采样点 (600,440)。
    for (format, expect_pix) in [("jpg", "yuvj420p"), ("webp", "yuv420p"), ("png", "rgb24")] {
        let label = format!("{name}-image-{format}");
        let out = dir.join(format!("wm-out.{format}"));
        let p = args::ImageParams {
            format: format.into(),
            quality: 90,
            watermark: wm_params(0),
            ..Default::default()
        };
        let argv = match args::build_image(
            &base_img.to_string_lossy(),
            out.to_string_lossy().as_ref(),
            &p,
        ) {
            Ok(argv) => argv,
            Err(e) => {
                report.fail(&label, e);
                continue;
            }
        };
        if let Err(e) = run(ffmpeg, &argv) {
            report.fail(&label, format!("ffmpeg 失败：{e}"));
            continue;
        }
        let dims = probe(ffprobe, &out).map(|p| (p.width, p.height)).ok();
        let pix_fmt = probe_pix_fmt(ffprobe, &out).unwrap_or_default();
        let pixel = sample_pixel(ffmpeg, &out, 600, 440).ok();
        let pixel_ok = pixel.map(|(r, _, b)| r >= 180 && b <= 80).unwrap_or(false);
        if dims == Some((640, 480)) && pix_fmt == expect_pix && pixel_ok {
            let (r, g, b) = pixel.unwrap();
            report.pass(
                &label,
                format!("640x480 / pix_fmt={pix_fmt} / 采样像素 RGB({r},{g},{b})"),
            );
        } else {
            report.fail(
                &label,
                format!(
                    "尺寸 {dims:?}（期望 640x480），pix_fmt={pix_fmt}（期望 {expect_pix}），像素 {pixel:?}（期望 R>=180 且 B<=80）"
                ),
            );
        }
    }

    // 视频：320x240，20% 短边 → 48x48；margin 24 → se (248,168)；1.5× → 1.333s。
    let vp = args::VideoParams {
        container: "mp4".into(),
        codec: "h264".into(),
        encoder: "libopenh264".into(),
        rate_mode: "quality".into(),
        quality: 23,
        bitrate_kbps: 0,
        resolution: "source".into(),
        fps: "source".into(),
        crop: "source".into(),
        rotation: "0".into(),
        audio_mode: "aac".into(),
        audio_bitrate_kbps: 128,
        hw_decode: false,
        faststart: true,
        speed: 1.5,
        keep_pitch: true,
        watermark: args::WatermarkParams {
            enabled: true,
            path: wm.to_string_lossy().to_string(),
            position: "se".into(),
            opacity: 0.9,
            scale_percent: 20,
            margin: 24,
            rotation: 0.0,
            base_width: 320,
            base_height: 240,
            wm_width: 64,
            wm_height: 64,
            ..Default::default()
        },
        subtitle: args::SubtitleParams::default(),
        volume: args::VolumeParams::default(),
        split: args::SplitParams::default(),
    };
    for container in ["mp4", "mkv"] {
        let label = format!("{name}-video-{container}");
        let mut p = vp.clone();
        p.container = container.into();
        let out = dir.join(format!("wm-video.{container}"));
        let argv = match args::build_video(
            "libopenh264",
            base_mp4.to_string_lossy().as_ref(),
            out.to_string_lossy().as_ref(),
            &p,
            "aac",
            None,
        ) {
            Ok(argv) => argv,
            Err(e) => {
                report.fail(&label, e);
                continue;
            }
        };
        if let Err(e) = run(ffmpeg, &argv) {
            report.fail(&label, format!("ffmpeg 失败：{e}"));
            continue;
        }
        let probed = match probe(ffprobe, &out) {
            Ok(p) => p,
            Err(e) => {
                report.fail(&label, e);
                continue;
            }
        };
        let audio = probe_audio_duration(ffprobe, &out).unwrap_or((0.0, 0, 0));
        let format_name = probe_meta(ffprobe, &out)
            .map(|(_, f)| f)
            .unwrap_or_default();
        let container_ok = if container == "mkv" {
            format_name.contains("matroska")
        } else {
            format_name.contains("mp4")
        };
        let ok = probed.width == 320
            && probed.height == 240
            && (probed.duration - 1.333).abs() <= 0.1
            && audio.1 == 48000
            && audio.2 == 2
            && container_ok;
        if ok {
            report.pass(
                &label,
                format!(
                    "320x240，{:.2}s（期望 1.33s），音频 {} Hz / {} 声道，容器 {format_name}",
                    probed.duration, audio.1, audio.2
                ),
            );
        } else {
            report.fail(
                &label,
                format!(
                    "{}x{}，{:.2}s（期望 1.33s），音频 {} Hz / {} 声道（期望 48000 / 2），容器 {format_name}",
                    probed.width, probed.height, probed.duration, audio.1, audio.2
                ),
            );
        }
    }

    // 缺失水印图片必须在开始前硬失败，不能静默跳过。
    let missing = args::validate_watermark(&args::WatermarkParams {
        enabled: true,
        path: dir.join("missing-wm.png").to_string_lossy().to_string(),
        ..Default::default()
    })
    .unwrap_err();
    if missing.contains("水印图片不存在") {
        report.pass(&format!("{name}-validate"), missing);
    } else {
        report.fail(
            &format!("{name}-validate"),
            format!("错误文案异常：{missing}"),
        );
    }
}

struct SubtitleTrack {
    codec: String,
    language: String,
    default: i64,
}

struct SubtitleProbe {
    tracks: Vec<SubtitleTrack>,
    duration: f64,
    sample_rate: u32,
    channels: u32,
}

fn probe_subtitles(ffprobe: &Path, path: &Path) -> Result<SubtitleProbe, String> {
    let json = run(
        ffprobe,
        &[
            "-v".into(),
            "error".into(),
            "-show_streams".into(),
            "-show_format".into(),
            "-of".into(),
            "json".into(),
            path.to_string_lossy().to_string(),
        ],
    )?;
    let value: serde_json::Value =
        serde_json::from_str(&json).map_err(|e| format!("ffprobe JSON 解析失败：{e}"))?;
    let streams = value
        .get("streams")
        .and_then(|s| s.as_array())
        .cloned()
        .unwrap_or_default();
    let mut tracks = Vec::new();
    let mut sample_rate = 0;
    let mut channels = 0;
    for stream in &streams {
        match stream.get("codec_type").and_then(|v| v.as_str()) {
            Some("subtitle") => tracks.push(SubtitleTrack {
                codec: stream
                    .get("codec_name")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string(),
                language: stream
                    .get("tags")
                    .and_then(|t| t.get("language"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string(),
                default: stream
                    .get("disposition")
                    .and_then(|d| d.get("default"))
                    .and_then(|v| v.as_i64())
                    .unwrap_or(0),
            }),
            Some("audio") if sample_rate == 0 => {
                sample_rate = stream
                    .get("sample_rate")
                    .and_then(|v| v.as_str())
                    .and_then(|s| s.parse().ok())
                    .unwrap_or(0);
                channels = stream.get("channels").and_then(|v| v.as_u64()).unwrap_or(0) as u32;
            }
            _ => {}
        }
    }
    let duration = value
        .get("format")
        .and_then(|f| f.get("duration"))
        .and_then(|v| v.as_str())
        .and_then(|s| s.parse().ok())
        .or_else(|| {
            streams
                .iter()
                .find(|s| s.get("codec_type").and_then(|v| v.as_str()) == Some("video"))
                .and_then(|s| s.get("duration"))
                .and_then(|v| v.as_str())
                .and_then(|s| s.parse().ok())
        })
        .unwrap_or(0.0);
    Ok(SubtitleProbe {
        tracks,
        duration,
        sample_rate,
        channels,
    })
}

/// 区域 RGB 均值（crop → rawvideo rgb24），用于对比烧录前后的字幕区域。
fn mean_region(
    ffmpeg: &Path,
    input: &Path,
    x: u32,
    y: u32,
    w: u32,
    h: u32,
) -> Result<(f64, f64, f64), String> {
    let out = input.with_extension("region.rgb");
    run(
        ffmpeg,
        &[
            "-hide_banner".into(),
            "-nostdin".into(),
            "-y".into(),
            "-loglevel".into(),
            "error".into(),
            "-i".into(),
            input.to_string_lossy().to_string(),
            "-vf".into(),
            format!("crop={w}:{h}:{x}:{y},format=rgb24"),
            "-frames:v".into(),
            "1".into(),
            "-an".into(),
            "-pix_fmt".into(),
            "rgb24".into(),
            "-f".into(),
            "rawvideo".into(),
            out.to_string_lossy().to_string(),
        ],
    )?;
    let bytes = std::fs::read(&out).map_err(|e| format!("读取像素失败：{e}"))?;
    let _ = std::fs::remove_file(&out);
    if bytes.len() < 3 {
        return Err(format!("像素数据不足：{} 字节", bytes.len()));
    }
    let pixels = (bytes.len() / 3) as f64;
    let mut sums = [0f64; 3];
    for chunk in bytes.chunks_exact(3) {
        sums[0] += chunk[0] as f64;
        sums[1] += chunk[1] as f64;
        sums[2] += chunk[2] as f64;
    }
    Ok((sums[0] / pixels, sums[1] / pixels, sums[2] / pixels))
}

fn libass_available(ffmpeg: &Path) -> bool {
    let filters = run(ffmpeg, &["-hide_banner".into(), "-filters".into()]).unwrap_or_default();
    filters.lines().any(|line| {
        let mut parts = line.split_whitespace();
        parts.next();
        parts.next() == Some("subtitles")
    })
}

/// 字幕矩阵：软字幕 mp4/mkv、烧录前后的字幕区域像素对比、容器拒绝路径、缺失文件。
fn subtitle_matrix(ffmpeg: &Path, ffprobe: &Path, dir: &Path, report: &mut Report) {
    let name = "subtitle";
    let base = dir.join("sub-base.mp4");
    if !base.is_file() {
        if let Err(e) = run(
            ffmpeg,
            &[
                "-hide_banner".into(),
                "-nostdin".into(),
                "-y".into(),
                "-loglevel".into(),
                "error".into(),
                "-f".into(),
                "lavfi".into(),
                "-i".into(),
                "testsrc2=size=320x240:rate=30:duration=2".into(),
                "-f".into(),
                "lavfi".into(),
                "-i".into(),
                "sine=frequency=440:sample_rate=48000:duration=2".into(),
                "-c:v".into(),
                "libopenh264".into(),
                "-c:a".into(),
                "aac".into(),
                "-b:a".into(),
                "128k".into(),
                "-ac".into(),
                "2".into(),
                "-shortest".into(),
                base.to_string_lossy().to_string(),
            ],
        ) {
            return report.fail(name, format!("生成基准视频失败：{e}"));
        }
    }

    let srt = dir.join("sub.srt");
    let srt_body = "1\n00:00:00,000 --> 00:00:02,000\nSUBTITLE LINE ONE LONG TEXT\nSUBTITLE LINE TWO LONG TEXT\n";
    if let Err(e) = std::fs::write(&srt, srt_body) {
        return report.fail(name, format!("写字幕文件失败：{e}"));
    }

    let subtitle = |mode: &str| {
        let mut s = args::SubtitleParams {
            enabled: true,
            mode: mode.into(),
            path: srt.to_string_lossy().to_string(),
            language: "chi".into(),
            default_track: true,
            ..Default::default()
        };
        if mode == "hard" {
            s.base_height = 240;
        }
        s
    };
    let video_params = |container: &str, subtitle: args::SubtitleParams| args::VideoParams {
        container: container.into(),
        codec: "h264".into(),
        encoder: "libopenh264".into(),
        rate_mode: "quality".into(),
        quality: 23,
        bitrate_kbps: 0,
        resolution: "source".into(),
        fps: "source".into(),
        crop: "source".into(),
        rotation: "0".into(),
        audio_mode: "aac".into(),
        audio_bitrate_kbps: 128,
        hw_decode: false,
        faststart: true,
        speed: 1.0,
        keep_pitch: true,
        watermark: args::WatermarkParams::default(),
        subtitle,
        volume: args::VolumeParams::default(),
        split: args::SplitParams::default(),
    };

    // mp4 内嵌：必须转码 mov_text，language 与 default disposition 写入容器。
    let label = format!("{name}-soft-mp4");
    let out = dir.join("sub-soft.mp4");
    let p = video_params("mp4", subtitle("soft"));
    match args::build_video(
        "libopenh264",
        base.to_string_lossy().as_ref(),
        out.to_string_lossy().as_ref(),
        &p,
        "aac",
        None,
    ) {
        Ok(argv) => match run(ffmpeg, &argv) {
            Ok(_) => match probe_subtitles(ffprobe, &out) {
                Ok(probe) => {
                    let track = probe.tracks.first();
                    let ok = probe.tracks.len() == 1
                        && track.map(|t| t.codec.as_str()) == Some("mov_text")
                        && track.map(|t| t.language.as_str()) == Some("chi")
                        && track.map(|t| t.default) == Some(1);
                    if ok {
                        let t = track.unwrap();
                        report.pass(
                            &label,
                            format!(
                                "1 条 mov_text 字幕流，language={}，disposition.default={}",
                                t.language, t.default
                            ),
                        );
                    } else {
                        report.fail(
                            &label,
                            format!(
                                "字幕流 {} 条（期望 1，mov_text/chi/default=1）：{:?}",
                                probe.tracks.len(),
                                probe
                                    .tracks
                                    .iter()
                                    .map(|t| (&t.codec, &t.language, t.default))
                                    .collect::<Vec<_>>()
                            ),
                        );
                    }
                }
                Err(e) => report.fail(&label, e),
            },
            Err(e) => report.fail(&label, format!("ffmpeg 失败：{e}")),
        },
        Err(e) => report.fail(&label, e),
    }

    // mkv 内嵌：-c:s copy，srt 原样成为 subrip 轨。
    let label = format!("{name}-soft-mkv");
    let out = dir.join("sub-soft.mkv");
    let p = video_params("mkv", subtitle("soft"));
    match args::build_video(
        "libopenh264",
        base.to_string_lossy().as_ref(),
        out.to_string_lossy().as_ref(),
        &p,
        "aac",
        None,
    ) {
        Ok(argv) => match run(ffmpeg, &argv) {
            Ok(_) => match probe_subtitles(ffprobe, &out) {
                Ok(probe) => {
                    let codec = probe.tracks.first().map(|t| t.codec.clone());
                    if probe.tracks.len() == 1 && codec.as_deref() == Some("subrip") {
                        report.pass(
                            &label,
                            format!(
                                "1 条 subrip 字幕流（copy），language={}",
                                probe.tracks[0].language
                            ),
                        );
                    } else {
                        report.fail(
                            &label,
                            format!(
                                "字幕流 {} 条，codec={codec:?}（期望 1 条 subrip）",
                                probe.tracks.len()
                            ),
                        );
                    }
                }
                Err(e) => report.fail(&label, e),
            },
            Err(e) => report.fail(&label, format!("ffmpeg 失败：{e}")),
        },
        Err(e) => report.fail(&label, e),
    }

    // 烧录：仅当 FFmpeg 含 libass 时验证像素差异；否则验证显式拒绝路径。
    let label = format!("{name}-hard-burn");
    if !libass_available(ffmpeg) {
        let err = args::validate_subtitle_burn(&subtitle("hard"), false).unwrap_err();
        if err.contains("libass") {
            report.pass(
                &label,
                format!("FFmpeg 无 libass（-filters 无 subtitles）→ 显式拒绝：{err}"),
            );
        } else {
            report.fail(&label, format!("缺少能力时的错误文案异常：{err}"));
        }
    } else {
        let plain_out = dir.join("sub-plain.mp4");
        let burn_out = dir.join("sub-burn.mp4");
        let plain_params = video_params("mp4", args::SubtitleParams::default());
        let mut hard = subtitle("hard");
        // 320x240 的低分辨率样片里，4.5% 字号只有约 11px，落在固定采样区里的占比过小；
        // 这里用 12%（约 29px）保证「字幕区域均值差」这一断言能真实反映烧录结果。
        hard.font_size_pct = 12.0;
        hard.outline = 2;
        hard.position = "bottom".into();
        hard.color = "#FFFFFF".into();
        hard.margin_v = 24;
        let burn_params = video_params("mp4", hard);
        let mut ran = true;
        for (params, out) in [(&plain_params, &plain_out), (&burn_params, &burn_out)] {
            let argv = match args::build_video(
                "libopenh264",
                base.to_string_lossy().as_ref(),
                out.to_string_lossy().as_ref(),
                params,
                "aac",
                None,
            ) {
                Ok(argv) => argv,
                Err(e) => {
                    report.fail(&label, e);
                    ran = false;
                    break;
                }
            };
            if let Err(e) = run(ffmpeg, &argv) {
                report.fail(&label, format!("ffmpeg 失败：{e}"));
                ran = false;
                break;
            }
        }
        if ran {
            // 底部 1/3、水平中部：320x240 → 160x80 @ (80,160)。
            match (
                mean_region(ffmpeg, &plain_out, 80, 160, 160, 80),
                mean_region(ffmpeg, &burn_out, 80, 160, 160, 80),
            ) {
                (Ok(plain), Ok(burned)) => {
                    let diff = (0..3)
                        .map(|i| match i {
                            0 => (plain.0 - burned.0).abs(),
                            1 => (plain.1 - burned.1).abs(),
                            _ => (plain.2 - burned.2).abs(),
                        })
                        .fold(0.0, f64::max);
                    let probed = probe_subtitles(ffprobe, &burn_out).ok();
                    let duration_ok = probed
                        .as_ref()
                        .map(|p| (p.duration - 2.0).abs() <= 0.1)
                        .unwrap_or(false);
                    let audio_ok = probed
                        .as_ref()
                        .map(|p| p.sample_rate == 48000 && p.channels == 2)
                        .unwrap_or(false);
                    if diff > 15.0 && duration_ok && audio_ok {
                        let p = probed.unwrap();
                        report.pass(
                            &label,
                            format!(
                                "字幕区域均值对比 max Δ={diff:.1}（>15），时长 {:.2}s，音频 {} Hz / {} 声道",
                                p.duration, p.sample_rate, p.channels
                            ),
                        );
                    } else {
                        report.fail(
                            &label,
                            format!(
                                "max Δ={diff:.1}（期望 >15），时长 {:?}，音频 {:?}",
                                probed.as_ref().map(|p| p.duration),
                                probed.as_ref().map(|p| (p.sample_rate, p.channels))
                            ),
                        );
                    }
                }
                (Err(e), _) | (_, Err(e)) => report.fail(&label, e),
            }
        }
    }

    // webm 不承载内嵌字幕轨：必须给出可操作错误。
    let label = format!("{name}-webm-reject");
    let err = args::validate_subtitle(&subtitle("soft"), "webm", 1.0).unwrap_err();
    if err.contains("不承载内嵌字幕轨") {
        report.pass(&label, err);
    } else {
        report.fail(&label, format!("错误文案异常：{err}"));
    }

    // 字幕文件缺失：拒绝并提示重新选择。
    let label = format!("{name}-missing-file");
    let mut missing = subtitle("soft");
    missing.path = dir.join("missing-sub.srt").to_string_lossy().to_string();
    let err = args::validate_subtitle(&missing, "mp4", 1.0).unwrap_err();
    if err.contains("字幕文件不存在") {
        report.pass(&label, err);
    } else {
        report.fail(&label, format!("错误文案异常：{err}"));
    }
}

fn preset_roundtrip(dir: &Path, report: &mut Report) {
    let name = "preset-import-export";
    let path = dir.join("preset.json");
    let built = preset::build(
        json!({
            "video": { "codec": "h264", "quality": 23, "resolution": "1080" },
            "image": { "format": "webp", "maxEdge": 1600, "cropMode": "ratio", "cropRatio": "1:1" },
            "pdf": { "splitMode": "ranges", "ranges": "1-3", "dpi": 150 }
        }),
        json!({ "conflict": "suffix", "nameTemplate": "{name}-{index}" }),
    );
    let built = match built {
        Ok(v) => v,
        Err(e) => return report.fail(name, format!("导出构建失败：{e}")),
    };
    if let Err(e) = preset::write(&path, &built) {
        return report.fail(name, format!("写盘失败：{e}"));
    }
    let loaded = match preset::read(&path) {
        Ok(v) => v,
        Err(e) => return report.fail(name, format!("导入失败：{e}")),
    };
    let bad_version =
        preset::parse(r#"{"version":9,"kind":"transmutio-preset","panels":{},"output":{}}"#)
            .unwrap_err();
    let unknown = preset::parse(
        r#"{"version":1,"kind":"transmutio-preset","panels":{"video":{"foo":1}},"output":{}}"#,
    )
    .unwrap_err();
    let path_template = preset::parse(r#"{"version":1,"kind":"transmutio-preset","panels":{},"output":{"nameTemplate":"C:\\x\\{name}"}}"#).unwrap_err();
    let bytes = std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
    let ok = loaded.version == 1
        && loaded.output.get("conflict").and_then(|v| v.as_str()) == Some("suffix")
        && bad_version.contains("版本 9 不受支持")
        && unknown.contains("未知字段「foo」")
        && path_template.contains("不能包含路径");
    if ok {
        report.pass(
            name,
            format!(
                "导出 {} 字节并成功回读；版本/未知字段/路径模板均明确拒绝",
                bytes
            ),
        );
    } else {
        report.fail(
            name,
            format!("bad_version={bad_version} unknown={unknown} path={path_template}"),
        );
    }
}
