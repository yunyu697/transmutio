//! 无窗口更新器检查（dev-tools）：真实 GitHub API、真实下载、真实大小/哈希/签名校验。
//!
//! 运行：`cargo run --features dev-tools --bin updater-check -- [当前版本] [--mirror=id] [--download]`
//! 例：`... -- updater-check -- 1.0.0-alpha3 --download` 会按 alpha3 为基线找新版本并真实下载。

use formatforge_lib::updater;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut current = env!("CARGO_PKG_VERSION").to_string();
    let mut preferred: Option<String> = None;
    let mut download = false;
    for arg in &args {
        if arg == "--download" {
            download = true;
        } else if let Some(rest) = arg.strip_prefix("--mirror=") {
            preferred = Some(rest.to_string());
        } else {
            current = arg.clone();
        }
    }

    let info = updater::info();
    println!("程序版本：{}", info.current_version);
    println!(
        "检查基线：{current} · {}/{} · 通道 {}",
        info.platform, info.arch, info.channel
    );
    println!("签名公钥：{}", if info.signature_configured { "已配置" } else { "未配置（安装会 fail-closed）" });
    println!("安装能力：{}（{}）", info.install_mode, info.install_mode_label);

    let result = updater::check_from(&current, preferred.as_deref());
    println!("{}", serde_json::to_string_pretty(&result).unwrap_or_default());

    if !download {
        return;
    }
    let candidate = match result.candidate {
        Some(v) => v,
        None => {
            println!("没有可下载的新版本，跳过下载。");
            return;
        }
    };
    let dir = std::env::temp_dir().join(format!("transmutio-updater-{}", std::process::id()));
    let spec = updater::DownloadSpec {
        version: candidate.version.clone(),
        tag: candidate.tag.clone(),
        asset_name: candidate.asset_name.clone(),
        asset_size: candidate.asset_size,
        asset_sha256: candidate.asset_sha256.clone(),
        asset_url: candidate.asset_url.clone(),
        sums_url: candidate.sums_url.clone(),
        signature_url: candidate.signature_url.clone(),
    };
    let mut last = 0u64;
    let outcome = updater::download_headless(&dir, &spec, preferred.as_deref(), |phase, _i, _t, received, total, label| {
        if total > 0 && (received - last >= 256 * 1024 || received == total) {
            last = received;
            println!("[{phase}] {label} {received}/{total} ({:.1}%)", received as f64 / total as f64 * 100.0);
        }
    });

    match outcome {
        Ok(downloaded) => {
            let size = std::fs::metadata(&downloaded.path).map(|m| m.len()).unwrap_or(0);
            println!("落盘：{}", downloaded.path.display());
            println!(
                "字节：{size}（期望 {}）· SHA-256：{} · 来源：{} · 签名：{}",
                spec.asset_size,
                downloaded.sha256,
                downloaded.source_label,
                if downloaded.signature_verified { "通过" } else { "未执行" }
            );
            println!("来源尝试：{}", serde_json::to_string_pretty(&downloaded.attempts).unwrap_or_default());
            let sig_path = format!("{}.sig", downloaded.path.display());
            match std::fs::read_to_string(&sig_path) {
                Ok(sig) => println!("独立复核签名：{:?}", updater::verify_signature(&downloaded.path, &sig)),
                Err(_) => println!(
                    "未拿到 .sig 附件；安装路径应 fail-closed：{:?}",
                    updater::verify_signature(&downloaded.path, "")
                ),
            }
        }
        Err(error) => println!("下载失败：{error}"),
    }
}
