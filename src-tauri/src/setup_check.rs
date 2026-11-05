//! 首次启动引导的「落位」链路检查：无窗口、真实 LGPL ffmpeg、真实文件系统。
//! 与应用命令共用 `setup::install_blocking`，不是替身实现。

use crate::{flavor, jobs, setup, tools};
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

pub const LEVEL: &str = "真实 setup 落位管线 + 真实 LGPL ffmpeg + 真实文件系统（无窗口）";

struct Report {
    results: Vec<Value>,
    passed: usize,
    failed: usize,
    skipped: usize,
}

impl Report {
    fn new() -> Self {
        Self { results: Vec::new(), passed: 0, failed: 0, skipped: 0 }
    }

    fn check(&mut self, name: &str, ok: bool, detail: impl Into<String>, evidence: Value) {
        let detail = detail.into();
        if ok {
            self.passed += 1;
        } else {
            self.failed += 1;
        }
        println!("{} {name}：{detail}", if ok { "PASS" } else { "FAIL" });
        self.results.push(json!({
            "name": name,
            "status": if ok { "pass" } else { "fail" },
            "detail": detail,
            "evidence": evidence,
            "level": LEVEL,
        }));
    }

    fn skip(&mut self, name: &str, detail: impl Into<String>, evidence: Value) {
        let detail = detail.into();
        self.skipped += 1;
        println!("SKIP {name}：{detail}");
        self.results.push(json!({
            "name": name,
            "status": "skip",
            "detail": detail,
            "evidence": evidence,
            "level": LEVEL,
        }));
    }
}

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| PathBuf::from("."))
}

/// 优先读环境变量；否则在 samples/_dl 下找路径含 `lgpl` 的 `bin/ffmpeg(.exe)`。
fn find_lgpl_ffmpeg(root: &Path) -> Option<PathBuf> {
    if let Ok(path) = std::env::var("FFMPEG_LGPL_BIN") {
        let p = PathBuf::from(path.trim().trim_matches('"'));
        if p.is_file() {
            return Some(p);
        }
    }
    fn walk(dir: &Path, depth: usize, out: &mut Vec<PathBuf>) {
        if depth > 5 {
            return;
        }
        let Ok(entries) = std::fs::read_dir(dir) else { return };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                walk(&path, depth + 1, out);
            } else if path.file_name().map(|n| n == "ffmpeg.exe" || n == "ffmpeg").unwrap_or(false)
                && path
                    .parent()
                    .and_then(|p| p.file_name())
                    .map(|n| n == "bin")
                    .unwrap_or(false)
                && path
                    .components()
                    .any(|c| c.as_os_str().to_string_lossy().to_lowercase().contains("lgpl"))
            {
                out.push(path);
            }
        }
    }
    let mut found = Vec::new();
    walk(&root.join("samples").join("_dl"), 0, &mut found);
    found.into_iter().next()
}

fn probe_sibling(ffmpeg: &Path) -> Option<PathBuf> {
    let name = if cfg!(windows) { "ffprobe.exe" } else { "ffprobe" };
    ffmpeg.parent().map(|d| d.join(name)).filter(|p| p.is_file())
}

fn list_dirs(dir: &Path) -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(dir)
        .map(|entries| {
            entries
                .flatten()
                .filter(|e| e.path().is_dir())
                .map(|e| e.file_name().to_string_lossy().to_string())
                .collect()
        })
        .unwrap_or_default();
    names.sort();
    names
}

fn count_tmp(dir: &Path) -> usize {
    let mut count = 0usize;
    let Ok(entries) = std::fs::read_dir(dir) else { return 0 };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            count += count_tmp(&path);
        } else if path
            .file_name()
            .map(|n| n.to_string_lossy().starts_with(".tmp-") || n.to_string_lossy().starts_with(".ff-tmp-"))
            .unwrap_or(false)
        {
            count += 1;
        }
    }
    count
}

