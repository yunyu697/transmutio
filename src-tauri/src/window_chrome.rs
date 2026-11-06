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

use tauri::{Theme, WebviewWindow};

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
    let mode = match theme.as_str() {
        "light" => Theme::Light,
        "dark" => Theme::Dark,
        other => return Err(format!("未知主题：{other}")),
    };
    window
        .set_theme(Some(mode))
        .map_err(|e| format!("设置窗口主题失败：{e}"))?;
    let (r, g, b) = parse_hex(&bg)?;
    window
        .set_background_color(Some(tauri::window::Color(r, g, b, 255)))
        .map_err(|e| format!("设置窗口底色失败：{e}"))?;
    #[cfg(windows)]
    {
        let (cr, cg, cb) = parse_hex(&caption)?;
        apply_caption_color(&window, (cr, cg, cb))?;
    }
    #[cfg(not(windows))]
    {
        let _ = caption;
    }
    Ok(())
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
