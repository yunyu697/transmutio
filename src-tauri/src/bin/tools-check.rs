//! 内置下载器的离线断言 + 可选真实下载验证。
//!
//! 用法（必须带 dev-tools feature，默认不构建）：
//!   cargo run --bin tools-check --features dev-tools
//!   cargo run --bin tools-check --features dev-tools -- --live ffmpeg-windows-x64-lgpl
//!
//! 不带 `--live` 时不访问网络：只断言清单、变体匹配、镜像顺序、校验拒绝、空间与续传分支。
//! `--live` 会把包真实下载到系统临时目录，打印耗时、大小与 SHA-256（验证完即删除）。
#[cfg(feature = "dev-tools")]
fn main() {
    use formatforge_lib::download::{
        self, ensure_space, plan_for, resume_action, verify_file, ResumeAction,
    };
    use formatforge_lib::flavor::Flavor;

    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut live: Option<String> = None;
    let mut source: Option<String> = None;
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--live" => {
                index += 1;
                live = args.get(index).cloned();
            }
            "--source" => {
                index += 1;
                source = args.get(index).cloned();
            }
            other => live = Some(other.to_string()),
        }
        index += 1;
    }
    let mut failures = 0usize;
    let mut pass = |label: &str, ok: bool, detail: String| {
        if ok {
            println!("PASS  {label}  {detail}");
        } else {
            failures += 1;
            println!("FAIL  {label}  {detail}");
        }
    };

    let manifest = download::manifest().expect("内置清单必须能解析");
    println!(
        "清单：schema={} 来源={} 包={} 手动方案={}",
        manifest.schema,
        manifest.sources.len(),
        manifest.packages.len(),
        manifest.manual.len()
    );
    for pkg in &manifest.packages {
        println!(
            "  - {} ({}/{}, {}, {}) {} bytes sha256={}",
            pkg.id, pkg.platform, pkg.arch, pkg.flavor, pkg.version, pkg.size, pkg.sha256
        );
    }

    let lgpl = plan_for(Flavor::Lgpl, "windows", "x86_64").expect("windows/lgpl 计划");
    let lgpl_ids: Vec<_> = lgpl.entries.iter().map(|e| e.package_id.clone()).collect();
    pass(
        "LGPL 计划不含 GPL 包",
        !lgpl_ids.iter().any(|id| id.contains("-gpl")),
        format!("entries={lgpl_ids:?}"),
    );
    pass(
        "LGPL 计划至少有官方源 + 2 个镜像",
        lgpl.entries
            .iter()
            .all(|e| e.sources.len() >= 3 && e.sources[0].source_id == "official"),
        format!(
            "每个包来源数={}",
            lgpl.entries.first().map(|e| e.sources.len()).unwrap_or(0)
        ),
    );

    let gpl = plan_for(Flavor::Gpl, "windows", "x86_64").expect("windows/gpl 计划");
    pass(
        "GPL 计划不含 LGPL 包",
        !gpl.entries.iter().any(|e| e.package_id.contains("-lgpl")),
        format!(
            "entries={:?}",
            gpl.entries
                .iter()
                .map(|e| &e.package_id)
                .collect::<Vec<_>>()
        ),
    );

    let arm = plan_for(Flavor::Lgpl, "windows", "aarch64").expect("windows/arm64 计划");
    pass(
        "无内置包的架构不给假下载项",
        arm.entries.is_empty(),
        format!("entries={} manual={}", arm.entries.len(), arm.manual.len()),
    );
    let mac = plan_for(Flavor::Lgpl, "macos", "x86_64").expect("macos 计划");
    pass(
        "macOS 无 FFmpeg 自动包，但给出可读原因",
        mac.entries.iter().all(|e| e.kind != "ffmpeg")
            && mac
                .manual
                .iter()
                .any(|m| m.kind == "ffmpeg" && m.reason.contains("不提供自动下载")),
        format!("manual={}", mac.manual.len()),
    );
    let linux = plan_for(Flavor::Gpl, "linux", "x86_64").expect("linux 计划");
    pass(
        "Linux 无 FFmpeg 自动包，但给出可读原因",
        linux.entries.iter().all(|e| e.kind != "ffmpeg")
            && linux
                .manual
                .iter()
                .any(|m| m.kind == "ffmpeg" && m.reason.contains("不提供自动下载")),
        format!("manual={}", linux.manual.len()),
    );

    let tmp = std::env::temp_dir().join(format!("transmutio-tools-check-{}", std::process::id()));
    std::fs::create_dir_all(&tmp).expect("创建临时目录");
    let fake = tmp.join("fake.zip");
    std::fs::write(&fake, b"not the real archive").expect("写临时文件");
    // "not the real archive" 恰好 20 字节
    let err = verify_file(&fake, &"0".repeat(64), 20).expect_err("哈希必须被拒绝");
    pass(
        "校验失败拒绝落盘",
        err.contains("SHA-256 校验失败") && err.contains("拒绝落盘"),
        err.clone(),
    );
    let ok_size = verify_file(&fake, &"0".repeat(64), 21).is_err();
    pass("大小不符也拒绝", ok_size, "expected size mismatch".into());
    let space = ensure_space(&tmp, u64::MAX / 2).expect_err("空间不足必须可读");
    pass(
        "磁盘空间不足给出可读原因",
        space.contains("磁盘空间不足") && space.contains("不会开始下载"),
        space.clone(),
    );
    std::fs::remove_dir_all(&tmp).ok();

    let actions = [
        (resume_action(200, 0, 100), ResumeAction::Restart),
        (resume_action(200, 40, 100), ResumeAction::Restart),
        (resume_action(206, 40, 100), ResumeAction::Append),
        (resume_action(416, 40, 100), ResumeAction::DropAndRestart),
        (resume_action(206, 100, 100), ResumeAction::Complete),
    ];
    pass(
        "断点续传分支（200/206/416/已完成）",
        actions.iter().all(|(a, b)| a == b),
        format!("{actions:?}"),
    );
    let order = download::ordered_sources(&manifest.sources, Some("ghproxy"));
    pass(
        "指定镜像优先，其余按清单顺序",
        order.first().map(|s| s.id.as_str()) == Some("ghproxy")
            && order.get(1).map(|s| s.id.as_str()) == Some("official"),
        format!("{:?}", order.iter().map(|s| &s.id).collect::<Vec<_>>()),
    );

    if let Some(package_id) = live {
        println!(
            "\n[live] 开始真实下载：{package_id}（来源偏好={:?}）",
            source
        );
        let dir = std::env::temp_dir().join(format!("transmutio-live-{}", std::process::id()));
        let started = std::time::Instant::now();
        let mut last_percent: Option<u64> = None;
        let result = download::download_blocking(
            &dir,
            Flavor::current(),
            &package_id,
            source.as_deref(),
            |p| {
                if p.total == 0 {
                    return;
                }
                let percent = p.received * 100 / p.total;
                let should_print = match last_percent {
                    Some(last) => percent >= last + 5 || p.phase != "download",
                    None => true,
                };
                if should_print {
                    last_percent = Some(percent);
                    println!(
                        "  [{percent:>3}%] {} {} / {}  {}",
                        p.phase, p.received, p.total, p.message
                    );
                }
            },
        );
        match result {
            Ok(landed) => {
                println!(
                    "[live] 成功：kind={} version={} dir={} archive_sha256={} 耗时={}s",
                    landed.kind,
                    landed.version,
                    landed.dir,
                    landed.archive_sha256,
                    started.elapsed().as_secs()
                );
            }
            Err(err) => {
                failures += 1;
                println!("[live] 失败（可读原因）：\n{err}");
            }
        }
        std::fs::remove_dir_all(&dir).ok();
    } else {
        println!("\n离线断言全部执行完毕（未指定 --live，不访问网络）");
    }

    if failures > 0 {
        eprintln!("\n{failures} 项失败");
        std::process::exit(1);
    }
    println!("\n全部通过");
}

#[cfg(not(feature = "dev-tools"))]
fn main() {
    eprintln!("tools-check 需要 --features dev-tools");
    std::process::exit(2);
}