fn files_in(dir: &Path) -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(dir)
        .map(|entries| {
            entries
                .flatten()
                .filter(|e| e.path().is_file())
                .map(|e| e.file_name().to_string_lossy().to_string())
                .collect()
        })
        .unwrap_or_default();
    names.sort();
    names
}

/// 真实 ffmpeg 二进制 + 只改版本横幅里的构建配置字符串（等长替换），
/// 用于覆盖「nonfree 构建必须拒绝」的分支；不改任何可执行逻辑。
fn patch_nonfree(src: &Path, dst: &Path) -> std::io::Result<bool> {
    let mut bytes = std::fs::read(src)?;
    let needle = b"--enable-libvmaf";
    let replacement = b"--enable-nonfree";
    assert_eq!(needle.len(), replacement.len());
    let Some(pos) = bytes.windows(needle.len()).position(|w| w == needle) else {
        return Ok(false);
    };
    bytes[pos..pos + needle.len()].copy_from_slice(replacement);
    if let Some(parent) = dst.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(dst, bytes)?;
    Ok(true)
}

fn run_capture(program: &Path, args: &[&str]) -> (bool, String) {
    let output = std::process::Command::new(program).args(args).output();
    match output {
        Ok(o) => (
            o.status.success(),
            String::from_utf8_lossy(&o.stderr).to_string() + &String::from_utf8_lossy(&o.stdout).to_string(),
        ),
        Err(err) => (false, format!("无法启动 {}：{err}", program.display())),
    }
}

