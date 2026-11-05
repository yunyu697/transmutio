fn main() {
    let gpl = std::env::var("CARGO_FEATURE_GPL").is_ok();
    println!("cargo:rustc-env=FF_FLAVOR={}", if gpl { "gpl" } else { "lgpl" });
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-env-changed=CARGO_FEATURE_GPL");

    // 必须保留：tauri-build 负责生成并链接 Windows 资源（应用 manifest / 图标 / 版本信息）。
    // 少了这一步，release 二进制没有 .rsrc → 没有 comctl32 6.0 的 activation context，
    // 启动时 TaskDialogIndirect 无法解析，Windows 直接报「无法定位程序输入点」。
    tauri_build::build();
}
