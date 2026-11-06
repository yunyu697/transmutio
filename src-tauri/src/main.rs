// Windows 发布版不弹控制台窗口；debug 保留便于查看日志。
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

/// 版本单一来源：`package.json` → `scripts/version-sync.mjs` 注入 `Cargo.toml` →
/// 编译期由 `CARGO_PKG_VERSION` 固化。与 tag、关于页（`lib.rs` 的 package_info）同源。
const VERSION: &str = env!("CARGO_PKG_VERSION");

fn main() {
    // 支持 `--version` / `-V`：不启动窗口，直接输出产品名、版本与发行变体。
    if std::env::args()
        .skip(1)
        .any(|a| a == "--version" || a == "-V")
    {
        print_version();
        return;
    }

    // 必须在 Tauri/WebView2 初始化之前迁移：%LOCALAPPDATA% 下的 WebView2 目录
    // 在窗口创建后就会被占用，届时无法原子替换。
    let flavor = formatforge_lib::flavor::Flavor::current();
    if let Some(paths) = formatforge_lib::migration::current_paths(flavor) {
        let _ = formatforge_lib::migration::run_prelaunch(&paths);
    }
    formatforge_lib::run()
}

fn print_version() {
    let flavor = formatforge_lib::flavor::Flavor::current();
    let text = format!("{} {} ({})\n", flavor.product_name(), VERSION, flavor.id());

    #[cfg(windows)]
    {
        // release 是 GUI 子系统，没有自己的控制台。附着到父进程控制台后直接写标准输出句柄，
        // 不走 CRT 的 stdout 流（后者在 AttachConsole 之前就已绑定，收不到新的控制台）。
        #[link(name = "kernel32")]
        extern "system" {
            fn AttachConsole(process_id: u32) -> i32;
            fn GetStdHandle(index: i32) -> *mut std::ffi::c_void;
            fn WriteFile(
                handle: *mut std::ffi::c_void,
                buf: *const u8,
                len: u32,
                written: *mut u32,
                overlapped: *mut std::ffi::c_void,
            ) -> i32;
        }
        const ATTACH_PARENT_PROCESS: u32 = 0xFFFF_FFFF;
        const STD_OUTPUT_HANDLE: i32 = -11;
        unsafe {
            AttachConsole(ATTACH_PARENT_PROCESS);
            let handle = GetStdHandle(STD_OUTPUT_HANDLE);
            if !handle.is_null() && handle as isize != -1 {
                let mut written: u32 = 0;
                WriteFile(
                    handle,
                    text.as_ptr(),
                    text.len() as u32,
                    &mut written,
                    std::ptr::null_mut(),
                );
            }
        }
    }

    #[cfg(not(windows))]
    {
        println!("{}", text.trim_end());
    }
}