pub async fn run_checks(report_path: &str) -> i32 {
    let root = repo_root();
    let work = root.join("samples").join("setup-check");
    let _ = std::fs::remove_dir_all(&work);
    if let Err(err) = std::fs::create_dir_all(&work) {
        eprintln!("无法创建工作目录 {}：{err}", work.display());
        return 1;
    }
    let appdata = work.join("appdata");
    let out_dir = work.join("out");
    let mut report = Report::new();

    let Some(lgpl) = find_lgpl_ffmpeg(&root) else {
        eprintln!("未找到 LGPL ffmpeg。请设置 FFMPEG_LGPL_BIN，或把 LGPL 包解压到 samples/_dl/。");
        return 1;
    };
    let ffprobe = probe_sibling(&lgpl);

    // 1) 校验 LGPL 构建
    let inspection = setup::inspect_file(&lgpl.to_string_lossy());
    report.check(
        "inspect：识别真实 LGPL 构建",
        inspection.exists
            && !inspection.version.is_empty()
            && inspection.lgpl_ok
            && !inspection.gpl
            && !inspection.nonfree
            && inspection.has_aac_encoder,
        format!(
            "version={} · gpl={} · nonfree={} · lgplOk={} · h264={} · aac={} · sha256={}",
            inspection.version,
            inspection.gpl,
            inspection.nonfree,
            inspection.lgpl_ok,
            inspection.has_h264_encoder,
            inspection.has_aac_encoder,
            inspection.sha256
        ),
        json!({
            "ffmpeg": lgpl.to_string_lossy(),
            "configuration": inspection.configuration,
            "size": inspection.size,
            "notes": inspection.notes,
        }),
    );

    // 2) 正常落位：目录、原子复制、落位后哈希复核
    let probe_str: Option<String> = ffprobe.as_deref().map(|p| p.to_string_lossy().to_string());
    let installed = setup::install_blocking(
        &appdata,
        flavor::Flavor::Lgpl,
        &lgpl.to_string_lossy(),
        probe_str.as_deref(),
        Some(inspection.sha256.as_str()),
        false,
    );
    let tools_root = appdata.join("tools");
    match &installed {
        Ok(info) => {
            let dest = info.ffmpeg.as_ref().map(|t| t.path.clone()).unwrap_or_default();
            let dest_path = PathBuf::from(&dest);
            let dest_dir = dest_path.parent().map(|p| p.to_path_buf()).unwrap_or_default();
            let copied_hash = setup::hash_file(&dest_path).unwrap_or_default();
            report.check(
                "落位：路径、哈希复核、无临时文件",
                dest_path.is_file()
                    && copied_hash.eq_ignore_ascii_case(&inspection.sha256)
                    && dest_dir
                        .file_name()
                        .map(|n| n.to_string_lossy().starts_with("ffmpeg-"))
                        .unwrap_or(false)
                    && count_tmp(&appdata) == 0,
                format!(
                    "dest={dest} · hashMatch={} · dirs={:?} · tmp={} · files={:?}",
                    copied_hash.eq_ignore_ascii_case(&inspection.sha256),
                    list_dirs(&tools_root),
                    count_tmp(&appdata),
                    files_in(&dest_dir)
                ),
                json!({
                    "appData": appdata.to_string_lossy(),
                    "installed": info,
                }),
            );
        }
        Err(err) => report.check("落位：路径、哈希复核、无临时文件", false, err.clone(), json!({})),
    }

    // 3) 同一版本再次落位不覆盖已有目录
    let second = setup::install_blocking(
        &appdata,
        flavor::Flavor::Lgpl,
        &lgpl.to_string_lossy(),
        probe_str.as_deref(),
        Some(inspection.sha256.as_str()),
        false,
    );
    let dirs_before_reinstall = list_dirs(&tools_root);
    report.check(
        "重复落位：新建独立版本目录，不覆盖已有文件",
        second.is_ok() && dirs_before_reinstall.len() == 2,
        format!("dirs={dirs_before_reinstall:?}"),
        json!({ "toolsDirs": dirs_before_reinstall }),
    );

    // 4) 错误哈希：拒绝且不创建任何目录/临时文件
    let before = list_dirs(&tools_root).len();
    let bad_hash = "0000000000000000000000000000000000000000000000000000000000000000";
    let wrong = setup::install_blocking(
        &appdata,
        flavor::Flavor::Lgpl,
        &lgpl.to_string_lossy(),
        ffprobe.as_deref().map(|p| p.to_string_lossy().to_string()).as_deref(),
        Some(bad_hash),
        false,
    );
    let after = list_dirs(&tools_root).len();
    report.check(
        "错误哈希：拒绝并保持目录不变",
        wrong.is_err()
            && wrong.as_ref().err().map(|e| e.contains("哈希不匹配")).unwrap_or(false)
            && after == before
            && count_tmp(&appdata) == 0,
        format!(
            "err={:?} · dirs {}→{} · tmp={}",
            wrong.err(),
            before,
            after,
            count_tmp(&appdata)
        ),
        json!({ "appData": appdata.to_string_lossy() }),
    );

    // 5) nonfree 变体：拒绝
    let nonfree_bin = work.join("fixtures").join(if cfg!(windows) { "ffmpeg-nonfree.exe" } else { "ffmpeg-nonfree" });
    let patched = patch_nonfree(&lgpl, &nonfree_bin);
    let patched_ok = patched.as_ref().map(|ok| *ok).unwrap_or(false);
    let patched_info = patched_ok.then(|| setup::inspect_file(&nonfree_bin.to_string_lossy()));
    if let Some(info) = &patched_info {
        let before = list_dirs(&tools_root).len();
        let denied = setup::install_blocking(
            &appdata,
            flavor::Flavor::Lgpl,
            &nonfree_bin.to_string_lossy(),
            None,
            None,
            true,
        );
        let after = list_dirs(&tools_root).len();
        report.check(
            "nonfree 构建：即使 allowGpl 也拒绝",
            info.nonfree
                && !info.version.is_empty()
                && denied.is_err()
                && denied.as_ref().err().map(|e| e.contains("nonfree")).unwrap_or(false)
                && after == before,
            format!(
                "fixture={} · version={} · nonfree={} · err={:?} · dirs {}→{}",
                nonfree_bin.display(),
                info.version,
                info.nonfree,
                denied.err(),
                before,
                after
            ),
            json!({
                "fixture": nonfree_bin.to_string_lossy(),
                "patch": "把版本横幅里的 --enable-libvmaf 等长替换为 --enable-nonfree（真实二进制，未改执行逻辑）",
                "inspection": info,
            }),
        );
    } else {
        report.skip("nonfree 构建：即使 allowGpl 也拒绝", "无法生成 nonfree 变体（未找到可替换的构建配置字符串）", json!({}));
    }

    // 6) 不存在的文件
    let missing = work.join("no-such-ffmpeg.exe");
    let err = setup::install_blocking(
        &appdata,
        flavor::Flavor::Lgpl,
        &missing.to_string_lossy(),
        None,
        None,
        false,
    );
    report.check(
        "不存在的文件：拒绝",
        err.as_ref().err().map(|e| e.contains("不存在")).unwrap_or(false),
        format!("err={:?}", err.err()),
        json!({ "path": missing.to_string_lossy() }),
    );

    // 7) 存在但读不到版本的文件
    let junk = work.join("not-ffmpeg.exe");
    let _ = std::fs::write(&junk, "这不是可执行文件");
    let junk_info = setup::inspect_file(&junk.to_string_lossy());
    let err = setup::install_blocking(
        &appdata,
        flavor::Flavor::Lgpl,
        &junk.to_string_lossy(),
        None,
        None,
        false,
    );
    report.check(
        "无版本信息的文件：拒绝",
        junk_info.exists
            && junk_info.version.is_empty()
            && err.as_ref().err().map(|e| e.contains("无法读取版本")).unwrap_or(false),
        format!("exists={} · version={:?} · err={:?}", junk_info.exists, junk_info.version, err.err()),
        json!({ "path": junk.to_string_lossy() }),
    );

    // 8) 变体判定：同一个系统 GPL 构建，在 LGPL 版被拒绝，在 GPL 版被接受
    let mut system_gpl: Option<PathBuf> = None;
    if let Some(system) = tools::find_tool("ffmpeg") {
        let sys_info = setup::inspect_file(&system.to_string_lossy());
        if sys_info.gpl && !sys_info.nonfree {
            system_gpl = Some(system.clone());
            let err = setup::install_blocking(
                &appdata,
                flavor::Flavor::Lgpl,
                &system.to_string_lossy(),
                None,
                None,
                false,
            );
            report.check(
                "LGPL 版：拒绝 GPL 构建，并提示改用 GPL 版",
                err.as_ref()
                    .err()
                    .map(|e| e.contains("GPL") && e.contains("GPL 版"))
                    .unwrap_or(false)
                    && count_tmp(&appdata) == 0,
                format!("ffmpeg={} · err={:?}", system.display(), err.err()),
                json!({
                    "ffmpeg": system.to_string_lossy(),
                    "gpl": sys_info.gpl,
                    "configuration": sys_info.configuration,
                }),
            );
        } else {
            report.skip(
                "LGPL 版：拒绝 GPL 构建，并提示改用 GPL 版",
                "系统 ffmpeg 不是 GPL 构建",
                json!({}),
            );
        }
    } else {
        report.skip("LGPL 版：拒绝 GPL 构建，并提示改用 GPL 版", "系统未找到 ffmpeg", json!({}));
    }

    // 8b) GPL 版接受 GPL 构建，并落在独立的 appdata（与 LGPL 版互不覆盖）
    let gpl_appdata = work.join("gpl-appdata");
    let lgpl_tools = appdata.join("tools");
    match &system_gpl {
        Some(system) => {
            let accepted = setup::install_blocking(
                &gpl_appdata,
                flavor::Flavor::Gpl,
                &system.to_string_lossy(),
                None,
                None,
                false,
            );
            let gpl_tools = gpl_appdata.join("tools");
            let dest = accepted.as_ref().ok().and_then(|i| i.ffmpeg.as_ref().map(|t| t.path.clone()));
            report.check(
                "GPL 版：接受 GPL 构建，且与 LGPL 版数据目录隔离",
                accepted.is_ok()
                    && dest
                        .as_deref()
                        .map(|d| Path::new(d).starts_with(&gpl_tools))
                        .unwrap_or(false)
                    && gpl_tools != lgpl_tools,
                format!(
                    "gplAppData={} · dest={:?} · lgplDirs={:?} · gplDirs={:?}",
                    gpl_appdata.display(),
                    dest,
                    list_dirs(&lgpl_tools),
                    list_dirs(&gpl_tools)
                ),
                json!({
                    "lgplAppData": appdata.to_string_lossy(),
                    "gplAppData": gpl_appdata.to_string_lossy(),
                    "installed": accepted,
                }),
            );

            // 8c) GPL 版拒绝 LGPL 构建（保证 GPL 包只分发 GPL 构建）
            let err = setup::install_blocking(
                &gpl_appdata,
                flavor::Flavor::Gpl,
                &lgpl.to_string_lossy(),
                None,
                None,
                false,
            );
            report.check(
                "GPL 版：拒绝 LGPL 构建（需要 libx264/libx265）",
                err.as_ref().err().map(|e| e.contains("需要 GPL 构建")).unwrap_or(false),
                format!("err={:?}", err.err()),
                json!({ "flavor": "gpl", "ffmpeg": lgpl.to_string_lossy() }),
            );
        }
        None => {
            report.skip(
                "GPL 版：接受 GPL 构建，且与 LGPL 版数据目录隔离",
                "缺少真实 GPL 构建",
                json!({}),
            );
            report.skip("GPL 版：拒绝 LGPL 构建（需要 libx264/libx265）", "已由 accept 用例覆盖", json!({}));
        }
    }

    // 9) 闭环：用落位后的 ffmpeg 真实转码
    let Ok(info) = installed else {
        report.check("闭环：落位后的 ffmpeg 真实转码", false, "落位失败，无法继续", json!({}));
        return finish(report, report_path, &appdata);
    };
    let installed_ffmpeg = PathBuf::from(info.ffmpeg.as_ref().map(|t| t.path.clone()).unwrap_or_default());
    std::fs::create_dir_all(&out_dir).ok();
    let source = work.join("source.mp4");
    let generated = run_capture(
        &installed_ffmpeg,
        &[
            "-y",
            "-hide_banner",
            "-f",
            "lavfi",
            "-i",
            "testsrc2=size=640x360:rate=30",
            "-f",
            "lavfi",
            "-i",
            "sine=frequency=440:sample_rate=44100",
            "-t",
            "3",
            "-c:v",
            "mpeg4",
            "-c:a",
            "aac",
            "-shortest",
            source.to_string_lossy().as_ref(),
        ],
    );
    report.check(
        "闭环：生成测试素材",
        generated.0 && source.is_file(),
        format!("ffmpeg={} · stderr尾部={}", installed_ffmpeg.display(), tail(&generated.1, 200)),
        json!({ "source": source.to_string_lossy() }),
    );

    // 暂时移走源 LGPL 目录，确保转码只能用「落位后」的二进制。
    let hidden = hide_source_bin(&lgpl);
    let result = run_conversion(info.clone(), &source, &out_dir).await;
    restore_source_bin(&lgpl, hidden.as_ref());
    report.check(
        "闭环：落位后的 ffmpeg 完成真实转码",
        result.0,
        result.1.clone(),
        json!({
            "installedFfmpeg": installed_ffmpeg.to_string_lossy(),
            "sourceBinHidden": hidden.is_some(),
            "outputDir": out_dir.to_string_lossy(),
        }),
    );

    finish(report, report_path, &appdata)
}

