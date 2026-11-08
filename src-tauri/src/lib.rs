pub mod after;
pub mod args;
pub mod avif;
pub mod compress;
pub mod containers;
pub mod device_presets;
pub mod download;
pub mod exif_write;
pub mod flavor;
pub mod flavor_check;
pub mod headless;
mod jobs;
pub mod preset;
pub mod probe;
mod setup;
pub mod setup_check;
pub mod tools;
pub mod updater;
pub mod window_chrome;

use std::path::PathBuf;
use std::sync::Arc;

use jobs::{AppState, Job, JobSpec};
use tauri::{AppHandle, Emitter, Manager, State};

/// 批次结束动作的真实执行器：打开目录用 opener 插件；关机默认禁用，
/// 只有 `FORMATFORGE_ALLOW_SHUTDOWN=1` 时才下发（构造命令的测试不受影响）。
fn after_hook(app: AppHandle) -> impl Fn(&str, &str) + Send + Sync + 'static {
    move |action, dir| match action {
        "open-folder" => {
            use tauri_plugin_opener::OpenerExt;
            let _ = app.opener().open_path(dir.to_string(), None::<&str>);
        }
        "shutdown" => {
            if after::shutdown_enabled() {
                let spec = after::shutdown_command(60);
                let _ = after::execute(&spec);
            }
        }
        _ => {}
    }
}

/// 把 Tauri 事件桥接到任务核心：核心代码不直接依赖 Tauri，便于无窗口驱动。
fn job_ctx(app: &AppHandle) -> jobs::Ctx {
    let state = app.state::<Arc<AppState>>().inner().clone();
    let handle_update = app.clone();
    let handle_remove = app.clone();
    jobs::Ctx::new(
        state,
        move |job| {
            let _ = handle_update.emit(jobs::JOB_EVENT, job);
        },
        move |id| {
            let _ = handle_remove.emit(jobs::JOB_REMOVED_EVENT, id);
        },
    )
    .with_after_action(after_hook(app.clone()))
}

#[tauri::command]
async fn resolve_tools(
    app: AppHandle,
    state: State<'_, Arc<AppState>>,
    custom_ffmpeg: Option<String>,
) -> Result<tools::ToolsInfo, String> {
    let info = tools::resolve(&app, custom_ffmpeg);
    *state.tools.lock().await = Some(info.clone());
    Ok(info)
}

#[tauri::command]
async fn probe_capabilities(
    app: AppHandle,
    state: State<'_, Arc<AppState>>,
    custom_ffmpeg: Option<String>,
    allow_gpl: bool,
    force: bool,
) -> Result<probe::Capabilities, String> {
    let info = tools::resolve(&app, custom_ffmpeg);
    *state.tools.lock().await = Some(info.clone());
    // GPL 变体恒为 true；LGPL 版只有 debug 构建能被设置项/环境变量追加打开。
    let allow_gpl = flavor::allow_gpl_effective(allow_gpl);
    let caps = probe::detect(&app, &info, allow_gpl, force).await?;
    {
        let mut inner = state.inner.lock().await;
        inner.caps = Some(caps.clone());
        inner.allow_gpl = allow_gpl;
    }
    Ok(caps)
}

#[tauri::command]
async fn probe_files(
    state: State<'_, Arc<AppState>>,
    paths: Vec<String>,
) -> Result<Vec<probe::MediaInfo>, String> {
    let ffprobe = {
        let tools = state.tools.lock().await;
        tools
            .as_ref()
            .and_then(|t| t.ffprobe.as_ref().map(|p| p.path.clone()))
    };
    let ffprobe = ffprobe.ok_or_else(|| "未找到 ffprobe，无法读取媒体信息".to_string())?;
    let mut result = Vec::with_capacity(paths.len());
    for path in paths {
        result.push(probe::probe_media(&ffprobe, &path).await);
    }
    Ok(result)
}

#[tauri::command]
fn list_device_presets() -> Vec<device_presets::DevicePreset> {
    device_presets::list().to_vec()
}

#[tauri::command]
async fn preview_frame(
    state: State<'_, Arc<AppState>>,
    path: String,
    time_sec: Option<f64>,
    max_edge: Option<u32>,
) -> Result<String, String> {
    let ffmpeg = {
        let tools = state.tools.lock().await;
        tools
            .as_ref()
            .and_then(|t| t.ffmpeg.as_ref().map(|p| p.path.clone()))
    };
    let ffmpeg = ffmpeg.ok_or_else(|| "未找到 ffmpeg，无法生成裁剪预览帧".to_string())?;
    jobs::preview_frame_data_url(
        &ffmpeg,
        &path,
        time_sec.unwrap_or(0.0),
        max_edge.unwrap_or(960),
    )
    .await
}

