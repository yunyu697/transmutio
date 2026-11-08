//! 标题栏与窗口底色跟随应用主题。
//!
//! - `set_theme`：让原生标题栏进入 light / dark（Windows 走 DWM 深色标题栏，其它平台同理由 Tauri 处理）；
//! - `set_background_color`：WebView 底色与页面 `--ff-bg` 一致，避免启动/切换时闪白；
//! - Windows 11 额外用 `DWMWA_CAPTION_COLOR` / `DWMWA_BORDER_COLOR` 把标题栏涂成页面顶部
//!   （顶栏渐变起点 `--ff-bg2`），消除标题栏与内容区之间的色差线。
//!
//! Windows 10 不支持 caption color（DWM 返回失败），此时只保留 light/dark 标题栏；
//! 不做 `decorations:false` 的伪标题栏，保留系统缩放、贴边与快捷键行为。

#[cfg(windows)]
use std::os::raw::c_void;

use std::path::PathBuf;

use tauri::{Manager, Theme, WebviewWindow};

/// 首次启动（还没有记录）时的兜底：与默认的深色 + 石墨一致。
const DEFAULT_THEME: &str = "dark";
const DEFAULT_BG: &str = "#0A0E14";
const DEFAULT_CAPTION: &str = "#0D131B";

fn parse_hex(text: &str) -> Result<(u8, u8, u8), String> {
    let hex = text.trim().trim_start_matches('#');
    if hex.len() != 6 {
        return Err(format!("颜色必须是 #RRGGBB：{text}"));
    }
    let r = u8::from_str_radix(&hex[0..2], 16).map_err(|e| e.to_string())?;
    let g = u8::from_str_radix(&hex[2..4], 16).map_err(|e| e.to_string())?;
    let b = u8::from_str_radix(&hex[4..6], 16).map_err(|e| e.to_string())?;
    Ok((r, g, b))
}

/// 主题切换时由前端调用：`bg` = 页面底色，`caption` = 顶栏渐变起点色。
#[tauri::command]
pub fn set_window_chrome(
    window: WebviewWindow,
    theme: String,
    bg: String,
    caption: String,
) -> Result<(), String> {
    apply(&window, &theme, &bg, &caption)?;
    save_chrome(window.app_handle(), &theme, &bg, &caption);
    Ok(())
}

/// 启动时在窗口创建之前套用上次的主题：直接改窗口配置里的底色与明暗，
/// 这样窗口第一次出现就是正确的颜色（浅色用户不会再先看到约 0.8s 的深色空窗）。
pub fn preset_window_config<R: tauri::Runtime>(context: &mut tauri::Context<R>) {
    let (theme, bg, _caption) = read_chrome_file(early_chrome_path());
    let Ok((r, g, b)) = parse_hex(&bg) else {
        return;
    };
    let Some(window) = context.config_mut().app.windows.first_mut() else {
        return;
    };
    window.background_color = Some(tauri::utils::config::Color(r, g, b, 255));
    window.theme = Some(if theme == "light" {
        Theme::Light
    } else {
        Theme::Dark
    });
}

/// 窗口创建后补齐标题栏颜色（配置里只能改底色与明暗）。
pub fn apply_saved_chrome(app: &tauri::AppHandle) {
    let Some(window) = app.get_webview_window("main") else {
        return;
    };
    let (theme, bg, caption) = read_chrome_file(chrome_path(app));
    let _ = apply(&window, &theme, &bg, &caption);
}

fn apply(window: &WebviewWindow, theme: &str, bg: &str, caption: &str) -> Result<(), String> {
    let mode = match theme {
        "light" => Theme::Light,
        "dark" => Theme::Dark,
        other => return Err(format!("未知主题：{other}")),
    };
    window
        .set_theme(Some(mode))
        .map_err(|e| format!("设置窗口主题失败：{e}"))?;
    let (r, g, b) = parse_hex(bg)?;
    window
        .set_background_color(Some(tauri::window::Color(r, g, b, 255)))
        .map_err(|e| format!("设置窗口底色失败：{e}"))?;
    #[cfg(windows)]
    {
        let (cr, cg, cb) = parse_hex(caption)?;
        apply_caption_color(window, (cr, cg, cb))?;
    }
    #[cfg(not(windows))]
    {
        let _ = caption;
    }
    Ok(())
}

fn chrome_path(app: &tauri::AppHandle) -> Option<PathBuf> {
    app.path()
        .app_config_dir()
        .ok()
        .map(|dir| dir.join("window-chrome.txt"))
}

/// 记住最近一次的窗口配色，供下次启动在首帧之前使用；失败不影响主题切换本身。
fn save_chrome(app: &tauri::AppHandle, theme: &str, bg: &str, caption: &str) {
    let Some(path) = chrome_path(app) else {
        return;
    };
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let _ = std::fs::write(path, format!("{theme}\n{bg}\n{caption}\n"));
}

/// 早于 `AppHandle` 的路径：Windows 的配置文件目录就是 `%APPDATA%\<identifier>`。
fn early_chrome_path() -> Option<PathBuf> {
    std::env::var_os("APPDATA").map(|dir| {
        PathBuf::from(dir)
            .join(crate::flavor::Flavor::current().app_identifier())
            .join("window-chrome.txt")
    })
}

fn read_chrome_file(path: Option<PathBuf>) -> (String, String, String) {
    let fallback = (
        DEFAULT_THEME.to_string(),
        DEFAULT_BG.to_string(),
        DEFAULT_CAPTION.to_string(),
    );
    let Some(path) = path else {
        return fallback;
    };
    let Ok(text) = std::fs::read_to_string(path) else {
        return fallback;
    };
    let mut lines = text.lines().map(str::trim);
    let theme = lines.next().unwrap_or_default().to_string();
    let bg = lines.next().unwrap_or_default().to_string();
    let caption = lines.next().unwrap_or_default().to_string();
    if !matches!(theme.as_str(), "light" | "dark")
        || parse_hex(&bg).is_err()
        || parse_hex(&caption).is_err()
    {
        return fallback;
    }
    (theme, bg, caption)
}

#[cfg(windows)]
fn apply_caption_color(window: &WebviewWindow, rgb: (u8, u8, u8)) -> Result<(), String> {
    use windows_sys::Win32::Graphics::Dwm::{
        DwmSetWindowAttribute, DWMWA_BORDER_COLOR, DWMWA_CAPTION_COLOR,
    };

    let hwnd = window.hwnd().map_err(|e| format!("取窗口句柄失败：{e}"))?;
    let (r, g, b) = rgb;
    let color_ref: u32 = ((b as u32) << 16) | ((g as u32) << 8) | (r as u32);
    unsafe {
        let raw = hwnd.0 as *mut c_void;
        let caption = DwmSetWindowAttribute(
            raw,
            DWMWA_CAPTION_COLOR as u32,
            &color_ref as *const u32 as *const c_void,
            4,
        );
        // 边框同色：去掉系统主题色描边，避免出现 1px 色差线。
        let _ = DwmSetWindowAttribute(
            raw,
            DWMWA_BORDER_COLOR as u32,
            &color_ref as *const u32 as *const c_void,
            4,
        );
        // Win10 上 caption color 不受支持，静默保留 light/dark 标题栏。
        let _ = caption;
    }
    Ok(())
}
