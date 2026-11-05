//! P2 真实产物检查（dev-tools）。不依赖 UI，直接调用产品内的参数构造与 PDF 模块，
//! 用 ffmpeg/ffprobe 生成并校验真实文件；PDF 渲染另需 pdftoppm（缺失时验证拒绝路径）。
//!
//! 运行：设置 FFMPEG_PATH / FFPROBE_PATH（可选 PDFTOPPM_PATH），
//! `cargo run --features dev-tools --bin p2-check`。

use formatforge_lib::{args, containers, pdf, pdf_tools, preset};
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

    image_sequence_to_video(&ffmpeg, &ffprobe, &dir, &mut report);
    video_to_frames(&ffmpeg, &ffprobe, &dir, &mut report);
    image_crop(&ffmpeg, &ffprobe, &dir, &mut report);
    image_collage(&ffmpeg, &ffprobe, &dir, &mut report);
    pdf_merge_and_split(&dir, &mut report);
    pdf_to_image(&dir, &ffprobe, &mut report);
    format_matrix(&ffmpeg, &ffprobe, &dir, &mut report);
    speed_matrix(&ffmpeg, &ffprobe, &dir, &mut report);
    watermark_matrix(&ffmpeg, &ffprobe, &dir, &mut report);
    subtitle_matrix(&ffmpeg, &ffprobe, &dir, &mut report);
    preset_roundtrip(&dir, &mut report);

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
