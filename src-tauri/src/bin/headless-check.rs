//! 无窗口真实链路检查入口：`cargo run --bin headless-check -- samples/core-e2e-report.json`
//! 直接驱动应用同一份任务核心与真实 ffmpeg，不启动任何窗口。

#[tokio::main]
async fn main() {
    let report = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "samples/core-e2e-report.json".to_string());
    let code = formatforge_lib::headless::run_checks(&report).await;
    std::process::exit(code);
}
