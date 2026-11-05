//! 双变体无窗口检查入口。
//! 用法：flavor-check [报告路径]
//!   LGPL 版：cargo run --bin flavor-check -- samples/flavor-check-lgpl.json
//!   GPL 版 ：cargo run --features gpl --bin flavor-check -- samples/flavor-check-gpl.json

#[tokio::main]
async fn main() {
    let report = std::env::args().nth(1).unwrap_or_else(|| {
        format!(
            "samples/flavor-check-{}.json",
            formatforge_lib::flavor::Flavor::current().id()
        )
    });
    let code = formatforge_lib::flavor_check::run_checks(&report).await;
    std::process::exit(code);
}