#[tauri::command]
async fn enqueue_jobs(app: AppHandle, specs: Vec<JobSpec>) -> Result<Vec<Job>, String> {
    jobs::enqueue(&job_ctx(&app), specs).await
}

#[tauri::command]
async fn list_jobs(app: AppHandle) -> Vec<Job> {
    jobs::list(&job_ctx(&app)).await
}

#[tauri::command]
async fn cancel_job(app: AppHandle, id: String) -> Result<(), String> {
    jobs::cancel(&job_ctx(&app), &id).await
}

#[tauri::command]
async fn retry_job(app: AppHandle, id: String) -> Result<(), String> {
    jobs::retry(&job_ctx(&app), &id).await
}

#[tauri::command]
async fn remove_finished_jobs(app: AppHandle) -> Vec<String> {
    jobs::remove_finished(&job_ctx(&app)).await
}

#[tauri::command]
async fn set_job_concurrency(app: AppHandle, value: usize) -> usize {
    jobs::set_concurrency(&job_ctx(&app), value).await
}

#[tauri::command]
async fn set_job_paused(app: AppHandle, paused: bool) -> bool {
    jobs::set_paused(&job_ctx(&app), paused).await
}

#[tauri::command]
fn export_preset(
    path: String,
    panels: serde_json::Value,
    output: serde_json::Value,
) -> Result<preset::PresetFile, String> {
    let file = preset::build(panels, output)?;
    preset::write(std::path::Path::new(&path), &file)?;
    Ok(file)
}

#[tauri::command]
fn import_preset(path: String) -> Result<preset::PresetFile, String> {
    preset::read(std::path::Path::new(&path))
}

#[tauri::command]
async fn get_job_paused(app: AppHandle) -> bool {
    jobs::is_paused(&job_ctx(&app)).await
}

#[tauri::command]
async fn set_after_action(app: AppHandle, action: String) -> String {
    jobs::set_after_action(&job_ctx(&app), &action).await
}

#[tauri::command]
fn open_folder(app: AppHandle, path: String) -> Result<(), String> {
    use tauri_plugin_opener::OpenerExt;
    app.opener()
        .open_path(path, None::<&str>)
        .map_err(|e| format!("打开目录失败：{e}"))
}

#[tauri::command]
async fn open_job_folder(app: AppHandle, id: String) -> Result<(), String> {
    use tauri_plugin_opener::OpenerExt;
    let job = jobs::list(&job_ctx(&app))
        .await
        .into_iter()
        .find(|j| j.id == id)
        .ok_or_else(|| "任务不存在".to_string())?;
    let dir = std::path::Path::new(&job.output)
        .parent()
        .map(|p| p.to_string_lossy().to_string())
        .ok_or_else(|| "无法定位输出目录".to_string())?;
    app.opener()
        .open_path(dir, None::<&str>)
        .map_err(|e| format!("打开目录失败：{e}"))
}

#[tauri::command]
async fn open_job_log(app: AppHandle, id: String) -> Result<(), String> {
    use tauri_plugin_opener::OpenerExt;
    let job = jobs::list(&job_ctx(&app))
        .await
        .into_iter()
        .find(|j| j.id == id)
        .ok_or_else(|| "任务不存在".to_string())?;
    let path = job
        .log_path
        .ok_or_else(|| "该任务还没有生成日志（日志在任务结束时写入）".to_string())?;
    app.opener()
        .open_path(path, None::<&str>)
        .map_err(|e| format!("打开日志失败：{e}"))
}

#[tauri::command]
async fn encode_avif(
    files: Vec<String>,
    params: avif::AvifParams,
    output_dir: Option<String>,
) -> Result<Vec<avif::AvifResult>, String> {
    tokio::task::spawn_blocking(move || {
        let dir = output_dir.as_deref().map(std::path::Path::new);
        avif::encode_batch(&files, &params, dir)
    })
    .await
    .map_err(|e| format!("AVIF 编码任务失败：{e}"))
}

#[tauri::command]
async fn write_exif(
    files: Vec<String>,
    fields: exif_write::ExifFields,
    output_dir: Option<String>,
) -> Result<Vec<exif_write::ExifWriteResult>, String> {
    tokio::task::spawn_blocking(move || {
        let dir = output_dir.as_deref().map(std::path::Path::new);
        exif_write::write_batch(&files, &fields, dir)
    })
    .await
    .map_err(|e| format!("EXIF 写入任务失败：{e}"))
}

#[tauri::command]
fn default_output_dir(app: AppHandle) -> String {
    app.path()
        .download_dir()
        .or_else(|_| app.path().home_dir())
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_else(|_| std::env::temp_dir().to_string_lossy().to_string())
}

