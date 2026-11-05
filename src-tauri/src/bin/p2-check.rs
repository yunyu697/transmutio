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
    let argv = args::build_image(
        &src.to_string_lossy(),
        ratio_out.to_string_lossy().as_ref(),
        &ratio_params,
    );
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
    let argv = args::build_image(
        &src.to_string_lossy(),
        custom_out.to_string_lossy().as_ref(),
        &custom_params,
    );
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