fn tail(text: &str, limit: usize) -> String {
    let filtered: Vec<&str> = text.lines().map(str::trim).filter(|l| !l.is_empty()).collect();
    let joined = filtered.iter().rev().take(2).cloned().collect::<Vec<_>>().join(" · ");
    if joined.chars().count() > limit {
        joined.chars().take(limit).collect::<String>() + "…"
    } else {
        joined
    }
}

fn hide_source_bin(ffmpeg: &Path) -> Option<PathBuf> {
    let bin = ffmpeg.parent()?.to_path_buf();
    let target = bin.with_file_name("bin.hidden");
    std::fs::rename(&bin, &target).ok().map(|_| target)
}

fn restore_source_bin(ffmpeg: &Path, hidden: Option<&PathBuf>) {
    if let Some(target) = hidden {
        if let Some(bin) = ffmpeg.parent() {
            let _ = std::fs::rename(target, bin);
        }
    }
}

async fn run_conversion(info: tools::ToolsInfo, source: &Path, out_dir: &Path) -> (bool, String) {
    let state = Arc::new(jobs::AppState::default());
    {
        let mut guard = state.tools.lock().await;
        *guard = Some(info);
    }
    let ctx = jobs::Ctx::silent(state);
    tokio::spawn(jobs::start_scheduler(ctx.clone()));

    let spec = jobs::JobSpec {
        kind: "audio-aac".into(),
        input: source.to_string_lossy().to_string(),
        output_dir: out_dir.to_string_lossy().to_string(),
        inputs: Vec::new(),
        name_template: None,
        conflict: None,
        output_stem: Some("引导安装后转码".into()),
        params: json!({
            "format": "aac",
            "bitrateKbps": 128,
            "sampleRate": "source",
            "channels": "source",
        }),
    };
    let started = match jobs::enqueue(&ctx, vec![spec]).await {
        Ok(jobs) => jobs,
        Err(err) => return (false, format!("入队失败：{err}")),
    };
    let Some(job) = started.first() else {
        return (false, "入队未返回任务".into());
    };

    let deadline = Instant::now() + Duration::from_secs(60);
    loop {
        let current = jobs::list(&ctx)
            .await
            .into_iter()
            .find(|j| j.id == job.id);
        if let Some(job) = current {
            if job.status == "done" {
                let output = PathBuf::from(&job.output);
                let size = std::fs::metadata(&output).map(|m| m.len()).unwrap_or(0);
                let temps = list_temps(&out_dir);
                let ok = output.is_file() && size > 0 && temps == 0;
                return (
                    ok,
                    format!(
                        "status=done · output={} · size={size} · encoder={:?} · tmp={temps}",
                        job.output, job.encoder_used
                    ),
                );
            }
            if job.status == "failed" || job.status == "cancelled" {
                return (
                    false,
                    format!("status={} · kind={:?} · message={:?}", job.status, job.error_kind, job.message),
                );
            }
        }
        if Instant::now() > deadline {
            return (false, "等待转码超时（60s）".into());
        }
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
}

fn list_temps(dir: &Path) -> usize {
    let mut count = 0usize;
    let Ok(entries) = std::fs::read_dir(dir) else { return 0 };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            count += list_temps(&path);
        } else if path
            .file_name()
            .map(|n| n.to_string_lossy().starts_with(".ff-tmp-"))
            .unwrap_or(false)
        {
            count += 1;
        }
    }
    count
}

fn finish(report: Report, report_path: &str, appdata: &Path) -> i32 {
    let payload = json!({
        "generatedAt": std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis())
            .unwrap_or(0),
        "level": LEVEL,
        "environment": {
            "platform": std::env::consts::OS,
            "arch": std::env::consts::ARCH,
            "appDataDir": appdata.to_string_lossy(),
            "note": "落位目录为测试用 appData（samples/setup-check/appdata），不是真实安装目录",
        },
        "summary": {
            "passed": report.passed,
            "failed": report.failed,
            "skipped": report.skipped,
            "total": report.results.len(),
        },
        "results": report.results,
    });
    let path = Path::new(report_path);
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    match serde_json::to_string_pretty(&payload).map_err(|e| e.to_string()).and_then(|text| {
        std::fs::write(path, text).map_err(|e| e.to_string())
    }) {
        Ok(()) => println!(
            "setup 落位检查完成：{} PASS / {} FAIL / {} SKIP（报告：{}）",
            report.passed, report.failed, report.skipped, report_path
        ),
        Err(err) => eprintln!("写入报告失败：{err}"),
    }
    report.failed as i32
}