#[tauri::command]
fn app_info() -> serde_json::Value {
    serde_json::json!({
        "version": env!("CARGO_PKG_VERSION"),
        "platform": std::env::consts::OS,
        "arch": std::env::consts::ARCH,
        "debug": cfg!(debug_assertions),
        "flavor": flavor::Flavor::current().id(),
    })
}

/// 当前发行变体：编译期确定，前端据此切换文案、默认下载源与许可提示。
#[tauri::command]
fn flavor_info() -> serde_json::Value {
    flavor::Flavor::current().info()
}

#[tauri::command]
fn save_diagnostics(app: AppHandle, text: String) -> Result<String, String> {
    let dir = app
        .path()
        .app_data_dir()
        .map_err(|e| format!("无法定位应用数据目录：{e}"))?
        .join("logs");
    std::fs::create_dir_all(&dir).map_err(|e| format!("创建日志目录失败：{e}"))?;
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let path = dir.join(format!("diagnostics-{stamp}.log"));
    std::fs::write(&path, text).map_err(|e| format!("写入日志失败：{e}"))?;
    Ok(path.to_string_lossy().to_string())
}

/// 崩溃可观测性：release 用 `panic = "abort"`，但 abort 之前 panic hook 仍会执行。
/// 这里把 panic 文本 + 尽力取到的 backtrace 追加到 `<appData>/crash.log` 并 fsync 落盘，
/// 再交给默认 hook 打印到 stderr。路径不依赖 `AppHandle`（启动早期就要生效）。
fn install_panic_hook() {
    use std::io::Write;

    let dir = std::env::var_os("APPDATA")
        .map(|p| PathBuf::from(p).join(flavor::Flavor::current().app_identifier()))
        .unwrap_or_else(std::env::temp_dir);
    let log_path = dir.join("crash.log");
    let default_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        default_hook(info);
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        let body = format!(
            "[{stamp}] flavor={} pid={} {}\n{}\n\n",
            flavor::Flavor::current().id(),
            std::process::id(),
            info,
            std::backtrace::Backtrace::force_capture()
        );
        if std::fs::create_dir_all(&dir).is_ok() {
            if let Ok(mut f) = std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(&log_path)
            {
                let _ = f.write_all(body.as_bytes());
                let _ = f.sync_all();
            }
        }
    }));
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    install_panic_hook();
    let mut context = tauri::generate_context!();
    window_chrome::preset_window_config(&mut context);
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .manage(Arc::new(AppState::default()))
        .setup(|app| {
            let handle = app.handle().clone();
            // 窗口创建后补齐标题栏颜色；底色与明暗已在创建前按上次设置写进窗口配置。
            window_chrome::apply_saved_chrome(&handle);
            let shared = app.state::<Arc<AppState>>().inner().clone();
            let boot_handle = handle.clone();
            let boot_state = shared.clone();
            tauri::async_runtime::spawn(async move {
                if let Ok(dir) = boot_handle.path().app_data_dir() {
                    *boot_state.log_dir.lock().await = Some(dir.join("logs"));
                }
                let info = tools::resolve(&boot_handle, None);
                *boot_state.tools.lock().await = Some(info);
                let _ = boot_handle.emit("tools:ready", ());
                // 清理中断的更新下载临时文件与过期 pending。
                updater::startup_cleanup(&boot_handle);
            });
            tauri::async_runtime::spawn(jobs::start_scheduler(
                jobs::Ctx::new(
                    shared,
                    {
                        let h = handle.clone();
                        move |job| {
                            let _ = h.emit(jobs::JOB_EVENT, job);
                        }
                    },
                    {
                        let h = handle.clone();
                        move |id| {
                            let _ = h.emit(jobs::JOB_REMOVED_EVENT, id);
                        }
                    },
                )
                .with_after_action(after_hook(handle.clone())),
            ));
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            resolve_tools,
            probe_capabilities,
            probe_files,
            list_device_presets,
            preview_frame,
            enqueue_jobs,
            list_jobs,
            cancel_job,
            retry_job,
            remove_finished_jobs,
            set_job_concurrency,
            set_job_paused,
            get_job_paused,
            export_preset,
            import_preset,
            set_after_action,
            open_folder,
            open_job_folder,
            open_job_log,
            write_exif,
            encode_avif,
            default_output_dir,
            app_info,
            flavor_info,
            save_diagnostics,
            setup::inspect_ffmpeg_file,
            setup::install_ffmpeg_files,
            download::tools_download_plan,
            download::download_tool,
            download::cancel_tool_download,
            updater::update_info,
            updater::update_check,
            updater::update_pending,
            updater::update_download,
            updater::update_install,
            updater::open_homepage,
            window_chrome::set_window_chrome
        ])
        .run(context)
        .expect("error while running tauri application");
}
