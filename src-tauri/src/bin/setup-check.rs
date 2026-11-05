//! 无窗口的首次启动「落位」检查入口。真实 LGPL ffmpeg + 真实文件系统。
//! 用法：setup-check [报告路径]，可用 FFMPEG_LGPL_BIN 指定 LGPL ffmpeg。

#[tokio::main]
async fn main() {
    let report = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "samples/setup-install-report.json".to_string());
    let code = formatforge_lib::setup_check::run_checks(&report).await;
    std::process::exit(code);
}
