// Windows 发布版不弹控制台窗口；debug 保留便于查看日志。
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    // 必须在 Tauri/WebView2 初始化之前迁移：%LOCALAPPDATA% 下的 WebView2 目录
    // 在窗口创建后就会被占用，届时无法原子替换。
    let flavor = formatforge_lib::flavor::Flavor::current();
    if let Some(paths) = formatforge_lib::migration::current_paths(flavor) {
        let _ = formatforge_lib::migration::run_prelaunch(&paths);
    }
    formatforge_lib::run()
}
