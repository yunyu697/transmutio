use crate::{args, containers, pdf, probe, tools};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{HashMap, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::{Child, Command};
use tokio::sync::{Mutex, Notify};

pub const DEFAULT_CONCURRENCY: usize = 4;
pub const JOB_EVENT: &str = "job:update";
pub const JOB_REMOVED_EVENT: &str = "job:removed";

static JOB_SEQ: AtomicU64 = AtomicU64::new(0);

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

fn new_job_id() -> String {
    let seq = JOB_SEQ.fetch_add(1, Ordering::SeqCst);
    format!("ff-{}-{}", now_ms(), seq)
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Job {
    pub id: String,
    /// video-mp4 | video-merge | video-mux | video-gif | audio-mp3 | audio-aac | image
    pub kind: String,
    pub input: String,
    /// 多输入任务（合并 / 混流）的完整输入列表；单输入任务为空。
    pub inputs: Vec<String>,
    pub input_name: String,
    /// 目标输出（完成时若冲突会自动改名，这里会同步更新）
    pub output: String,
    pub params: Value,
    /// queued | running | done | failed | cancelled
    pub status: String,
    /// 0..1
    pub progress: f64,
    pub speed: Option<String>,
    pub eta_sec: Option<f64>,
    pub message: Option<String>,
    /// hw-unavailable | hw-memory | invalid-params | encoder-missing | input | output | unknown
    pub error_kind: Option<String>,
    pub encoder_used: Option<String>,
    pub fallback_note: Option<String>,
    pub attempts: u32,
    /// suffix | skip | overwrite —— 该任务实际生效的输出命名冲突策略。
    pub conflict: String,
    /// 每任务诊断日志（完整 ffmpeg stderr + 参数），仅在配置了日志目录时生成。
    pub log_path: Option<String>,
    pub duration_sec: Option<f64>,
    pub output_size: Option<u64>,
    /// 压缩任务的目标字节数（quality 模式为空）。
    pub target_bytes: Option<u64>,
    /// 压缩任务是否达到目标大小；quality 模式为空。
    pub target_met: Option<bool>,
    /// 压缩实际尝试次数（1–8）。
    pub attempts_used: Option<u32>,
    /// 压缩最终采用的质量（1–100）。
    pub quality_used: Option<u32>,
    pub created_at: i64,
    pub started_at: Option<i64>,
    pub finished_at: Option<i64>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JobSpec {
    pub kind: String,
    /// 单输入任务的主输入；多输入任务（合并）也用它表示第一个文件。
    pub input: String,
    /// 多输入任务（合并 / 混流）的其余输入。
    #[serde(default)]
    pub inputs: Vec<String>,
    pub output_dir: String,
    pub output_stem: Option<String>,
    /// 命名模板，占位符：{name} {date} {index} {ext}；为空等价于 "{name}"。
    #[serde(default)]
    pub name_template: Option<String>,
    /// 输出冲突策略：suffix（默认，加序号） | skip | overwrite。
    #[serde(default)]
    pub conflict: Option<String>,
    pub params: Value,
}

#[derive(Debug)]
pub struct RunningChild {
    handle: tokio::task::JoinHandle<()>,
    pid: Option<u32>,
}

pub struct Inner {
    pub jobs: Vec<Job>,
    pub queue: VecDeque<String>,
    pub running: usize,
    pub concurrency: usize,
    /// 队列级暂停：不再调度新任务，已在运行的任务会继续跑完。
    pub paused: bool,
    /// 本轮批次是否已经触发过「完成动作」，避免重复触发。
    pub drained: bool,
    pub children: HashMap<String, RunningChild>,
    pub cancel_requested: std::collections::HashSet<String>,
    pub caps: Option<probe::Capabilities>,
    pub allow_gpl: bool,
}

impl Default for Inner {
    fn default() -> Self {
        Self {
            jobs: Vec::new(),
            queue: VecDeque::new(),
            running: 0,
            concurrency: DEFAULT_CONCURRENCY,
            paused: false,
            drained: true,
            children: HashMap::new(),
            cancel_requested: Default::default(),
            caps: None,
            allow_gpl: false,
        }
    }
}

pub struct AppState {
    pub inner: Mutex<Inner>,
    pub notify: Notify,
    pub tools: Mutex<Option<tools::ToolsInfo>>,
    /// 任务日志目录；为空时不落盘（无窗口检查可显式指定）。
    pub log_dir: Mutex<Option<PathBuf>>,
    /// 批次全部结束后的动作：none | open-folder | shutdown。
    pub after_action: Mutex<String>,
    /// 用户在设置里指定的 PDF 工具目录（仅影响 pdftoppm 查找）。
    pub pdf_tools_dir: Mutex<Option<String>>,
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            inner: Mutex::new(Inner::default()),
            notify: Notify::new(),
            tools: Mutex::new(None),
            log_dir: Mutex::new(None),
            after_action: Mutex::new("none".into()),
            pdf_tools_dir: Mutex::new(None),
        }
    }
}

/// 任务调度与 Tauri 解耦：只依赖共享状态 + 两个事件回调。
/// 这样真实转换链路可以在无窗口的 headless 检查里直接驱动（同一份代码，不是替身）。
#[derive(Clone)]
pub struct Ctx {
    pub state: Arc<AppState>,
    emit: Arc<dyn Fn(Job) + Send + Sync>,
    emit_removed: Arc<dyn Fn(String) + Send + Sync>,
    /// 批次全部结束后的动作回调：`(action, output_dir)`。
    /// 应用侧注入「打开目录 / 构造关机命令」；无窗口检查注入记录器，绝不真正执行。
    after: Arc<dyn Fn(&str, &str) + Send + Sync>,
}

impl Ctx {
    pub fn new(
        state: Arc<AppState>,
        emit: impl Fn(Job) + Send + Sync + 'static,
        emit_removed: impl Fn(String) + Send + Sync + 'static,
    ) -> Self {
        Self {
            state,
            emit: Arc::new(emit),
            emit_removed: Arc::new(emit_removed),
            after: Arc::new(|_, _| {}),
        }
    }

    /// 注入「完成后动作」执行器（打开目录 / 关机）。便于无窗口测试断言，不触碰真实系统。
    pub fn with_after_action(mut self, after: impl Fn(&str, &str) + Send + Sync + 'static) -> Self {
        self.after = Arc::new(after);
        self
    }

    /// 无界面场景：事件被丢弃，状态仍可从 `list()` 读取。
    pub fn silent(state: Arc<AppState>) -> Self {
        Self::new(state, |_| {}, |_| {})
    }

    fn publish(&self, job: &Job) {
        (self.emit)(job.clone());
    }

    fn publish_removed(&self, id: &str) {
        (self.emit_removed)(id.to_string());
    }
}

fn emit_job(ctx: &Ctx, job: &Job) {
    ctx.publish(job);
}

async fn set_job<F: FnOnce(&mut Job)>(ctx: &Ctx, id: &str, f: F) -> Option<Job> {
    let state = &ctx.state;
    let updated = {
        let mut inner = state.inner.lock().await;
        match inner.jobs.iter_mut().find(|j| j.id == id) {
            Some(job) => {
                f(job);
                Some(job.clone())
            }
            None => None,
        }
    };
    if let Some(job) = &updated {
        emit_job(ctx, job);
    }
    updated
}

async fn get_job(ctx: &Ctx, id: &str) -> Option<Job> {
    let state = &ctx.state;
    let inner = state.inner.lock().await;
    inner.jobs.iter().find(|j| j.id == id).cloned()
}

/// 从任务参数里读出分割参数（前端把 split 对象嵌在 VideoParams 内）。
fn split_params(params: &Value) -> args::SplitParams {
    params
        .get("split")
        .and_then(|value| serde_json::from_value::<args::SplitParams>(value.clone()).ok())
        .unwrap_or_default()
}

/// 秒数展示：最多 3 位小数并去掉尾随 0（0.000 → 0，2.500 → 2.5）。
fn fmt_sec_short(value: f64) -> String {
    let text = format!("{value:.3}");
    let trimmed = text.trim_end_matches('0').trim_end_matches('.');
    if trimmed.is_empty() {
        "0".into()
    } else {
        trimmed.to_string()
    }
}

fn kind_ext(kind: &str, params: &Value) -> String {
    match kind {
        // 音频格式由 params.format 决定；kind 仅用于任务分类（历史 id 保持兼容）。
        "audio-mp3" | "audio-aac" => {
            let format = params
                .get("format")
                .and_then(Value::as_str)
                .unwrap_or(if kind == "audio-aac" { "aac" } else { "mp3" });
            containers::audio_ext(format).unwrap_or("mp3").to_string()
        }
        // 视频容器由 params.container 决定（mp4 / mkv / webm / mov / ts / avi）。
        "video-mp4" => {
            let container = params
                .get("container")
                .and_then(Value::as_str)
                .unwrap_or("mp4");
            containers::container_ext(container)
                .unwrap_or("mp4")
                .to_string()
        }
        "video-gif" | "image-gif" => "gif".into(),
        // 分割输出容器取自 params.split.container（前端同时回填 params.container）。
        "video-split" => {
            let split = split_params(params);
            let container = if split.container.trim().is_empty() {
                params
                    .get("container")
                    .and_then(Value::as_str)
                    .unwrap_or("mp4")
            } else {
                split.container.as_str()
            };
            containers::container_ext(container)
                .unwrap_or("mp4")
                .to_string()
        }
        "pdf-merge" | "pdf-split" => "pdf".into(),
        "pdf-image" => {
            let format = params
                .get("format")
                .and_then(Value::as_str)
                .unwrap_or("png");
            if format == "jpg" {
                "jpg".into()
            } else {
                "png".into()
            }
        }
        "video-frames" => {
            let format = params
                .get("format")
                .and_then(Value::as_str)
                .unwrap_or("png");
            if format == "jpg" {
                "jpg".into()
            } else {
                "png".into()
            }
        }
        "image-collage" => {
            let format = params
                .get("format")
                .and_then(Value::as_str)
                .unwrap_or("png");
            match format {
                "jpg" => "jpg".into(),
                "webp" => "webp".into(),
                _ => "png".into(),
            }
        }
        "image" | "image-compress" => {
            let p: args::ImageParams = serde_json::from_value(params.clone()).unwrap_or_default();
            args::image_ext(if p.format.is_empty() {
                "jpg"
            } else {
                &p.format
            })
            .to_string()
        }
        _ => "mp4".into(),
    }
}

fn file_stem(path: &str) -> String {
    Path::new(path)
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "output".into())
}

/// 冲突永不覆盖：`name.ext`、`name (1).ext`、`name (2).ext`…
/// `used` 是已入队任务占用的输出路径，避免两个并发任务抢同一个新文件名。
fn unique_output(dir: &Path, stem: &str, ext: &str, used: &[PathBuf]) -> PathBuf {
    let mut index = 0usize;
    loop {
        let candidate = if index == 0 {
            dir.join(format!("{stem}.{ext}"))
        } else {
            dir.join(format!("{stem} ({index}).{ext}"))
        };
        if !candidate.exists() && !used.iter().any(|path| path == &candidate) {
            return candidate;
        }
        index += 1;
        if index >= 1000 {
            return dir.join(format!("{stem} ({index}).{ext}"));
        }
    }
}

fn temp_path(dir: &Path, id: &str, ext: &str) -> PathBuf {
    dir.join(format!(".ff-tmp-{id}.part.{ext}"))
}

/// 冲突策略归一化：未知值回落到最安全的 suffix。
fn conflict_policy(raw: Option<&str>) -> &'static str {
    match raw.map(|s| s.trim().to_ascii_lowercase()).as_deref() {
        Some("skip") => "skip",
        Some("overwrite") => "overwrite",
        _ => "suffix",
    }
}

/// concat demuxer 会把清单里的相对路径解析成「相对于清单文件所在目录」，
/// 而清单就写在输出目录里，直接用原始相对路径会变成 output/samples/...。
/// 这里统一补成绝对路径，保证与调用方的工作目录一致。
fn absolute_paths(paths: &[String]) -> Vec<String> {
    let cwd = std::env::current_dir().ok();
    paths
        .iter()
        .map(|p| {
            let path = Path::new(p);
            if path.is_absolute() {
                return p.clone();
            }
            match &cwd {
                Some(base) => base.join(path).to_string_lossy().to_string(),
                None => p.clone(),
            }
        })
        .collect()
}

/// UTC 日期 YYYYMMDD。命名模板只需要稳定可排序的日期，不依赖本地时区。
fn utc_date() -> String {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0) as i64;
    let days = secs / 86_400;
    // Howard Hinnant 的 days-from-civil 逆运算
    let z = days + 719_468;
    let era = z / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = if m <= 2 { y + 1 } else { y };
    format!("{year:04}{m:02}{d:02}")
}

/// 渲染命名模板（只渲染文件名主体，扩展名由任务类型单独追加）。
/// 未知占位符原样保留，避免把用户写的花括号悄悄吃掉。
pub fn render_name(
    template: &str,
    source_stem: &str,
    date: &str,
    index: usize,
    ext: &str,
) -> String {
    let template = template.trim();
    let template = if template.is_empty() {
        "{name}"
    } else {
        template
    };
    let rendered = template
        .replace("{name}", source_stem)
        .replace("{date}", date)
        .replace("{index}", &index.to_string())
        .replace("{ext}", ext);
    // 文件名里不允许出现路径分隔符，换成下划线；Windows 保留字符也一并替换。
    let cleaned: String = rendered
        .chars()
        .map(|c| match c {
            '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' => '_',
            _ => c,
        })
        .collect();
    let cleaned = cleaned.trim().trim_matches('.').to_string();
    if cleaned.is_empty() {
        source_stem.to_string()
    } else {
        cleaned
    }
}

/// 显式覆盖发布：先把旧文件挪到备份名，再发布新文件；失败时把备份挪回原位。
/// 只有任务显式选择 overwrite 策略时才会走到这里。
fn rename_overwrite(temp: &Path, target: &Path) -> std::io::Result<u64> {
    let size = std::fs::metadata(temp)?.len();
    let backup = target.with_extension(format!(
        "{}ffbak",
        target
            .extension()
            .map(|e| format!("{}.", e.to_string_lossy()))
            .unwrap_or_default()
    ));
    let had_existing = target.exists();
    if had_existing {
        std::fs::rename(target, &backup)?;
    }
    match std::fs::rename(temp, target) {
        Ok(()) => {
            if had_existing {
                let _ = std::fs::remove_file(&backup);
            }
            Ok(size)
        }
        Err(err) => {
            if had_existing {
                let _ = std::fs::rename(&backup, target);
            }
            Err(err)
        }
    }
}

/// 取消时子进程可能还没完全退出（Windows 上文件句柄未释放），删除临时文件需要短暂重试。
async fn remove_temp_file(path: &Path) {
    for attempt in 0..12 {
        if !path.exists() {
            return;
        }
        if std::fs::remove_file(path).is_ok() {
            return;
        }
        if attempt < 11 {
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    }
}

/// 入队前的参数校验。任何非法输入都直接拒绝，不创建「点进去才失败」的任务。
fn precheck_spec(
    kind: &str,
    inputs: &[String],
    params: &Value,
    pdf_dir: Option<&str>,
) -> Result<(), String> {
    match kind {
        "video-mp4" => {
            let p: args::VideoParams = serde_json::from_value(params.clone())
                .map_err(|e| format!("视频参数格式错误：{e}"))?;
            args::validate_speed(p.speed)?;
            if args::speed_or_default(p.speed) != 1.0 && p.audio_mode == "copy" {
                return Err("变速会改变音轨时长，不能与原样复制音轨同时使用：请改为按容器转码或移除音轨".into());
            }
            args::validate_watermark(&p.watermark)?;
            args::validate_subtitle(&p.subtitle, &p.container, p.speed)?;
            args::validate_volume(&p.volume)?;
            if p.audio_mode == "copy" && p.volume.enabled && !p.volume.mute {
                return Err(
                    "音量增益需要对音轨做滤镜处理，但当前是「原样复制」（-c:a copy），无法应用滤镜：请改为按容器转码，或关闭音量增益"
                        .into(),
                );
            }
            containers::validate_video(&p.container, &p.codec)
        }
        "audio-mp3" | "audio-aac" => {
            let p: args::AudioParams = serde_json::from_value(params.clone())
                .map_err(|e| format!("音频参数格式错误：{e}"))?;
            args::validate_speed(p.speed)?;
            args::validate_volume(&p.volume)?;
            args::validate_audio_volume(&p)?;
            containers::validate_audio(&p.format)
        }
        "video-split" => {
            let split = split_params(params);
            // precheck 阶段拿不到源时长，先用 INFINITY 跳过上界；enqueue 探测后再补校。
            args::validate_split(&split, f64::INFINITY)
        }
        "image-gif" => {
            let p: args::ImageGifParams = serde_json::from_value(params.clone())
                .map_err(|e| format!("GIF 参数格式错误：{e}"))?;
            if !(1..=30).contains(&p.fps) {
                return Err(format!("GIF 帧率必须在 1–30 之间（当前 {}）", p.fps));
            }
            if inputs.len() < 2 {
                return Err("图片转 GIF 至少需要 2 张图片：请多选图片后再入队".into());
            }
            Ok(())
        }
        "video-merge" | "video-mux" | "video-gif" => {
            if inputs.is_empty() {
                return Err("缺少输入文件".into());
            }
            Ok(())
        }
        "image" => {
            let p: args::ImageParams = serde_json::from_value(params.clone())
                .map_err(|e| format!("图片参数格式错误：{e}"))?;
            args::validate_image_crop(&p)?;
            args::validate_watermark(&p.watermark)
        }
        "image-compress" => {
            let p: args::ImageParams = serde_json::from_value(params.clone())
                .map_err(|e| format!("压缩参数格式错误：{e}"))?;
            args::validate_image_crop(&p)?;
            args::validate_watermark(&p.watermark)?;
            args::validate_compress(&p)
        }
        "image-seq-video" => {
            let p: args::SeqParams = serde_json::from_value(params.clone())
                .map_err(|e| format!("序列参数格式错误：{e}"))?;
            if !(1..=60).contains(&p.fps) {
                return Err("帧率必须在 1..60 之间".into());
            }
            args::validate_sequence(inputs)?;
            Ok(())
        }
        "video-frames" => {
            let p: args::FramesParams = serde_json::from_value(params.clone())
                .map_err(|e| format!("抽帧参数格式错误：{e}"))?;
            if !(1..=60).contains(&p.fps) {
                return Err("抽帧帧率必须在 1..60 之间".into());
            }
            if !["png", "jpg"].contains(&p.format.as_str()) {
                return Err(format!("不支持的图片格式「{}」，可选 png / jpg", p.format));
            }
            Ok(())
        }
        "image-collage" => {
            let p: args::CollageParams = serde_json::from_value(params.clone())
                .map_err(|e| format!("拼图参数格式错误：{e}"))?;
            if !(1..=8).contains(&p.cols) {
                return Err("网格列数必须在 1..8 之间".into());
            }
            if p.rows > 8 {
                return Err("网格行数必须在 0..8 之间（0 表示自动）".into());
            }
            if p.gap > 200 {
                return Err("间距不能超过 200 像素".into());
            }
            if !["png", "jpg", "webp"].contains(&p.format.as_str()) {
                return Err(format!("不支持的拼图格式「{}」", p.format));
            }
            // 用真实的构建函数做一次干跑，确保网格容量/颜色/单元尺寸都合法。
            args::build_collage(inputs, "precheck.out", &p).map(|_| ())
        }
        "pdf-merge" => {
            if inputs.len() < 2 {
                return Err("PDF 合并至少需要 2 个文件".into());
            }
            for input in inputs {
                let ext = Path::new(input)
                    .extension()
                    .map(|e| e.to_string_lossy().to_ascii_lowercase())
                    .unwrap_or_default();
                if ext != "pdf" {
                    return Err(format!("{input} 不是 PDF 文件"));
                }
            }
            Ok(())
        }
        "pdf-split" => {
            let mode = params.get("mode").and_then(Value::as_str).unwrap_or("per-page");
            if mode != "per-page" && mode != "ranges" {
                return Err(format!("不支持的拆分模式「{mode}」，可选 per-page / ranges"));
            }
            let total = pdf::page_count(&inputs[0])?;
            if mode == "ranges" {
                let ranges = params.get("ranges").and_then(Value::as_str).unwrap_or("");
                pdf::parse_ranges(ranges, total)?;
            }
            Ok(())
        }
        "pdf-image" => {
            let format = params.get("format").and_then(Value::as_str).unwrap_or("png");
            if !["png", "jpg"].contains(&format) {
                return Err(format!("不支持的图片格式「{format}」，可选 png / jpg"));
            }
            let dpi = params.get("dpi").and_then(Value::as_u64).unwrap_or(96);
            if !(36..=600).contains(&dpi) {
                return Err("DPI 必须在 36..600 之间".into());
            }
            let tools = crate::pdf_tools::resolve(pdf_dir);
            if !tools.available {
                return Err(tools.reason.unwrap_or_else(|| "未找到 pdftoppm".into()));
            }
            // 提前读一遍页数，损坏/加密文件在入队阶段就拒绝。
            pdf::page_count(&inputs[0])?;
            Ok(())
        }
        other => Err(format!(
            "未知任务类型「{other}」；支持 video-mp4 / video-split / video-merge / video-mux / video-gif / image-gif / audio-mp3 / audio-aac / image / image-compress / image-seq-video / video-frames / image-collage / pdf-merge / pdf-split / pdf-image"
        )),
    }
}

pub async fn enqueue(ctx: &Ctx, specs: Vec<JobSpec>) -> Result<Vec<Job>, String> {
    let state = &ctx.state;
    let (ffprobe, has_ffmpeg) = {
        let tools = state.tools.lock().await;
        (
            tools
                .as_ref()
                .and_then(|t| t.ffprobe.as_ref().map(|p| p.path.clone())),
            tools
                .as_ref()
                .and_then(|t| t.ffmpeg.as_ref().map(|p| p.path.clone())),
        )
    };
    if has_ffmpeg.is_none() {
        return Err("未找到 ffmpeg，无法开始转换。请在设置中指定可执行文件或安装 sidecar。".into());
    }

    let date = utc_date();
    let mut created: Vec<Job> = Vec::new();
    let mut queued_any = false;

    for (index, spec) in specs.into_iter().enumerate() {
        // 多输入任务：主输入 + 其余输入去重后组成完整列表。
        let mut inputs: Vec<String> = if spec.inputs.is_empty() {
            vec![spec.input.clone()]
        } else {
            spec.inputs.clone()
        };
        if !inputs.iter().any(|p| p == &spec.input) {
            inputs.insert(0, spec.input.clone());
        }

        let pdf_dir = { state.pdf_tools_dir.lock().await.clone() };
        precheck_spec(&spec.kind, &inputs, &spec.params, pdf_dir.as_deref())?;

        let mut info = match ffprobe.as_deref() {
            Some(probe_bin) => probe::probe_media(probe_bin, &spec.input).await,
            None => probe::MediaInfo {
                path: spec.input.clone(),
                ..Default::default()
            },
        };
        if info
            .error
            .as_deref()
            .map(|e| e.contains("0 字节"))
            .unwrap_or(false)
        {
            return Err(format!("{}：文件为空，已拒绝入队", info.name));
        }
        let mut total_duration = info.duration_sec;

        // 图片自定义裁剪需要在入队阶段对照源图尺寸校验，避免任务跑起来才失败。
        if spec.kind == "image" || spec.kind == "image-compress" {
            let p: args::ImageParams =
                serde_json::from_value(spec.params.clone()).unwrap_or_default();
            if p.crop_mode == "custom" {
                if let (Some(width), Some(height)) = (info.width, info.height) {
                    args::validate_custom_crop(
                        p.crop_x, p.crop_y, p.crop_w, p.crop_h, width, height,
                    )?;
                }
            }
        }

        // 序列任务：探测每一张图的尺寸，用于尺寸统一策略与时长估算。
        if spec.kind == "image-seq-video" {
            let p: args::SeqParams = serde_json::from_value(spec.params.clone())
                .map_err(|e| format!("序列参数格式错误：{e}"))?;
            let mut dims = Vec::with_capacity(inputs.len());
            for (index, input) in inputs.iter().enumerate() {
                let part = if index == 0 {
                    info.clone()
                } else {
                    match ffprobe.as_deref() {
                        Some(probe_bin) => probe::probe_media(probe_bin, input).await,
                        None => probe::MediaInfo {
                            path: input.clone(),
                            ..Default::default()
                        },
                    }
                };
                match (part.width, part.height) {
                    (Some(w), Some(h)) if w > 0 && h > 0 => dims.push((w, h)),
                    _ => {
                        return Err(format!(
                            "无法读取图片尺寸：{}（需要可被 ffprobe 识别的 PNG/JPEG/WebP）",
                            input
                        ))
                    }
                }
            }
            args::seq_target_size(&p, &dims)?;
            let fps = p.fps.max(1);
            total_duration = Some(inputs.len() as f64 / fps as f64);
        }

        // 分割任务：探测到源时长后补校上界，并把任务时长改为本段请求时长。
        if spec.kind == "video-split" {
            let split = split_params(&spec.params);
            let source_duration = info.duration_sec.unwrap_or(f64::INFINITY);
            args::validate_split(&split, source_duration)?;
            total_duration = Some((split.end_sec - split.start_sec).max(0.0));
        }

        // 合并任务的总时长/进度依赖全部输入，这里一次性探测并累加。
        if spec.kind == "video-merge" {
            let mut sum = 0.0f64;
            let mut all_known = true;
            for input in &inputs {
                let part = match ffprobe.as_deref() {
                    Some(probe_bin) if input != &spec.input => {
                        probe::probe_media(probe_bin, input).await
                    }
                    _ => info.clone(),
                };
                match part.duration_sec {
                    Some(d) => sum += d,
                    None => all_known = false,
                }
            }
            total_duration = if all_known { Some(sum) } else { None };
        }
        if inputs.len() > 1 {
            info.name = format!("{} +{}", info.name, inputs.len() - 1);
        }
        // 分割的每段是独立任务：任务名带上段时间区间，失败时能直接定位是哪一段。
        if spec.kind == "video-split" {
            let split = split_params(&spec.params);
            info.name = format!(
                "{}（{}s–{}s）",
                info.name,
                fmt_sec_short(split.start_sec),
                fmt_sec_short(split.end_sec)
            );
        }

        let dir = PathBuf::from(&spec.output_dir);
        std::fs::create_dir_all(&dir)
            .map_err(|e| format!("无法创建输出目录 {}：{e}", dir.display()))?;

        let ext = kind_ext(&spec.kind, &spec.params);
        let source_stem = spec
            .output_stem
            .clone()
            .filter(|s| !s.trim().is_empty())
            .unwrap_or_else(|| file_stem(&spec.input));
        let stem = render_name(
            spec.name_template.as_deref().unwrap_or(""),
            &source_stem,
            &date,
            index + 1,
            &ext,
        );
        // 模板里可能显式写了 {ext}，此时不要再追加一次扩展名。
        let stem = if stem
            .to_ascii_lowercase()
            .ends_with(&format!(".{}", ext.to_ascii_lowercase()))
        {
            stem[..stem.len() - ext.len() - 1].to_string()
        } else {
            stem
        };
        // 分割：每段追加 -partN，避免多段彼此覆盖（冲突策略仍按 suffix/skip/overwrite 生效）。
        let stem = if spec.kind == "video-split" {
            format!(
                "{stem}-part{}",
                split_params(&spec.params).segment_index + 1
            )
        } else {
            stem
        };
        let policy = conflict_policy(spec.conflict.as_deref());
        let id = new_job_id();

        // 在同一把锁内完成「选取输出 + 入队」，避免并发 enqueue 抢到同一个文件名。
        let mut job = Job {
            id: id.clone(),
            kind: spec.kind.clone(),
            input: spec.input.clone(),
            inputs: inputs.clone(),
            input_name: info.name.clone(),
            output: String::new(),
            params: spec.params.clone(),
            status: "queued".into(),
            progress: 0.0,
            speed: None,
            eta_sec: None,
            message: None,
            error_kind: None,
            encoder_used: None,
            fallback_note: None,
            attempts: 0,
            conflict: policy.to_string(),
            log_path: None,
            duration_sec: total_duration,
            output_size: None,
            target_bytes: None,
            target_met: None,
            attempts_used: None,
            quality_used: None,
            created_at: now_ms(),
            started_at: None,
            finished_at: None,
        };

        {
            let mut inner = state.inner.lock().await;
            let used: Vec<PathBuf> = inner
                .jobs
                .iter()
                .map(|j| PathBuf::from(&j.output))
                .collect();
            let target = dir.join(format!("{stem}.{ext}"));
            let reserved = used.iter().any(|p| p == &target);
            let exists = target.exists();

            if policy == "skip" && (exists || reserved) {
                job.status = "skipped".into();
                job.message = Some(format!(
                    "目标文件已存在（{}），按「跳过」策略未生成新文件",
                    target
                        .file_name()
                        .map(|n| n.to_string_lossy().to_string())
                        .unwrap_or_default()
                ));
                job.finished_at = Some(now_ms());
                job.output = target.to_string_lossy().to_string();
            } else {
                let output = if policy == "overwrite" && !reserved {
                    target
                } else {
                    // suffix 默认策略；overwrite 与本批其它任务撞名时也退回安全改名。
                    unique_output(&dir, &stem, &ext, &used)
                };
                job.output = output.to_string_lossy().to_string();
                inner.queue.push_back(id.clone());
                inner.drained = false;
                queued_any = true;
            }
            inner.jobs.push(job.clone());
        }
        emit_job(ctx, &job);
        created.push(job);
    }

    if queued_any {
        state.notify.notify_one();
    }
    Ok(created)
}

pub async fn cancel(ctx: &Ctx, id: &str) -> Result<(), String> {
    let state = &ctx.state;
    let mut queued_cancelled = false;
    let child = {
        let mut inner = state.inner.lock().await;
        inner.cancel_requested.insert(id.to_string());
        if let Some(pos) = inner.queue.iter().position(|q| q == id) {
            inner.queue.remove(pos);
            queued_cancelled = true;
        }
        inner.children.remove(id)
    };
    let mut was_running = false;

    if let Some(child) = child {
        was_running = true;
        // 取消 = 中止等待任务 → 子进程被 kill_on_drop 终止；再按 pid 兜底强杀。
        child.handle.abort();
        if let Some(pid) = child.pid {
            #[cfg(windows)]
            {
                const CREATE_NO_WINDOW: u32 = 0x0800_0000;
                use std::os::windows::process::CommandExt;
                let _ = std::process::Command::new("taskkill")
                    .args(["/PID", &pid.to_string(), "/T", "/F"])
                    .stdin(std::process::Stdio::null())
                    .stdout(std::process::Stdio::null())
                    .stderr(std::process::Stdio::null())
                    .creation_flags(CREATE_NO_WINDOW)
                    .status();
            }
            #[cfg(not(windows))]
            {
                let _ = std::process::Command::new("kill")
                    .args(["-TERM", &pid.to_string()])
                    .stderr(std::process::Stdio::null())
                    .status();
            }
        }
    }

    if queued_cancelled {
        set_job(ctx, id, |job| {
            job.status = "cancelled".into();
            job.finished_at = Some(now_ms());
            job.message = Some("已取消（尚未开始执行）".into());
        })
        .await;
        cleanup_temp(ctx, id).await;
        state.notify.notify_one();
    } else if was_running {
        // 运行中的任务由 run_job 收尾；这里再挂一个延迟兜底：子进程退出后文件句柄才释放，
        // 若 run_job 的清理因句柄占用失败，延迟重试能把临时文件清掉（仅对仍处于 cancelled 的任务生效）。
        let ctx2 = ctx.clone();
        let id2 = id.to_string();
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_secs(3)).await;
            let status = {
                let inner = ctx2.state.inner.lock().await;
                inner
                    .jobs
                    .iter()
                    .find(|j| j.id == id2)
                    .map(|j| j.status.clone())
            };
            if status.as_deref() == Some("cancelled") {
                cleanup_temp(&ctx2, &id2).await;
            }
        });
    } else {
        set_job(ctx, id, |job| {
            if job.status == "queued" {
                job.status = "cancelled".into();
                job.finished_at = Some(now_ms());
            }
        })
        .await;
    }
    Ok(())
}

pub async fn retry(ctx: &Ctx, id: &str) -> Result<(), String> {
    let state = &ctx.state;
    {
        let mut inner = state.inner.lock().await;
        let Some(job) = inner.jobs.iter_mut().find(|j| j.id == id) else {
            return Err("任务不存在".into());
        };
        if job.status != "failed" && job.status != "cancelled" {
            return Err("只有失败或已取消的任务可以重试".into());
        }
        job.status = "queued".into();
        job.progress = 0.0;
        job.speed = None;
        job.eta_sec = None;
        job.message = None;
        job.error_kind = None;
        job.finished_at = None;
        job.started_at = None;
        job.attempts += 1;
        job.encoder_used = None;
        job.fallback_note = None;
        job.output_size = None;
        job.log_path = None;
        // params 原样保留：重试不得重置用户在 Inspector 里的选择。
        inner.queue.push_back(id.to_string());
        inner.cancel_requested.remove(id);
    }
    if let Some(job) = get_job(ctx, id).await {
        emit_job(ctx, &job);
    }
    state.notify.notify_one();
    Ok(())
}

pub async fn remove_finished(ctx: &Ctx) -> Vec<String> {
    let state = &ctx.state;
    let (removed, stale_temps): (Vec<String>, Vec<PathBuf>) = {
        let mut inner = state.inner.lock().await;
        let keep: Vec<Job> = inner
            .jobs
            .iter()
            .filter(|j| j.status == "running" || j.status == "queued")
            .cloned()
            .collect();
        let stale: Vec<Job> = inner
            .jobs
            .iter()
            .filter(|j| j.status != "running" && j.status != "queued")
            .cloned()
            .collect();
        let temps = stale
            .iter()
            .filter_map(|j| {
                let out = Path::new(&j.output);
                let dir = out.parent()?;
                let ext = out.extension()?.to_string_lossy().to_string();
                Some(temp_path(dir, &j.id, &ext))
            })
            .collect();
        let removed = stale.iter().map(|j| j.id.clone()).collect();
        inner.jobs = keep;
        (removed, temps)
    };
    // 清空记录时顺手把可能残留的临时文件删掉（取消后延迟清理也可能已删）。
    for temp in &stale_temps {
        remove_temp_file(temp).await;
    }
    for id in &removed {
        ctx.publish_removed(id);
    }
    removed
}

/// 调度器主循环。**必须**在 Tokio 运行时内被 spawn（它自己不再 spawn 外层任务）：
/// 直接 `tokio::spawn` 会让「在同步上下文调用」变成启动即 panic
/// （`there is no reactor running`）。应用侧用 `tauri::async_runtime::spawn`，
/// 无窗口检查程序在自己的 `#[tokio::main]` 里 `tokio::spawn`。
pub async fn start_scheduler(ctx: Ctx) {
    loop {
        let state = &ctx.state;
        let mut started: Vec<Job> = Vec::new();
        {
            let mut inner = state.inner.lock().await;
            // 队列级暂停：只停止调度新任务；已在运行的任务会自然跑完。
            while !inner.paused && inner.running < inner.concurrency {
                let Some(id) = inner.queue.pop_front() else {
                    break;
                };
                let Some(pos) = inner
                    .jobs
                    .iter()
                    .position(|j| j.id == id && j.status == "queued")
                else {
                    continue;
                };
                let mut job = inner.jobs[pos].clone();
                job.status = "running".into();
                job.progress = 0.0;
                job.started_at = Some(now_ms());
                job.speed = None;
                job.eta_sec = None;
                job.error_kind = None;
                job.message = None;
                inner.jobs[pos] = job.clone();
                inner.running += 1;
                started.push(job);
            }
        }
        for job in started {
            emit_job(&ctx, &job);
            let task_ctx = ctx.clone();
            let id = job.id.clone();
            tokio::spawn(async move {
                run_job(task_ctx, id).await;
            });
        }
        state.notify.notified().await;
    }
}

/// 图片序列默认使用当前 FFmpeg 真实可用的软件 H.264 编码器：
/// LGPL 包通常只有 libopenh264，GPL 包才有 libx264，不能硬编码 libx264。
fn pick_sequence_encoder(caps: Option<&probe::Capabilities>) -> String {
    if let Some(caps) = caps {
        for preferred in ["libx264", "libopenh264"] {
            if caps
                .video_encoders
                .iter()
                .any(|e| e.id == preferred && e.available && e.allowed)
            {
                return preferred.to_string();
            }
        }
        if let Some(encoder) = caps
            .video_encoders
            .iter()
            .find(|e| e.codec == "h264" && e.available && e.allowed && !e.hardware)
        {
            return encoder.id.clone();
        }
        if let Some(encoder) = caps
            .video_encoders
            .iter()
            .find(|e| e.codec == "h264" && e.available && e.allowed)
        {
            return encoder.id.clone();
        }
    }
    "libopenh264".to_string()
}

async fn finish_slot(ctx: &Ctx, id: &str) {
    let state = &ctx.state;
    let drained_dir = {
        let mut inner = state.inner.lock().await;
        inner.running = inner.running.saturating_sub(1);
        inner.cancel_requested.remove(id);
        inner.children.remove(id);
        let drained = inner.running == 0 && inner.queue.is_empty() && !inner.drained;
        let dir = inner.jobs.iter().find(|j| j.id == id).and_then(|j| {
            Path::new(&j.output)
                .parent()
                .map(|p| p.to_string_lossy().to_string())
        });
        if drained {
            inner.drained = true;
        }
        if drained {
            dir
        } else {
            None
        }
    };
    // 批次结束动作（打开目录 / 关机）只在这里触发一次；回调可由测试注入，不直接操作宿主。
    if let Some(dir) = drained_dir {
        let action = state.after_action.lock().await.clone();
        if action != "none" {
            (ctx.after)(&action, &dir);
        }
    }
    state.notify.notify_one();
}

/// 自动重试只针对「可重试的失败类别」：硬件暂不可用、临时 IO。
/// 参数错误 / 输入损坏 / 缺编码器 / 权限或磁盘满都不重试，避免无意义重复。
pub fn is_retryable_kind(kind: &str) -> bool {
    matches!(kind, "hw-unavailable" | "io-temporary")
}

/// 每个任务最多自动重试 1 次。必须在 `finish_slot` 之前调用：
/// 让队列里已有新任务，批次结束动作不会被提前触发。
async fn maybe_auto_retry(ctx: &Ctx, id: &str, kind: &str) -> bool {
    if !is_retryable_kind(kind) {
        return false;
    }
    let mut inner = ctx.state.inner.lock().await;
    if inner.cancel_requested.contains(id) {
        return false;
    }
    let Some(job) = inner.jobs.iter_mut().find(|j| j.id == id) else {
        return false;
    };
    if job.attempts >= 1 {
        return false;
    }
    job.attempts += 1;
    job.status = "queued".into();
    job.progress = 0.0;
    job.finished_at = None;
    job.error_kind = None;
    job.speed = None;
    job.eta_sec = None;
    job.message = Some(format!(
        "检测到可重试的失败（{kind}），已自动重试（第 2 次尝试）"
    ));
    inner.queue.push_back(id.to_string());
    inner.drained = false;
    true
}

async fn cleanup_temp(ctx: &Ctx, id: &str) {
    let state = &ctx.state;
    let temps: Vec<String> = {
        let inner = state.inner.lock().await;
        inner
            .jobs
            .iter()
            .filter(|j| j.id == id)
            .flat_map(|j| {
                let dir = Path::new(&j.output).parent().map(|p| p.to_path_buf());
                let ext = Path::new(&j.output)
                    .extension()
                    .map(|e| e.to_string_lossy().to_string())
                    .unwrap_or_else(|| "mp4".into());
                dir.map(|d| temp_path(&d, id, &ext).to_string_lossy().to_string())
                    .into_iter()
                    .collect::<Vec<_>>()
            })
            .collect()
    };
    for temp in temps {
        remove_temp_file(Path::new(&temp)).await;
    }
}

fn classify(stderr: &str) -> (String, String) {
    let low = stderr.to_lowercase();
    let kind = if low.contains("no such file or directory")
        || low.contains("invalid data found")
        || low.contains("moov atom not found")
        || low.contains("error while decoding")
        || low.contains("could not find codec parameters")
    {
        "input"
    } else if low.contains("permission denied") || low.contains("no space left") {
        "output"
    } else if low.contains("resource temporarily unavailable")
        || low.contains("text file busy")
        || low.contains("device or resource busy")
        || low.contains("sharing violation")
        || low.contains("being used by another process")
        || low.contains("input/output error")
        || low.contains("operation timed out")
    {
        "io-temporary"
    } else if low.contains("out of memory")
        || low.contains("cannot allocate memory")
        || low.contains("内存")
    {
        "hw-memory"
    } else if low.contains("mfx session")
        || low.contains("createcomponent")
        || low.contains("could not open encoder")
        || low.contains("openencodesession")
        || low.contains("no device")
        || low.contains("device creation failed")
        || low.contains("function not implemented")
        || low.contains("encoder not found")
        || low.contains("cannot load")
        || low.contains("vainitialize")
        || low.contains("failed to initialise")
        || low.contains("failed to initialize")
    {
        "hw-unavailable"
    } else if low.contains("unknown encoder") || low.contains("encoder not found") {
        "encoder-missing"
    } else if low.contains("invalid argument") || low.contains("incorrect parameters") {
        "invalid-params"
    } else {
        "unknown"
    };

    let lines: Vec<String> = stderr
        .lines()
        .map(|l| l.trim().to_string())
        .filter(|l| {
            let l = l.to_lowercase();
            !l.is_empty()
                && (l.contains("error")
                    || l.contains("failed")
                    || l.contains("invalid")
                    || l.contains("not support")
                    || l.contains("cannot")
                    || l.contains("unable")
                    || l.contains("no such"))
        })
        .collect();
    let message = if lines.is_empty() {
        stderr
            .lines()
            .rev()
            .find(|l| !l.trim().is_empty())
            .map(|l| l.trim().to_string())
            .unwrap_or_else(|| "转换失败（无详细错误输出）".into())
    } else {
        let take = lines.len().saturating_sub(2);
        let tail = lines[take..].join(" · ");
        if tail.chars().count() > 260 {
            tail.chars().take(260).collect::<String>() + "…"
        } else {
            tail
        }
    };
    (kind.to_string(), message)
}

fn is_hardware_encoder(id: &str) -> bool {
    id.ends_with("_nvenc")
        || id.ends_with("_amf")
        || id.ends_with("_qsv")
        || id.ends_with("_vaapi")
        || id.ends_with("_videotoolbox")
        || id.ends_with("_d3d12va")
        || id.ends_with("_mf")
}

fn encoder_chain(
    caps: Option<&probe::Capabilities>,
    codec: &str,
    mode: &str,
    allow_gpl: bool,
) -> Vec<String> {
    let Some(caps) = caps else {
        return if mode == "software" || mode == "auto" {
            vec![]
        } else {
            vec![mode.to_string()]
        };
    };
    let usable: Vec<&probe::EncoderCap> = caps
        .video_encoders
        .iter()
        .filter(|e| e.codec == codec && e.compiled && e.allowed && e.available)
        .collect();

    if mode != "auto" && mode != "software" && !mode.is_empty() {
        // 显式指定的编码器同样受变体许可约束：LGPL 版不允许绕过能力表直接用 libx264。
        if let Some(chosen) = caps.video_encoders.iter().find(|e| e.id == mode) {
            if chosen.license != "lgpl" && !allow_gpl {
                return Vec::new();
            }
        }
        let mut chain = vec![mode.to_string()];
        let chosen = caps.video_encoders.iter().find(|e| e.id == mode);
        if chosen.map(|c| c.hardware).unwrap_or(false) {
            for enc in &usable {
                if !enc.hardware {
                    chain.push(enc.id.clone());
                }
            }
        }
        return chain;
    }

    let mut chain: Vec<String> = Vec::new();
    if mode != "software" {
        let order = ["nvenc", "amf", "qsv", "vaapi", "videotoolbox", "d3d12va"];
        for vendor in order {
            for enc in &usable {
                if enc.hardware && enc.vendor == vendor && !chain.contains(&enc.id) {
                    chain.push(enc.id.clone());
                }
            }
        }
    }
    for enc in &usable {
        if !enc.hardware {
            if enc.license != "lgpl" && !allow_gpl {
                continue;
            }
            chain.push(enc.id.clone());
        }
    }
    chain
}

fn best_decode_accel(caps: Option<&probe::Capabilities>) -> Option<String> {
    caps.and_then(|c| {
        c.decode_accels
            .iter()
            .find(|d| d.available)
            .map(|d| d.id.clone())
    })
}

struct ExecOutcome {
    success: bool,
    killed: bool,
    stderr: String,
}

async fn execute_streaming(
    ctx: &Ctx,
    job_id: &str,
    program: &str,
    argv: Vec<String>,
    duration: Option<f64>,
    progress: (f64, f64),
) -> ExecOutcome {
    let state = ctx.state.clone();
    let mut cmd = Command::new(program);
    cmd.args(&argv)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .kill_on_drop(true);
    #[cfg(windows)]
    {
        // tokio::process::Command 自带 creation_flags；避免每次转换弹黑色控制台窗口。
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }

    let spawned = cmd.spawn();
    let mut child: Child = match spawned {
        Ok(child) => child,
        Err(err) => {
            return ExecOutcome {
                success: false,
                killed: false,
                stderr: format!("无法启动 ffmpeg：{err}"),
            }
        }
    };
    let pid = child.id();
    let stdout = child.stdout.take();
    let stderr = child.stderr.take();

    let stderr_buf: Arc<Mutex<VecDeque<String>>> = Arc::new(Mutex::new(VecDeque::new()));
    if let Some(err_pipe) = stderr {
        let buf = stderr_buf.clone();
        tokio::spawn(async move {
            let mut reader = BufReader::new(err_pipe).lines();
            while let Ok(Some(line)) = reader.next_line().await {
                let mut guard = buf.lock().await;
                guard.push_back(line);
                // 保留更多行：任务日志需要完整 stderr，任务中心仍只用摘要。
                if guard.len() > 400 {
                    guard.pop_front();
                }
            }
        });
    }
    if let Some(out_pipe) = stdout {
        let ctx2 = ctx.clone();
        let id2 = job_id.to_string();
        tokio::spawn(async move {
            parse_progress(ctx2, id2, out_pipe, duration, progress).await;
        });
    }

    let (tx, rx) = tokio::sync::oneshot::channel();
    let waiter = tokio::spawn(async move {
        let status = child.wait().await;
        let _ = tx.send(status);
    });

    {
        let mut inner = state.inner.lock().await;
        inner.children.insert(
            job_id.to_string(),
            RunningChild {
                handle: waiter,
                pid,
            },
        );
    }

    let wait_result = rx.await;
    {
        let mut inner = state.inner.lock().await;
        inner.children.remove(job_id);
    }

    let killed = {
        let inner = state.inner.lock().await;
        inner.cancel_requested.contains(job_id)
    };

    let stderr_text = {
        let guard = stderr_buf.lock().await;
        guard.iter().cloned().collect::<Vec<_>>().join("\n")
    };

    match wait_result {
        Ok(Ok(status)) => ExecOutcome {
            success: status.success(),
            killed,
            stderr: stderr_text,
        },
        _ => ExecOutcome {
            success: false,
            killed: true,
            stderr: stderr_text,
        },
    }
}

async fn parse_progress(
    ctx: Ctx,
    job_id: String,
    pipe: tokio::process::ChildStdout,
    duration: Option<f64>,
    progress: (f64, f64),
) {
    let mut reader = BufReader::new(pipe).lines();
    let mut last_emit = Instant::now()
        .checked_sub(Duration::from_secs(10))
        .unwrap_or_else(Instant::now);
    let mut last_pct: i32 = -1;
    let mut speed: Option<String> = None;
    let mut out_us: f64 = 0.0;

    while let Ok(Some(line)) = reader.next_line().await {
        let (key, value) = match line.split_once('=') {
            Some(kv) => kv,
            None => continue,
        };
        match key {
            "out_time_us" | "out_time_ms" => {
                if let Ok(v) = value.trim().parse::<f64>() {
                    out_us = v;
                }
            }
            "speed" => {
                let v = value.trim();
                if v.ends_with('x') {
                    speed = Some(v.to_string());
                }
            }
            "progress" => {
                if value.trim() == "end" {
                    out_us = duration.map(|d| d * 1_000_000.0).unwrap_or(out_us);
                }
            }
            _ => {}
        }

        let step_ratio = duration
            .filter(|d| *d > 0.0)
            .map(|d| (out_us / 1_000_000.0 / d).clamp(0.0, 0.999))
            .unwrap_or(0.0);
        // 多步任务（合并转码 + 拼接）把每一步映射到总进度的一段，避免进度条回跳。
        let ratio = (progress.0 + step_ratio * progress.1).clamp(0.0, 0.999);
        let pct = (ratio * 100.0) as i32;
        let should_emit =
            last_pct < 0 || pct - last_pct >= 2 || last_emit.elapsed() > Duration::from_millis(250);
        if should_emit {
            last_emit = Instant::now();
            last_pct = pct;
            let eta = duration.filter(|d| *d > 0.0).and_then(|d| {
                speed
                    .as_deref()
                    .and_then(|s| s.trim_end_matches('x').parse::<f64>().ok())
                    .filter(|s| *s > 0.01)
                    .map(|s| (d - out_us / 1_000_000.0).max(0.0) / s)
            });
            let speed_clone = speed.clone();
            let _ = set_job(&ctx, &job_id, |job| {
                job.progress = ratio;
                job.speed = speed_clone;
                job.eta_sec = eta;
            })
            .await;
        }
    }
}

/// 原子发布临时文件，且绝不覆盖目标。
/// 优先用硬链接：目标已存在时由文件系统原子地返回 AlreadyExists；
/// 某些文件系统（exFAT/FAT32）不支持硬链接时，退回到「检查后 rename」，仍不覆盖。
fn rename_atomic(temp: &Path, target: &Path) -> std::io::Result<u64> {
    let size = std::fs::metadata(temp)?.len();
    match std::fs::hard_link(temp, target) {
        Ok(()) => {
            std::fs::remove_file(temp)?;
            Ok(size)
        }
        Err(err) if err.kind() == std::io::ErrorKind::AlreadyExists => Err(err),
        Err(_) => {
            if target.exists() {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::AlreadyExists,
                    "目标文件已存在，拒绝覆盖",
                ));
            }
            std::fs::rename(temp, target)?;
            Ok(size)
        }
    }
}

/// 每任务诊断日志：完整参数 + 最终 stderr，写到 `<appData>/logs/job-<id>.log`。
/// 没有配置日志目录（例如部分无窗口检查）时静默跳过，不影响任务结果。
async fn write_job_log(ctx: &Ctx, id: &str, status: &str, attempts: &[String], stderr: &str) {
    let dir = { ctx.state.log_dir.lock().await.clone() };
    let Some(dir) = dir else { return };
    if std::fs::create_dir_all(&dir).is_err() {
        return;
    }
    let Some(job) = get_job(ctx, id).await else {
        return;
    };
    let params =
        serde_json::to_string_pretty(&job.params).unwrap_or_else(|_| job.params.to_string());
    let body = format!(
        "Transmutio 任务日志\n\
         id: {id}\n\
         status: {status}\n\
         kind: {}\n\
         input: {}\n\
         output: {}\n\
         durationSec: {:?}\n\
         attempts: {}\n\
         encoderUsed: {:?}\n\
         errorKind: {:?}\n\
         message: {:?}\n\
         fallback: {}\n\
         params:\n{params}\n\n\
         --- ffmpeg stderr ---\n{stderr}\n",
        job.kind,
        job.input,
        job.output,
        job.duration_sec,
        job.attempts,
        job.encoder_used,
        job.error_kind,
        job.message,
        if attempts.is_empty() {
            "无".to_string()
        } else {
            attempts.join("；")
        },
    );
    let path = dir.join(format!("job-{id}.log"));
    if std::fs::write(&path, body).is_ok() {
        let text = path.to_string_lossy().to_string();
        let _ = set_job(ctx, id, |j| j.log_path = Some(text)).await;
    }
}

struct CompositeStep {
    argv: Vec<String>,
    duration: Option<f64>,
    label: String,
}

/// 多步任务（合并 / 混流 / GIF）的执行器。
/// 与单步转换共用同一套子进程执行、取消、日志与发布逻辑，避免两套行为分叉。
#[allow(clippy::too_many_arguments)]
async fn run_composite(
    ctx: Ctx,
    id: String,
    job: Job,
    ffmpeg: String,
    caps: Option<probe::Capabilities>,
    allow_gpl: bool,
    dir: PathBuf,
    temp: PathBuf,
) {
    let state = ctx.state.clone();
    let ffprobe = {
        let tools = state.tools.lock().await;
        tools
            .as_ref()
            .and_then(|t| t.ffprobe.as_ref())
            .map(|t| t.path.clone())
    };
    let ext = Path::new(&job.output)
        .extension()
        .map(|e| e.to_string_lossy().to_string())
        .unwrap_or_else(|| "mp4".into());
    let mut plan: Vec<CompositeStep> = Vec::new();
    let mut temps: Vec<PathBuf> = vec![temp.clone()];
    let mut failure: Option<(String, String)> = None;
    let mut used_encoder: Option<String> = None;
    let mut planned_duration: Option<f64> = None;

    match job.kind.as_str() {
        "video-merge" => {
            let p: args::MergeParams =
                serde_json::from_value(job.params.clone()).unwrap_or_default();
            let inputs: Vec<String> = if job.inputs.is_empty() {
                vec![job.input.clone()]
            } else {
                job.inputs.clone()
            };
            if inputs.len() < 2 {
                failure = Some(("invalid-params".into(), "合并至少需要 2 个输入文件".into()));
            } else if ffprobe.is_none() {
                failure = Some((
                    "input".into(),
                    "找不到 ffprobe，无法判断输入参数是否一致".into(),
                ));
            } else {
                let probe_bin = ffprobe.clone().unwrap_or_default();
                let mut infos: Vec<probe::MediaInfo> = Vec::new();
                for input in &inputs {
                    infos.push(probe::probe_media(&probe_bin, input).await);
                }
                if let Some(bad) = infos.iter().find(|i| i.error.is_some()) {
                    failure = Some((
                        "input".into(),
                        format!(
                            "无法读取 {}：{}",
                            bad.name,
                            bad.error.clone().unwrap_or_default()
                        ),
                    ));
                } else {
                    let mut mismatch: Option<String> = None;
                    for info in &infos[1..] {
                        if mismatch.is_none() {
                            mismatch = args::merge_incompatibility(&infos[0], info);
                        }
                    }
                    let copy_mode = match p.mode.as_str() {
                        "copy" => true,
                        "transcode" => false,
                        _ => mismatch.is_none(),
                    };
                    if p.mode == "copy" && mismatch.is_some() {
                        failure = Some((
                            "invalid-params".into(),
                            format!(
                                "输入参数不一致（{}）。「直接拼接」要求编码/分辨率/帧率/音轨参数完全一致；请改用「自动」或「统一转码」。",
                                mismatch.clone().unwrap_or_default()
                            ),
                        ));
                    } else if copy_mode {
                        let list = dir.join(format!(".ff-list-{id}.txt"));
                        let abs_inputs = absolute_paths(&inputs);
                        if let Err(err) = std::fs::write(&list, args::concat_list(&abs_inputs)) {
                            failure = Some(("output".into(), format!("无法写入拼接清单：{err}")));
                        } else {
                            temps.push(list.clone());
                            plan.push(CompositeStep {
                                argv: args::build_concat_copy(
                                    &list.to_string_lossy(),
                                    &temp.to_string_lossy(),
                                ),
                                duration: job.duration_sec,
                                label: "直接拼接（stream copy）".into(),
                            });
                            used_encoder = Some("copy".into());
                        }
                    } else {
                        let codec = if p.codec.is_empty() {
                            "h264"
                        } else {
                            p.codec.as_str()
                        };
                        let chain = encoder_chain(caps.as_ref(), codec, "auto", allow_gpl);
                        let encoder = chain.first().cloned().or_else(|| {
                            if caps.is_none() {
                                Some("libx264".to_string())
                            } else {
                                None
                            }
                        });
                        match encoder {
                            None => {
                                failure = Some((
                                    "encoder-missing".into(),
                                    "没有可用的 H.264 编码器，无法先统一转码再拼接。".into(),
                                ))
                            }
                            Some(encoder) => {
                                used_encoder = Some(encoder.clone());
                                let target_h = match p.resolution.as_str() {
                                    "" | "source" => infos[0].height,
                                    other => other.parse::<u32>().ok().or(infos[0].height),
                                };
                                let target_fps = match p.fps.as_str() {
                                    "" | "source" => infos[0].fps.map(|f| f.round() as u32),
                                    other => other.parse::<u32>().ok(),
                                };
                                let mut segments: Vec<String> = Vec::new();
                                let mut sum = 0.0f64;
                                for (index, input) in inputs.iter().enumerate() {
                                    let segment = dir.join(format!(".ff-seg-{id}-{index}.mp4"));
                                    temps.push(segment.clone());
                                    segments.push(segment.to_string_lossy().to_string());
                                    let d = infos[index].duration_sec;
                                    sum += d.unwrap_or(0.0);
                                    plan.push(CompositeStep {
                                        argv: args::build_merge_segment(
                                            &encoder,
                                            input,
                                            &segment.to_string_lossy(),
                                            &p,
                                            target_h,
                                            target_fps,
                                        ),
                                        duration: d,
                                        label: format!("统一转码 {}/{}", index + 1, inputs.len()),
                                    });
                                }
                                let list = dir.join(format!(".ff-list-{id}.txt"));
                                let abs_segments = absolute_paths(&segments);
                                if let Err(err) =
                                    std::fs::write(&list, args::concat_list(&abs_segments))
                                {
                                    failure =
                                        Some(("output".into(), format!("无法写入拼接清单：{err}")));
                                } else {
                                    temps.push(list.clone());
                                    plan.push(CompositeStep {
                                        argv: args::build_concat_copy(
                                            &list.to_string_lossy(),
                                            &temp.to_string_lossy(),
                                        ),
                                        duration: Some(sum),
                                        label: "拼接".into(),
                                    });
                                }
                                planned_duration = Some(sum);
                            }
                        }
                    }
                }
            }
        }
        "video-mux" => {
            let p: args::MuxParams = serde_json::from_value(job.params.clone()).unwrap_or_default();
            let audio = job
                .inputs
                .iter()
                .find(|path| path.as_str() != job.input)
                .cloned();
            match (audio, ffprobe.clone()) {
                (None, _) => {
                    failure = Some(("invalid-params".into(), "请再选择一个音频文件".into()))
                }
                (_, None) => {
                    failure = Some(("input".into(), "找不到 ffprobe，无法读取音频参数".into()))
                }
                (Some(audio), Some(probe_bin)) => {
                    let video_info = probe::probe_media(&probe_bin, &job.input).await;
                    let audio_info = probe::probe_media(&probe_bin, &audio).await;
                    if let Some(err) = video_info.error.clone() {
                        failure = Some(("input".into(), format!("无法读取视频：{err}")));
                    } else if let Some(err) = audio_info.error.clone() {
                        failure = Some(("input".into(), format!("无法读取音频：{err}")));
                    } else if audio_info.audio_codec.is_none() {
                        failure = Some((
                            "invalid-params".into(),
                            format!("{} 里没有音轨，无法用于混流", audio_info.name),
                        ));
                    } else if p.audio_mode == "copy" {
                        let codec = audio_info
                            .audio_codec
                            .clone()
                            .unwrap_or_default()
                            .to_lowercase();
                        // MP4 可直接容纳的常见音频编码；其余格式必须转 AAC。
                        let copyable = ["aac", "mp3", "ac3", "eac3", "opus", "alac", "mp4a"];
                        if !copyable.contains(&codec.as_str()) {
                            failure = Some((
                                "invalid-params".into(),
                                format!(
                                    "音频编码 {codec} 不能直接复制进 MP4（采样率/编码不被容器接受）；请把音频模式改为 AAC 转码。"
                                ),
                            ));
                        }
                    }
                    if failure.is_none() {
                        plan.push(CompositeStep {
                            argv: args::build_mux(
                                &job.input,
                                &audio,
                                &temp.to_string_lossy(),
                                &p,
                                video_info.duration_sec,
                            ),
                            duration: video_info.duration_sec,
                            label: "混流（视频 copy）".into(),
                        });
                        used_encoder = Some("copy".into());
                        planned_duration = video_info.duration_sec;
                    }
                }
            }
        }
        "video-gif" => {
            let p: args::GifParams = serde_json::from_value(job.params.clone()).unwrap_or_default();
            match ffprobe.clone() {
                None => failure = Some(("input".into(), "找不到 ffprobe，无法读取视频时长".into())),
                Some(probe_bin) => {
                    let info = probe::probe_media(&probe_bin, &job.input).await;
                    if let Some(err) = info.error.clone() {
                        failure = Some(("input".into(), format!("无法读取视频：{err}")));
                    } else {
                        if let (Some(total), true) = (info.duration_sec, p.start_sec > 0.0) {
                            if p.start_sec >= total {
                                failure = Some((
                                    "invalid-params".into(),
                                    format!(
                                        "开始时间 {:.2}s 超出视频时长 {:.2}s",
                                        p.start_sec, total
                                    ),
                                ));
                            }
                        }
                        let duration = if p.duration_sec > 0.0 {
                            Some(p.duration_sec)
                        } else {
                            info.duration_sec.map(|d| (d - p.start_sec).max(0.0))
                        };
                        if failure.is_none() {
                            plan.push(CompositeStep {
                                argv: args::build_gif(&job.input, &temp.to_string_lossy(), &p),
                                duration,
                                label: if p.optimize {
                                    "GIF（调色板优化）".into()
                                } else {
                                    "GIF（直接转码）".into()
                                },
                            });
                            planned_duration = duration;
                        }
                    }
                }
            }
        }
        _ => unreachable!(),
    }

    if let Some((kind, message)) = failure {
        for path in &temps {
            let _ = std::fs::remove_file(path);
        }
        set_job(&ctx, &id, |job| {
            job.status = "failed".into();
            job.error_kind = Some(kind.clone());
            job.message = Some(message.clone());
            job.finished_at = Some(now_ms());
            job.progress = 0.0;
            job.speed = None;
            job.eta_sec = None;
        })
        .await;
        write_job_log(&ctx, &id, "failed", &[], "").await;
        finish_slot(&ctx, &id).await;
        return;
    }

    let total_weight: f64 = plan
        .iter()
        .map(|s| s.duration.unwrap_or(1.0).max(0.1))
        .sum::<f64>()
        .max(0.1);
    let mut done_weight = 0.0f64;
    let mut attempts_info: Vec<String> = Vec::new();
    let mut last_stderr = String::new();

    for step in &plan {
        let cancelled = {
            let inner = state.inner.lock().await;
            inner.cancel_requested.contains(&id)
        };
        if cancelled {
            break;
        }
        let weight = step.duration.unwrap_or(1.0).max(0.1);
        let base = (done_weight / total_weight).clamp(0.0, 1.0);
        let span = (weight / total_weight).clamp(0.0, 1.0);
        let _ = set_job(&ctx, &id, |job| {
            job.message = Some(format!("正在{}…", step.label));
        })
        .await;
        let outcome = execute_streaming(
            &ctx,
            &id,
            &ffmpeg,
            step.argv.clone(),
            step.duration,
            (base, span),
        )
        .await;
        last_stderr = outcome.stderr.clone();

        let cancelled_now = {
            let inner = state.inner.lock().await;
            inner.cancel_requested.contains(&id)
        };
        if outcome.killed || cancelled_now {
            for path in &temps {
                remove_temp_file(path).await;
            }
            set_job(&ctx, &id, |job| {
                job.status = "cancelled".into();
                job.finished_at = Some(now_ms());
                job.progress = 0.0;
                job.message = Some("已取消".into());
                job.speed = None;
                job.eta_sec = None;
            })
            .await;
            write_job_log(&ctx, &id, "cancelled", &attempts_info, &last_stderr).await;
            finish_slot(&ctx, &id).await;
            return;
        }
        if !outcome.success {
            let (kind, message) = classify(&outcome.stderr);
            let guidance = match kind.as_str() {
                "invalid-params" => "当前参数组合不被接受，请调整合并模式/编码或音频模式。",
                "input" => "输入文件无法解码，或与合并要求不兼容。",
                "output" => "写入输出失败（权限或磁盘空间）。",
                "encoder-missing" => "本机缺少可用编码器。",
                _ => "处理失败，请查看诊断日志。",
            };
            for path in &temps {
                let _ = std::fs::remove_file(path);
            }
            set_job(&ctx, &id, |job| {
                job.status = "failed".into();
                job.error_kind = Some(kind.clone());
                job.message = Some(format!("{message} —— {guidance}"));
                job.finished_at = Some(now_ms());
                job.progress = 0.0;
                job.speed = None;
                job.eta_sec = None;
                job.encoder_used = used_encoder.clone();
                if !attempts_info.is_empty() {
                    job.fallback_note = Some(attempts_info.join("；"));
                }
            })
            .await;
            write_job_log(&ctx, &id, "failed", &attempts_info, &last_stderr).await;
            maybe_auto_retry(&ctx, &id, &kind).await;
            finish_slot(&ctx, &id).await;
            return;
        }
        done_weight += weight;
        attempts_info.push(format!("{} 完成", step.label));
        let _ = set_job(&ctx, &id, |job| {
            job.progress = (done_weight / total_weight).clamp(0.0, 1.0);
        })
        .await;
    }

    let target = PathBuf::from(&job.output);
    let published = if job.conflict == "overwrite" {
        rename_overwrite(&temp, &target)
    } else {
        match rename_atomic(&temp, &target) {
            Ok(size) => Ok(size),
            Err(err) if err.kind() == std::io::ErrorKind::AlreadyExists => {
                let used: Vec<PathBuf> = {
                    let inner = state.inner.lock().await;
                    inner
                        .jobs
                        .iter()
                        .filter(|other| other.id != id)
                        .map(|other| PathBuf::from(&other.output))
                        .collect()
                };
                let stem = file_stem(&job.output);
                let alt = unique_output(&dir, &stem, &ext, &used);
                match rename_atomic(&temp, &alt) {
                    Ok(size) => {
                        let _ = set_job(&ctx, &id, |job| {
                            job.output = alt.to_string_lossy().to_string();
                        })
                        .await;
                        Ok(size)
                    }
                    Err(err) => Err(err),
                }
            }
            Err(err) => Err(err),
        }
    };

    match published {
        Ok(size) => {
            set_job(&ctx, &id, |job| {
                job.status = "done".into();
                job.progress = 1.0;
                job.output_size = Some(size);
                job.encoder_used = used_encoder.clone();
                job.finished_at = Some(now_ms());
                job.speed = None;
                job.eta_sec = None;
                job.message = None;
                if let Some(d) = planned_duration {
                    job.duration_sec = Some(d);
                }
                if !attempts_info.is_empty() {
                    job.fallback_note = Some(attempts_info.join("；"));
                }
            })
            .await;
            write_job_log(&ctx, &id, "done", &attempts_info, &last_stderr).await;
        }
        Err(err) => {
            let _ = std::fs::remove_file(&temp);
            set_job(&ctx, &id, |job| {
                job.status = "failed".into();
                job.error_kind = Some("output".into());
                job.message = Some(format!("写出最终文件失败：{err}"));
                job.finished_at = Some(now_ms());
                job.progress = 0.0;
            })
            .await;
            write_job_log(&ctx, &id, "failed", &attempts_info, &last_stderr).await;
        }
    }
    // 清理中间片段与清单文件；最终文件已发布。
    for path in &temps {
        if path != &temp {
            let _ = std::fs::remove_file(path);
        }
    }
    finish_slot(&ctx, &id).await;
}

struct PdfOutcome {
    first_output: PathBuf,
    bytes: u64,
    message: String,
}

fn pdf_error_kind(message: &str) -> &'static str {
    if message.contains("pdftoppm") || message.contains("外部工具") {
        "external-tool"
    } else if message.contains("页码")
        || message.contains("至少需要")
        || message.contains("不支持")
        || message.contains("超出")
        || message.contains("加密")
        || message.contains("没有页面")
    {
        "invalid-params"
    } else {
        "input"
    }
}

/// PDF 合并 / 拆分 / 转图片。合并与拆分完全本地；转图片要求 pdftoppm。
async fn run_pdf(ctx: Ctx, id: String, job: Job, dir: PathBuf, temp: PathBuf) {
    let kind = job.kind.clone();
    let _ = set_job(&ctx, &id, |job| {
        job.message = Some(match kind.as_str() {
            "pdf-merge" => "正在合并 PDF…".into(),
            "pdf-split" => "正在拆分 PDF…".into(),
            _ => "正在把 PDF 渲染成图片…".into(),
        });
    })
    .await;

    let pdf_dir = { ctx.state.pdf_tools_dir.lock().await.clone() };
    let job2 = job.clone();
    let dir2 = dir.clone();
    let temp2 = temp.clone();
    let id2 = id.clone();
    let result: Result<PdfOutcome, String> =
        tokio::task::spawn_blocking(move || -> Result<PdfOutcome, String> {
            let stem = file_stem(&job2.output);
            match kind.as_str() {
                "pdf-merge" => {
                    let total = pdf::merge(&job2.inputs, &temp2)?;
                    let overwrite = conflict_policy(Some(&job2.conflict)) == "overwrite";
                    let (target, size) = if overwrite {
                        let size = rename_overwrite(&temp2, Path::new(&job2.output))
                            .map_err(|e| format!("写入输出失败：{e}"))?;
                        (PathBuf::from(&job2.output), size)
                    } else {
                        match rename_atomic(&temp2, Path::new(&job2.output)) {
                            Ok(size) => (PathBuf::from(&job2.output), size),
                            Err(err) if err.kind() == std::io::ErrorKind::AlreadyExists => {
                                let target = unique_output(&dir2, &stem, "pdf", &[]);
                                let size = rename_atomic(&temp2, &target)
                                    .map_err(|e| format!("写入输出失败：{e}"))?;
                                (target, size)
                            }
                            Err(err) => return Err(format!("写入输出失败：{err}")),
                        }
                    };
                    Ok(PdfOutcome {
                        first_output: target,
                        bytes: size,
                        message: format!("已合并 {total} 页 PDF"),
                    })
                }
                "pdf-split" => {
                    let tmpdir = dir2.join(format!(".ff-tmp-split-{id2}"));
                    let _ = std::fs::remove_dir_all(&tmpdir);
                    let per_page = job2
                        .params
                        .get("mode")
                        .and_then(Value::as_str)
                        .unwrap_or("per-page")
                        == "per-page";
                    let ranges = job2
                        .params
                        .get("ranges")
                        .and_then(Value::as_str)
                        .map(str::to_string);
                    let produced =
                        pdf::split(&job2.inputs[0], &tmpdir, &stem, per_page, ranges.as_deref())?;
                    let mut used: Vec<PathBuf> = Vec::new();
                    let mut bytes = 0u64;
                    let mut first: Option<PathBuf> = None;
                    for file in produced {
                        let ext = file
                            .extension()
                            .map(|e| e.to_string_lossy().to_string())
                            .unwrap_or_else(|| "pdf".into());
                        let target = unique_output(
                            &dir2,
                            &file_stem(file.to_string_lossy().as_ref()),
                            &ext,
                            &used,
                        );
                        std::fs::rename(&file, &target).map_err(|e| {
                            format!("写入拆分结果失败（{}）：{e}", target.display())
                        })?;
                        bytes += std::fs::metadata(&target).map(|m| m.len()).unwrap_or(0);
                        used.push(target.clone());
                        first.get_or_insert(target);
                    }
                    let _ = std::fs::remove_dir_all(&tmpdir);
                    Ok(PdfOutcome {
                        first_output: first.ok_or("拆分没有产生任何文件")?,
                        bytes,
                        message: format!("已拆分为 {} 个 PDF", used.len()),
                    })
                }
                _ => {
                    let tools = crate::pdf_tools::resolve(pdf_dir.as_deref());
                    let pdftoppm = tools.pdftoppm.clone().ok_or_else(|| {
                        tools
                            .reason
                            .clone()
                            .unwrap_or_else(|| "未找到 pdftoppm".into())
                    })?;
                    let format = job2
                        .params
                        .get("format")
                        .and_then(Value::as_str)
                        .unwrap_or("png")
                        .to_string();
                    let dpi = job2.params.get("dpi").and_then(Value::as_u64).unwrap_or(96) as u32;
                    let total = pdf::page_count(&job2.inputs[0])?;
                    let tmpdir = dir2.join(format!(".ff-tmp-pdfimg-{id2}"));
                    let _ = std::fs::remove_dir_all(&tmpdir);
                    let produced = pdf::rasterize(
                        &pdftoppm,
                        &job2.inputs[0],
                        &tmpdir,
                        "page",
                        &format,
                        dpi,
                        1,
                        total.max(1) as u32,
                    )?;
                    let ext = if format == "jpg" { "jpg" } else { "png" };
                    let mut used: Vec<PathBuf> = Vec::new();
                    let mut bytes = 0u64;
                    let mut first: Option<PathBuf> = None;
                    for (index, file) in produced.iter().enumerate() {
                        let target =
                            unique_output(&dir2, &format!("{stem}-p{:04}", index + 1), ext, &used);
                        std::fs::rename(file, &target)
                            .map_err(|e| format!("写入图片失败（{}）：{e}", target.display()))?;
                        bytes += std::fs::metadata(&target).map(|m| m.len()).unwrap_or(0);
                        used.push(target.clone());
                        first.get_or_insert(target);
                    }
                    let _ = std::fs::remove_dir_all(&tmpdir);
                    Ok(PdfOutcome {
                        first_output: first.ok_or("PDF 渲染没有产生图片")?,
                        bytes,
                        message: format!(
                            "已输出 {} 张图片（{} DPI，{} 页）",
                            used.len(),
                            dpi,
                            total
                        ),
                    })
                }
            }
        })
        .await
        .unwrap_or_else(|e| Err(format!("PDF 任务执行失败：{e}")));

    match result {
        Ok(outcome) => {
            let path_text = outcome.first_output.to_string_lossy().to_string();
            let _ = set_job(&ctx, &id, |job| {
                job.status = "done".into();
                job.progress = 1.0;
                job.output = path_text.clone();
                job.output_size = Some(outcome.bytes);
                job.message = Some(outcome.message.clone());
                job.finished_at = Some(now_ms());
                job.encoder_used = Some("native".into());
            })
            .await;
            write_job_log(&ctx, &id, "done", &[], &outcome.message).await;
        }
        Err(message) => {
            let kind = pdf_error_kind(&message);
            let _ = set_job(&ctx, &id, |job| {
                job.status = "failed".into();
                job.error_kind = Some(kind.into());
                job.message = Some(message.clone());
                job.finished_at = Some(now_ms());
                job.progress = 0.0;
            })
            .await;
            write_job_log(&ctx, &id, "failed", &[], &message).await;
            maybe_auto_retry(&ctx, &id, kind).await;
        }
    }
    finish_slot(&ctx, &id).await;
}

/// 视频 → 图片序列。先写临时目录，成功后再逐帧移入输出目录，失败不留半套。
async fn run_frames(ctx: Ctx, id: String, job: Job, ffmpeg: String, dir: PathBuf) {
    let p: args::FramesParams = serde_json::from_value(job.params.clone()).unwrap_or_default();
    let ext = if p.format == "jpg" { "jpg" } else { "png" };
    let stem = file_stem(&job.output);
    let tmpdir = dir.join(format!(".ff-tmp-frames-{id}"));
    let _ = std::fs::remove_dir_all(&tmpdir);
    if let Err(err) = std::fs::create_dir_all(&tmpdir) {
        let message = format!("无法创建临时目录：{err}");
        let _ = set_job(&ctx, &id, |job| {
            job.status = "failed".into();
            job.error_kind = Some("output".into());
            job.message = Some(message.clone());
            job.finished_at = Some(now_ms());
        })
        .await;
        write_job_log(&ctx, &id, "failed", &[], &message).await;
        finish_slot(&ctx, &id).await;
        return;
    }
    let pattern = tmpdir.join(format!("frame-%04d.{ext}"));
    let argv = args::build_video_frames(&job.input, pattern.to_string_lossy().as_ref(), &p);
    let _ = set_job(&ctx, &id, |job| {
        job.message = Some(format!("正在按 {} fps 抽帧…", p.fps.max(1)));
    })
    .await;
    let outcome = execute_streaming(&ctx, &id, &ffmpeg, argv, job.duration_sec, (0.0, 0.92)).await;
    if !outcome.success {
        let (kind, message) = classify(&outcome.stderr);
        let (kind, message) = if outcome.killed {
            ("cancelled".to_string(), "已取消".to_string())
        } else {
            (kind, message)
        };
        let _ = set_job(&ctx, &id, |job| {
            job.status = if kind == "cancelled" {
                "cancelled".into()
            } else {
                "failed".into()
            };
            job.error_kind = if kind == "cancelled" {
                None
            } else {
                Some(kind.clone())
            };
            job.message = Some(message.clone());
            job.finished_at = Some(now_ms());
            job.progress = 0.0;
        })
        .await;
        write_job_log(
            &ctx,
            &id,
            if kind == "cancelled" {
                "cancelled"
            } else {
                "failed"
            },
            &[],
            &outcome.stderr,
        )
        .await;
        if kind != "cancelled" {
            maybe_auto_retry(&ctx, &id, &kind).await;
        }
        let _ = std::fs::remove_dir_all(&tmpdir);
        finish_slot(&ctx, &id).await;
        return;
    }

    let mut frames: Vec<PathBuf> = std::fs::read_dir(&tmpdir)
        .map(|entries| {
            entries
                .filter_map(|entry| entry.ok().map(|e| e.path()))
                .filter(|path| path.is_file())
                .collect()
        })
        .unwrap_or_default();
    frames.sort();
    if frames.is_empty() {
        let message = "抽帧没有产生任何图片".to_string();
        let _ = set_job(&ctx, &id, |job| {
            job.status = "failed".into();
            job.error_kind = Some("input".into());
            job.message = Some(message.clone());
            job.finished_at = Some(now_ms());
        })
        .await;
        write_job_log(&ctx, &id, "failed", &[], &message).await;
        finish_slot(&ctx, &id).await;
        return;
    }

    let mut used: Vec<PathBuf> = Vec::new();
    let mut bytes = 0u64;
    let mut first: Option<PathBuf> = None;
    for (index, frame) in frames.iter().enumerate() {
        let target = unique_output(&dir, &format!("{stem}-{:04}", index + 1), ext, &used);
        std::fs::rename(frame, &target)
            .map_err(|e| e.to_string())
            .ok();
        if target.is_file() {
            bytes += std::fs::metadata(&target).map(|m| m.len()).unwrap_or(0);
            used.push(target.clone());
            first.get_or_insert(target);
        }
    }
    let _ = std::fs::remove_dir_all(&tmpdir);
    let count = used.len();
    let path_text = first
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_else(|| job.output.clone());
    let _ = set_job(&ctx, &id, |job| {
        job.status = "done".into();
        job.progress = 1.0;
        job.output = path_text;
        job.output_size = Some(bytes);
        job.message = Some(format!("已输出 {count} 帧"));
        job.finished_at = Some(now_ms());
    })
    .await;
    write_job_log(&ctx, &id, "done", &[], &format!("已输出 {count} 帧")).await;
    finish_slot(&ctx, &id).await;
}

/// 把切分时长核对追加到任务日志末尾。日志本身由 write_job_log 覆盖写入，这里只做追加。
async fn append_split_log(ctx: &Ctx, id: &str, line: &str) {
    let Some(log_path) = get_job(ctx, id).await.and_then(|job| job.log_path) else {
        return;
    };
    use std::io::Write as _;
    if let Ok(mut file) = std::fs::OpenOptions::new().append(true).open(&log_path) {
        let _ = write!(file, "\n--- 切分时长核对 ---\n{line}\n");
    }
}

/// 按时间点切分单段。
/// copy 只能切在关键帧，实测偏移会写入任务消息与日志；encode 走重编码，帧级准确。
#[allow(clippy::too_many_arguments)]
async fn run_split(
    ctx: Ctx,
    id: String,
    job: Job,
    ffmpeg: String,
    caps: Option<probe::Capabilities>,
    ffprobe: Option<String>,
    temp: PathBuf,
) {
    let split = split_params(&job.params);
    let encoder = pick_sequence_encoder(caps.as_ref());
    let requested = (split.end_sec - split.start_sec).max(0.0);
    let argv = args::build_split(
        &job.input,
        temp.to_string_lossy().as_ref(),
        &split,
        &encoder,
    );
    let mode_label = if split.mode == "encode" {
        "重编码"
    } else {
        "流复制"
    };
    let _ = set_job(&ctx, &id, |job| {
        job.message = Some(format!(
            "正在{}切分 {}s–{}s…",
            mode_label,
            fmt_sec_short(split.start_sec),
            fmt_sec_short(split.end_sec)
        ));
    })
    .await;

    let outcome = execute_streaming(&ctx, &id, &ffmpeg, argv, Some(requested), (0.0, 0.95)).await;
    if !outcome.success {
        let (kind, message) = if outcome.killed {
            ("cancelled".to_string(), "已取消".to_string())
        } else {
            let (kind, err) = classify(&outcome.stderr);
            (kind, format!("{mode_label}切分失败：{err}"))
        };
        let _ = set_job(&ctx, &id, |job| {
            job.status = if kind == "cancelled" {
                "cancelled".into()
            } else {
                "failed".into()
            };
            job.error_kind = Some(kind.clone());
            job.message = Some(message.clone());
            job.finished_at = Some(now_ms());
        })
        .await;
        write_job_log(
            &ctx,
            &id,
            if kind == "cancelled" {
                "cancelled"
            } else {
                "failed"
            },
            &[],
            &outcome.stderr,
        )
        .await;
        if kind != "cancelled" {
            maybe_auto_retry(&ctx, &id, &kind).await;
        }
        let _ = std::fs::remove_file(&temp);
        finish_slot(&ctx, &id).await;
        return;
    }

    // 用 ffprobe 复核输出实测时长，偏移写进消息与日志（copy 的 GOP 偏移属正常）。
    let actual = match ffprobe.as_deref() {
        Some(bin) => {
            probe::probe_media(bin, temp.to_string_lossy().as_ref())
                .await
                .duration_sec
        }
        None => None,
    };
    let offset = actual.map(|value| value - requested).unwrap_or(0.0);
    let message = match actual {
        Some(actual) if split.mode == "encode" => format!(
            "重编码切分：请求 {}–{}s，实测 {:.3}s，偏移 {offset:+.3}s",
            fmt_sec_short(split.start_sec),
            fmt_sec_short(split.end_sec),
            actual
        ),
        Some(actual) => format!(
            "流复制切分：请求 {}–{}s，实测 {:.3}s，偏移 {offset:+.3}s（-c copy 只能切在关键帧，偏移属正常）",
            fmt_sec_short(split.start_sec),
            fmt_sec_short(split.end_sec),
            actual
        ),
        None => format!(
            "{}切分已完成（未找到 ffprobe，无法核对实测时长）",
            mode_label
        ),
    };
    let detail = match actual {
        Some(actual) => format!(
            "请求 {:.3}s，实测 {:.3}s，偏移 {:+.3}s（模式 {}）",
            requested, actual, offset, split.mode
        ),
        None => format!(
            "请求 {:.3}s，模式 {}，未取得实测时长",
            requested, split.mode
        ),
    };

    if std::fs::metadata(&temp).map(|m| m.len()).unwrap_or(0) == 0 {
        let message = format!("{mode_label}切分没有产生输出文件");
        let _ = set_job(&ctx, &id, |job| {
            job.status = "failed".into();
            job.error_kind = Some("output".into());
            job.message = Some(message.clone());
            job.finished_at = Some(now_ms());
        })
        .await;
        write_job_log(&ctx, &id, "failed", &[], &outcome.stderr).await;
        let _ = std::fs::remove_file(&temp);
        finish_slot(&ctx, &id).await;
        return;
    }

    let target = PathBuf::from(&job.output);
    if target.is_file() {
        let _ = std::fs::remove_file(&target);
    }
    if let Err(e) = std::fs::rename(&temp, &target) {
        let message = format!("写入输出失败（{}）：{e}", target.display());
        let _ = set_job(&ctx, &id, |job| {
            job.status = "failed".into();
            job.error_kind = Some("output".into());
            job.message = Some(message.clone());
            job.finished_at = Some(now_ms());
        })
        .await;
        write_job_log(&ctx, &id, "failed", &[], &outcome.stderr).await;
        let _ = std::fs::remove_file(&temp);
        finish_slot(&ctx, &id).await;
        return;
    }

    let size = std::fs::metadata(&target).map(|m| m.len()).unwrap_or(0);
    let _ = set_job(&ctx, &id, |job| {
        job.status = "done".into();
        job.progress = 1.0;
        job.output = target.to_string_lossy().to_string();
        job.output_size = Some(size);
        job.message = Some(message.clone());
        job.duration_sec = actual;
        job.encoder_used = Some(if split.mode == "encode" {
            encoder.clone()
        } else {
            "copy".into()
        });
        job.finished_at = Some(now_ms());
    })
    .await;
    write_job_log(&ctx, &id, "done", &[], &outcome.stderr).await;
    append_split_log(&ctx, &id, &detail).await;
    finish_slot(&ctx, &id).await;
}

/// 多图转 GIF：palettegen（第 1 遍）+ paletteuse（第 2 遍）。
/// concat 清单与调色板写在临时目录，结束后清理；失败消息注明是第几遍。
async fn run_image_gif(
    ctx: Ctx,
    id: String,
    job: Job,
    ffmpeg: String,
    ffprobe: Option<String>,
    dir: PathBuf,
    temp: PathBuf,
) {
    let p: args::ImageGifParams = serde_json::from_value(job.params.clone()).unwrap_or_default();
    let inputs = if job.inputs.is_empty() {
        vec![job.input.clone()]
    } else {
        job.inputs.clone()
    };
    let tmpdir = dir.join(format!(".ff-tmp-gif-{id}"));
    let _ = std::fs::remove_dir_all(&tmpdir);

    let fail = |ctx: Ctx, id: String, tmpdir: PathBuf, kind: &str, message: String| {
        let kind = kind.to_string();
        async move {
            let _ = set_job(&ctx, &id, |job| {
                job.status = "failed".into();
                job.error_kind = Some(kind.clone());
                job.message = Some(message.clone());
                job.finished_at = Some(now_ms());
            })
            .await;
            write_job_log(&ctx, &id, "failed", &[], &message).await;
            let _ = std::fs::remove_dir_all(&tmpdir);
            finish_slot(&ctx, &id).await;
        }
    };

    if inputs.len() < 2 {
        fail(
            ctx,
            id,
            tmpdir,
            "invalid-params",
            "图片转 GIF 至少需要 2 张图片".to_string(),
        )
        .await;
        return;
    }
    if let Err(e) = std::fs::create_dir_all(&tmpdir) {
        fail(ctx, id, tmpdir, "output", format!("无法创建临时目录：{e}")).await;
        return;
    }
    let list = tmpdir.join("list.txt");
    let palette = tmpdir.join("palette.png");
    if let Err(e) = std::fs::write(&list, args::gif_concat_list(&inputs, p.fps)) {
        fail(ctx, id, tmpdir, "output", format!("无法写入 GIF 清单：{e}")).await;
        return;
    }

    let first_width = match ffprobe.as_deref() {
        Some(bin) => probe::probe_media(bin, &inputs[0]).await.width.unwrap_or(0),
        None => 0,
    };
    let width = args::gif_target_width(p.max_width, first_width);
    if width == 0 {
        fail(
            ctx,
            id,
            tmpdir,
            "input",
            "无法读取首图宽度：请填写最大宽度后重试".to_string(),
        )
        .await;
        return;
    }

    let _ = set_job(&ctx, &id, |job| {
        job.progress = 0.05;
        job.message = Some("正在生成全局调色板（第 1/2 遍 palettegen）…".into());
    })
    .await;
    let pass1 = args::build_gif_pass1(
        list.to_string_lossy().as_ref(),
        palette.to_string_lossy().as_ref(),
        &p,
        width,
    );
    let outcome1 = execute_streaming(&ctx, &id, &ffmpeg, pass1, None, (0.05, 0.5)).await;
    if !outcome1.success {
        let (kind, message) = if outcome1.killed {
            ("cancelled".to_string(), "已取消".to_string())
        } else {
            let (kind, err) = classify(&outcome1.stderr);
            (kind, format!("第 1 遍（palettegen）失败：{err}"))
        };
        if kind == "cancelled" {
            let _ = set_job(&ctx, &id, |job| {
                job.status = "cancelled".into();
                job.error_kind = Some(kind.clone());
                job.message = Some(message.clone());
                job.finished_at = Some(now_ms());
            })
            .await;
            write_job_log(&ctx, &id, "cancelled", &[], &outcome1.stderr).await;
            let _ = std::fs::remove_dir_all(&tmpdir);
            finish_slot(&ctx, &id).await;
        } else {
            fail(ctx, id, tmpdir, &kind, message).await;
        }
        return;
    }
    // 调色板文件是「两遍确实执行」的硬证据；缺失说明第 1 遍没有真正产出。
    if !palette.is_file() {
        fail(
            ctx,
            id,
            tmpdir,
            "output",
            "第 1 遍（palettegen）没有生成调色板文件".to_string(),
        )
        .await;
        return;
    }

    let _ = set_job(&ctx, &id, |job| {
        job.progress = 0.5;
        job.message = Some("正在合成 GIF（第 2/2 遍 paletteuse）…".into());
    })
    .await;
    let mut pass2 = args::build_gif_pass2(
        list.to_string_lossy().as_ref(),
        palette.to_string_lossy().as_ref(),
        temp.to_string_lossy().as_ref(),
        &p,
        width,
    );
    // concat 末行重复会多出一帧，按图片张数封顶。
    args::limit_gif_frames(&mut pass2, inputs.len() as u32);
    let outcome2 = execute_streaming(&ctx, &id, &ffmpeg, pass2, None, (0.5, 0.95)).await;
    if !outcome2.success {
        let (kind, message) = if outcome2.killed {
            ("cancelled".to_string(), "已取消".to_string())
        } else {
            let (kind, err) = classify(&outcome2.stderr);
            (kind, format!("第 2 遍（paletteuse）失败：{err}"))
        };
        if kind == "cancelled" {
            let _ = set_job(&ctx, &id, |job| {
                job.status = "cancelled".into();
                job.error_kind = Some(kind.clone());
                job.message = Some(message.clone());
                job.finished_at = Some(now_ms());
            })
            .await;
            write_job_log(&ctx, &id, "cancelled", &[], &outcome2.stderr).await;
            let _ = std::fs::remove_dir_all(&tmpdir);
            finish_slot(&ctx, &id).await;
        } else {
            fail(ctx, id, tmpdir, &kind, message).await;
        }
        return;
    }

    let target = PathBuf::from(&job.output);
    if target.is_file() {
        let _ = std::fs::remove_file(&target);
    }
    if let Err(e) = std::fs::rename(&temp, &target) {
        fail(
            ctx,
            id,
            tmpdir,
            "output",
            format!("写入输出失败（{}）：{e}", target.display()),
        )
        .await;
        return;
    }
    let size = std::fs::metadata(&target).map(|m| m.len()).unwrap_or(0);
    let message = format!(
        "已生成 GIF（{} fps，{}px 宽，{} 张图，2 遍调色板）",
        p.fps,
        width,
        inputs.len()
    );
    let _ = set_job(&ctx, &id, |job| {
        job.status = "done".into();
        job.progress = 1.0;
        job.output = target.to_string_lossy().to_string();
        job.output_size = Some(size);
        job.message = Some(message.clone());
        job.finished_at = Some(now_ms());
        job.encoder_used = Some("palettegen+paletteuse".into());
    })
    .await;
    let stderr = format!(
        "--- 第 1 遍 palettegen ---\n{}\n--- 第 2 遍 paletteuse ---\n{}",
        outcome1.stderr, outcome2.stderr
    );
    write_job_log(&ctx, &id, "done", &[], &stderr).await;
    let _ = std::fs::remove_dir_all(&tmpdir);
    finish_slot(&ctx, &id).await;
}

/// 读取「最终画面」与「水印源图」的真实尺寸。水印开启后任一步失败都返回可读原因，
/// 由 run_job 直接失败任务（缺失/损坏的水印绝不能被静默跳过）。
async fn watermark_dims_for_job(
    probe_bin: Option<&str>,
    job: &Job,
) -> Result<Option<args::WatermarkDims>, String> {
    let wm = match job.kind.as_str() {
        "video-mp4" => {
            serde_json::from_value::<args::VideoParams>(job.params.clone())
                .map_err(|e| format!("视频参数格式错误：{e}"))?
                .watermark
        }
        "image" | "image-compress" => {
            serde_json::from_value::<args::ImageParams>(job.params.clone())
                .map_err(|e| format!("图片参数格式错误：{e}"))?
                .watermark
        }
        _ => return Ok(None),
    };
    if !wm.enabled {
        return Ok(None);
    }
    let bin =
        probe_bin.ok_or_else(|| "找不到 ffprobe，无法读取水印叠加所需的画面尺寸".to_string())?;
    let base = probe::probe_media(bin, &job.input).await;
    let (src_w, src_h) = match (base.width, base.height) {
        (Some(w), Some(h)) if w > 0 && h > 0 => (w, h),
        _ => return Err(format!("无法读取输入画面尺寸：{}", job.input)),
    };
    let (base_w, base_h) = if job.kind == "video-mp4" {
        let p: args::VideoParams = serde_json::from_value(job.params.clone())
            .map_err(|e| format!("视频参数格式错误：{e}"))?;
        args::video_base_dims(&p, src_w, src_h)
    } else {
        let p: args::ImageParams = serde_json::from_value(job.params.clone())
            .map_err(|e| format!("图片参数格式错误：{e}"))?;
        args::image_base_dims(&p, &job.input, src_w, src_h)
    };
    let info = probe::probe_media(bin, &wm.path).await;
    let (wm_w, wm_h) = match (info.width, info.height) {
        (Some(w), Some(h)) if w > 0 && h > 0 => (w, h),
        _ => {
            return Err(format!(
                "无法读取水印图片尺寸：{}（文件可能已损坏，请重新选择）",
                wm.path
            ))
        }
    };
    Ok(Some(args::WatermarkDims {
        base_width: base_w,
        base_height: base_h,
        wm_width: wm_w,
        wm_height: wm_h,
    }))
}

/// 烧录字幕的字号按「最终画面高度 × 百分比」换算，运行前必须拿到真实高度。
async fn subtitle_base_height_for_job(
    probe_bin: Option<&str>,
    job: &Job,
) -> Result<Option<u32>, String> {
    if job.kind != "video-mp4" {
        return Ok(None);
    }
    let p: args::VideoParams =
        serde_json::from_value(job.params.clone()).map_err(|e| format!("视频参数格式错误：{e}"))?;
    if !p.subtitle.enabled || args::subtitle_mode(&p.subtitle) != "hard" {
        return Ok(None);
    }
    let bin =
        probe_bin.ok_or_else(|| "找不到 ffprobe，无法读取烧录字幕所需的画面高度".to_string())?;
    let info = probe::probe_media(bin, &job.input).await;
    match (info.width, info.height) {
        (Some(w), Some(h)) if w > 0 && h > 0 => Ok(Some(args::video_base_dims(&p, w, h).1)),
        _ => Err(format!("无法读取输入画面尺寸：{}", job.input)),
    }
}

/// 把每轮「质量 → 字节」追加到 per-job 诊断日志；无日志目录时静默跳过。
async fn append_compress_log(ctx: &Ctx, id: &str, log: &[(u32, u64)]) {
    let Some(job) = get_job(ctx, id).await else {
        return;
    };
    let Some(path) = job.log_path else {
        return;
    };
    use std::io::Write;
    if let Ok(mut file) = std::fs::OpenOptions::new().append(true).open(&path) {
        let mut text = String::from("\n--- 压缩迭代（质量, 字节）---\n");
        for (quality, bytes) in log {
            text.push_str(&format!("q={quality} bytes={bytes}\n"));
        }
        let _ = file.write_all(text.as_bytes());
    }
}

/// 图片压缩任务：同步跑完最多 8 轮重编码（每轮独立临时文件），
/// 成功发布后把目标/达标/尝试次数/最终质量写回 Job。
async fn run_compress(ctx: Ctx, id: String, job: Job, ffmpeg: String, dir: PathBuf, temp: PathBuf) {
    let mut p: args::ImageParams = serde_json::from_value(job.params.clone()).unwrap_or_default();
    let probe_bin = {
        let tools = ctx.state.tools.lock().await;
        tools
            .as_ref()
            .and_then(|t| t.ffprobe.as_ref())
            .map(|t| t.path.clone())
    };
    match watermark_dims_for_job(probe_bin.as_deref(), &job).await {
        Ok(dims) => {
            if let Some(dims) = dims {
                p.watermark.base_width = dims.base_width;
                p.watermark.base_height = dims.base_height;
                p.watermark.wm_width = dims.wm_width;
                p.watermark.wm_height = dims.wm_height;
            }
        }
        Err(message) => {
            remove_temp_file(&temp).await;
            set_job(&ctx, &id, |job| {
                job.status = "failed".into();
                job.error_kind = Some("invalid-params".into());
                job.message = Some(message.clone());
                job.finished_at = Some(now_ms());
            })
            .await;
            write_job_log(&ctx, &id, "failed", &[], &message).await;
            finish_slot(&ctx, &id).await;
            return;
        }
    }

    let original_bytes = match std::fs::metadata(&job.input) {
        Ok(meta) => meta.len(),
        Err(err) => {
            let message = format!("无法读取源文件大小：{err}");
            set_job(&ctx, &id, |job| {
                job.status = "failed".into();
                job.error_kind = Some("input".into());
                job.message = Some(message.clone());
                job.finished_at = Some(now_ms());
            })
            .await;
            write_job_log(&ctx, &id, "failed", &[], &message).await;
            finish_slot(&ctx, &id).await;
            return;
        }
    };

    let _ = set_job(&ctx, &id, |job| {
        job.message = Some("正在压缩图片…".into());
    })
    .await;

    let ffmpeg_bin = ffmpeg.clone();
    let input_path = job.input.clone();
    let output_path = temp.to_string_lossy().to_string();
    let work_dir = dir.clone();
    let job_key = id.clone();
    let params = p.clone();
    let result = tokio::task::spawn_blocking(move || {
        let request = crate::compress::CompressRequest {
            ffmpeg: &ffmpeg_bin,
            input: &input_path,
            output: &output_path,
            work_dir: &work_dir,
            job_id: &job_key,
            params: &params,
            original_bytes,
        };
        crate::compress::compress_image(&request)
    })
    .await;

    let report = match result {
        Ok(Ok(report)) => report,
        Ok(Err(message)) => {
            let (kind, message) = classify(&message);
            remove_temp_file(&temp).await;
            set_job(&ctx, &id, |job| {
                job.status = "failed".into();
                job.error_kind = Some(kind.clone());
                job.message = Some(message.clone());
                job.finished_at = Some(now_ms());
                job.progress = 0.0;
            })
            .await;
            write_job_log(&ctx, &id, "failed", &[], &message).await;
            finish_slot(&ctx, &id).await;
            return;
        }
        Err(err) => {
            let message = format!("压缩任务异常：{err}");
            remove_temp_file(&temp).await;
            set_job(&ctx, &id, |job| {
                job.status = "failed".into();
                job.error_kind = Some("unknown".into());
                job.message = Some(message.clone());
                job.finished_at = Some(now_ms());
                job.progress = 0.0;
            })
            .await;
            write_job_log(&ctx, &id, "failed", &[], &message).await;
            finish_slot(&ctx, &id).await;
            return;
        }
    };

    let ext = Path::new(&job.output)
        .extension()
        .map(|e| e.to_string_lossy().to_string())
        .unwrap_or_else(|| "jpg".into());
    let size = match if job.conflict == "overwrite" {
        rename_overwrite(&temp, Path::new(&job.output))
    } else {
        rename_atomic(&temp, Path::new(&job.output))
    } {
        Ok(size) => size,
        Err(err) if job.conflict == "overwrite" => {
            remove_temp_file(&temp).await;
            let message = format!("覆盖已有文件失败：{err}");
            set_job(&ctx, &id, |job| {
                job.status = "failed".into();
                job.error_kind = Some("output".into());
                job.message = Some(message.clone());
                job.finished_at = Some(now_ms());
                job.progress = 0.0;
            })
            .await;
            write_job_log(&ctx, &id, "failed", &[], &message).await;
            finish_slot(&ctx, &id).await;
            return;
        }
        Err(_) => {
            let used: Vec<PathBuf> = {
                let inner = ctx.state.inner.lock().await;
                inner
                    .jobs
                    .iter()
                    .filter(|other| other.id != id)
                    .map(|other| PathBuf::from(&other.output))
                    .collect()
            };
            let target = unique_output(&dir, file_stem(&job.output).as_str(), &ext, &used);
            match rename_atomic(&temp, &target) {
                Ok(size) => {
                    let _ = set_job(&ctx, &id, |job| {
                        job.output = target.to_string_lossy().to_string();
                    })
                    .await;
                    size
                }
                Err(err) => {
                    remove_temp_file(&temp).await;
                    let message = format!("写出最终文件失败：{err}");
                    set_job(&ctx, &id, |job| {
                        job.status = "failed".into();
                        job.error_kind = Some("output".into());
                        job.message = Some(message.clone());
                        job.finished_at = Some(now_ms());
                    })
                    .await;
                    write_job_log(&ctx, &id, "failed", &[], &message).await;
                    finish_slot(&ctx, &id).await;
                    return;
                }
            }
        }
    };

    let _ = set_job(&ctx, &id, |job| {
        job.status = "done".into();
        job.progress = 1.0;
        job.output_size = Some(size);
        job.target_bytes = report.target_bytes;
        job.target_met = report.target_met;
        job.attempts_used = Some(report.iterations);
        job.quality_used = Some(report.quality);
        job.encoder_used = Some("image-compress".into());
        job.finished_at = Some(now_ms());
        job.message = Some(report.message.clone());
    })
    .await;
    write_job_log(
        &ctx,
        &id,
        "done",
        &[format!("压缩尝试 {} 次", report.iterations)],
        "",
    )
    .await;
    append_compress_log(&ctx, &id, &report.log).await;
    finish_slot(&ctx, &id).await;
}

async fn run_job(ctx: Ctx, id: String) {
    let state = ctx.state.clone();
    let job = match get_job(&ctx, &id).await {
        Some(job) => job,
        None => {
            finish_slot(&ctx, &id).await;
            return;
        }
    };

    let tools_snapshot = { state.tools.lock().await.clone() };
    let ffmpeg = match tools_snapshot.as_ref().and_then(|t| t.ffmpeg.as_ref()) {
        Some(t) => t.path.clone(),
        None => {
            set_job(&ctx, &id, |job| {
                job.status = "failed".into();
                job.error_kind = Some("encoder-missing".into());
                job.message = Some("未找到 ffmpeg".into());
                job.finished_at = Some(now_ms());
            })
            .await;
            write_job_log(&ctx, &id, "failed", &[], "").await;
            finish_slot(&ctx, &id).await;
            return;
        }
    };

    let (caps, allow_gpl) = {
        let inner = state.inner.lock().await;
        // 变体策略在编译期钉死：GPL 变体恒允许 GPL 编码器，LGPL 版不接受。
        (
            inner.caps.clone(),
            crate::flavor::allow_gpl_effective(inner.allow_gpl),
        )
    };
    // 拿不到能力探测结果时按不可用处理：烧录字幕必须有 libass。
    let subtitle_burn = caps.as_ref().map(|c| c.subtitle_burn).unwrap_or(false);

    let dir = Path::new(&job.output)
        .parent()
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| std::env::temp_dir());
    let ext = Path::new(&job.output)
        .extension()
        .map(|e| e.to_string_lossy().to_string())
        .unwrap_or_else(|| "mp4".into());
    let temp = temp_path(&dir, &id, &ext);
    let _ = std::fs::remove_file(&temp);

    // 多步任务走独立执行器：合并 / 混流 / GIF 需要多条 ffmpeg 命令或额外的输入校验。
    if matches!(job.kind.as_str(), "video-merge" | "video-mux" | "video-gif") {
        run_composite(
            ctx.clone(),
            id.clone(),
            job.clone(),
            ffmpeg.clone(),
            caps.clone(),
            allow_gpl,
            dir.clone(),
            temp.clone(),
        )
        .await;
        return;
    }

    // PDF 任务：合并/拆分在进程内完成，转图片调用外部 pdftoppm。
    if matches!(job.kind.as_str(), "pdf-merge" | "pdf-split" | "pdf-image") {
        run_pdf(
            ctx.clone(),
            id.clone(),
            job.clone(),
            dir.clone(),
            temp.clone(),
        )
        .await;
        return;
    }

    // 视频 → 图片序列是多文件产物，单独走一个执行器。
    if job.kind == "video-frames" {
        run_frames(
            ctx.clone(),
            id.clone(),
            job.clone(),
            ffmpeg.clone(),
            dir.clone(),
        )
        .await;
        return;
    }

    let ffprobe = tools_snapshot
        .as_ref()
        .and_then(|t| t.ffprobe.as_ref())
        .map(|p| p.path.clone());

    // 按时间点切分：单段一个任务，跑完后用 ffprobe 核对实测时长。
    if job.kind == "video-split" {
        run_split(
            ctx.clone(),
            id.clone(),
            job.clone(),
            ffmpeg.clone(),
            caps.clone(),
            ffprobe.clone(),
            temp.clone(),
        )
        .await;
        return;
    }

    // 多图转 GIF：palettegen + paletteuse 两遍，临时目录存放 concat 清单与调色板。
    if job.kind == "image-gif" {
        run_image_gif(
            ctx.clone(),
            id.clone(),
            job.clone(),
            ffmpeg.clone(),
            ffprobe.clone(),
            dir.clone(),
            temp.clone(),
        )
        .await;
        return;
    }

    // 图片压缩需要多轮重编码 + 逐轮量字节，走独立的二分执行器。
    if job.kind == "image-compress" {
        run_compress(
            ctx.clone(),
            id.clone(),
            job.clone(),
            ffmpeg.clone(),
            dir.clone(),
            temp.clone(),
        )
        .await;
        return;
    }

    let mut attempts_info: Vec<String> = Vec::new();
    let mut last_kind = "unknown".to_string();
    let mut last_message = String::new();
    let mut used_encoder: Option<String> = None;

    // 序列 / 拼图的命令与编码器链无关，提前构造；参数错误在这里直接失败。
    let special_argv: Option<Vec<String>> = match job.kind.as_str() {
        "image-seq-video" => {
            let result: Result<Vec<String>, String> = async {
                let mut p: args::SeqParams = serde_json::from_value(job.params.clone())
                    .map_err(|e| format!("序列参数格式错误：{e}"))?;
                if p.encoder.trim().is_empty() || p.encoder == "auto" {
                    p.encoder = pick_sequence_encoder(caps.as_ref());
                }
                let (pattern, start, count) = args::validate_sequence(&job.inputs)?;
                p.start_number = start;
                let probe_bin = tools_snapshot
                    .as_ref()
                    .and_then(|t| t.ffprobe.as_ref())
                    .map(|p| p.path.clone());
                let mut dims = Vec::new();
                for input in &job.inputs {
                    let Some(bin) = probe_bin.as_deref() else {
                        return Err("找不到 ffprobe，无法读取图片尺寸".into());
                    };
                    let info = probe::probe_media(bin, input).await;
                    match (info.width, info.height) {
                        (Some(w), Some(h)) if w > 0 && h > 0 => dims.push((w, h)),
                        _ => return Err(format!("无法读取图片尺寸：{input}")),
                    }
                }
                let target = args::seq_target_size(&p, &dims)?;
                Ok(args::build_seq_video(
                    &pattern,
                    temp.to_string_lossy().as_ref(),
                    &p,
                    target,
                    count,
                ))
            }
            .await;
            match result {
                Ok(argv) => Some(argv),
                Err(message) => {
                    set_job(&ctx, &id, |job| {
                        job.status = "failed".into();
                        job.error_kind = Some("invalid-params".into());
                        job.message = Some(message.clone());
                        job.finished_at = Some(now_ms());
                    })
                    .await;
                    write_job_log(&ctx, &id, "failed", &[], &message).await;
                    finish_slot(&ctx, &id).await;
                    return;
                }
            }
        }
        "image-collage" => {
            let p: args::CollageParams =
                serde_json::from_value(job.params.clone()).unwrap_or_default();
            match args::build_collage(&job.inputs, temp.to_string_lossy().as_ref(), &p) {
                Ok(argv) => Some(argv),
                Err(message) => {
                    set_job(&ctx, &id, |job| {
                        job.status = "failed".into();
                        job.error_kind = Some("invalid-params".into());
                        job.message = Some(message.clone());
                        job.finished_at = Some(now_ms());
                    })
                    .await;
                    write_job_log(&ctx, &id, "failed", &[], &message).await;
                    finish_slot(&ctx, &id).await;
                    return;
                }
            }
        }
        _ => None,
    };

    let chain: Vec<String> = match job.kind.as_str() {
        "video-mp4" => {
            let p: args::VideoParams =
                serde_json::from_value(job.params.clone()).unwrap_or_default();
            let codec = if p.codec.is_empty() { "h264" } else { &p.codec };
            let chain = encoder_chain(caps.as_ref(), codec, &p.encoder, allow_gpl);
            if chain.is_empty() {
                let missing_reason = caps
                    .as_ref()
                    .map(|c| {
                        c.video_encoders
                            .iter()
                            .filter(|e| e.codec == codec && !e.available)
                            .map(|e| format!("{}：{}", e.id, e.reason.clone().unwrap_or_default()))
                            .collect::<Vec<_>>()
                            .join("；")
                    })
                    .unwrap_or_default();
                set_job(&ctx, &id, |job| {
                    job.status = "failed".into();
                    job.error_kind = Some("encoder-missing".into());
                    job.message = Some(format!(
                        "没有可用的 {codec} 编码器。{} 可在设置中重新探测硬件能力，或换成本机可用的编码器。",
                        if missing_reason.is_empty() { String::new() } else { format!("原因：{missing_reason}。") }
                    ));
                    job.finished_at = Some(now_ms());
                })
                .await;
                write_job_log(&ctx, &id, "failed", &[], "").await;
                finish_slot(&ctx, &id).await;
                return;
            }
            chain
        }
        "audio-mp3" | "audio-aac" => {
            let p: args::AudioParams =
                serde_json::from_value(job.params.clone()).unwrap_or_default();
            let wanted = args::audio_encoder(&p);
            let usable = caps
                .as_ref()
                .map(|c| {
                    c.audio_encoders
                        .iter()
                        .filter(|e| e.compiled && e.allowed && e.available)
                        .map(|e| e.id.clone())
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();
            let mut chain = Vec::new();
            if usable.contains(&wanted.to_string()) || caps.is_none() {
                chain.push(wanted.to_string());
            }
            if let Some(spec) = containers::find_audio(&p.format) {
                for alt in spec.encoders {
                    if *alt != wanted && usable.contains(&alt.to_string()) {
                        chain.push(alt.to_string());
                    }
                }
            }
            if chain.is_empty() {
                chain.push(wanted.to_string());
            }
            chain
        }
        _ => vec![String::new()],
    };

    // 水印位置依赖「最终画面 + 水印源图」的真实尺寸；探测失败时任务直接失败。
    let watermark_dims = {
        let probe_bin = tools_snapshot
            .as_ref()
            .and_then(|t| t.ffprobe.as_ref())
            .map(|p| p.path.as_str());
        match watermark_dims_for_job(probe_bin, &job).await {
            Ok(dims) => dims,
            Err(message) => {
                remove_temp_file(&temp).await;
                set_job(&ctx, &id, |job| {
                    job.status = "failed".into();
                    job.error_kind = Some("invalid-params".into());
                    job.message = Some(message);
                    job.finished_at = Some(now_ms());
                })
                .await;
                write_job_log(&ctx, &id, "failed", &[], "").await;
                finish_slot(&ctx, &id).await;
                return;
            }
        }
    };

    // 烧录字幕的字号依赖真实输出高度；探测失败时任务直接失败。
    let subtitle_base_height = {
        let probe_bin = tools_snapshot
            .as_ref()
            .and_then(|t| t.ffprobe.as_ref())
            .map(|p| p.path.as_str());
        match subtitle_base_height_for_job(probe_bin, &job).await {
            Ok(height) => height,
            Err(message) => {
                remove_temp_file(&temp).await;
                set_job(&ctx, &id, |job| {
                    job.status = "failed".into();
                    job.error_kind = Some("invalid-params".into());
                    job.message = Some(message);
                    job.finished_at = Some(now_ms());
                })
                .await;
                write_job_log(&ctx, &id, "failed", &[], "").await;
                finish_slot(&ctx, &id).await;
                return;
            }
        }
    };

    let mut last_stderr = String::new();
    for (idx, encoder) in chain.iter().enumerate() {
        let cancelled = {
            let inner = state.inner.lock().await;
            inner.cancel_requested.contains(&id)
        };
        if cancelled {
            break;
        }

        let argv = match special_argv.clone() {
            Some(argv) => argv,
            None => match job.kind.as_str() {
                "video-mp4" => {
                    let mut p: args::VideoParams =
                        serde_json::from_value(job.params.clone()).unwrap_or_default();
                    if let Some(dims) = watermark_dims {
                        p.watermark.base_width = dims.base_width;
                        p.watermark.base_height = dims.base_height;
                        p.watermark.wm_width = dims.wm_width;
                        p.watermark.wm_height = dims.wm_height;
                    }
                    p.subtitle.base_height = subtitle_base_height.unwrap_or(0);
                    if let Err(message) = args::validate_subtitle_burn(&p.subtitle, subtitle_burn) {
                        remove_temp_file(&temp).await;
                        set_job(&ctx, &id, |job| {
                            job.status = "failed".into();
                            job.error_kind = Some("invalid-params".into());
                            job.message = Some(message);
                            job.finished_at = Some(now_ms());
                        })
                        .await;
                        write_job_log(&ctx, &id, "failed", &[], "").await;
                        finish_slot(&ctx, &id).await;
                        return;
                    }
                    let accel = if p.hw_decode {
                        best_decode_accel(caps.as_ref())
                    } else {
                        None
                    };
                    // 容器决定可用的音频编码：webm 只能用 opus/vorbis，mp4/mov 用 aac。
                    let usable_audio = caps
                        .as_ref()
                        .map(|c| {
                            c.audio_encoders
                                .iter()
                                .filter(|e| e.compiled && e.allowed && e.available)
                                .map(|e| e.id.clone())
                                .collect::<Vec<_>>()
                        })
                        .unwrap_or_default();
                    let container_audio =
                        containers::pick_container_audio_encoder(&p.container, "", &usable_audio);
                    match args::build_video(
                        encoder,
                        &job.input,
                        temp.to_string_lossy().as_ref(),
                        &p,
                        container_audio,
                        accel.as_deref(),
                    ) {
                        Ok(argv) => argv,
                        Err(message) => {
                            remove_temp_file(&temp).await;
                            set_job(&ctx, &id, |job| {
                                job.status = "failed".into();
                                job.error_kind = Some("invalid-params".into());
                                job.message = Some(message);
                                job.finished_at = Some(now_ms());
                            })
                            .await;
                            write_job_log(&ctx, &id, "failed", &attempts_info, "").await;
                            finish_slot(&ctx, &id).await;
                            return;
                        }
                    }
                }
                "audio-mp3" | "audio-aac" => {
                    let p: args::AudioParams =
                        serde_json::from_value(job.params.clone()).unwrap_or_default();
                    args::build_audio(encoder, &job.input, temp.to_string_lossy().as_ref(), &p)
                }
                _ => {
                    let mut p: args::ImageParams =
                        serde_json::from_value(job.params.clone()).unwrap_or_default();
                    if let Some(dims) = watermark_dims {
                        p.watermark.base_width = dims.base_width;
                        p.watermark.base_height = dims.base_height;
                        p.watermark.wm_width = dims.wm_width;
                        p.watermark.wm_height = dims.wm_height;
                    }
                    match args::build_image(&job.input, temp.to_string_lossy().as_ref(), &p) {
                        Ok(argv) => argv,
                        Err(message) => {
                            remove_temp_file(&temp).await;
                            set_job(&ctx, &id, |job| {
                                job.status = "failed".into();
                                job.error_kind = Some("invalid-params".into());
                                job.message = Some(message);
                                job.finished_at = Some(now_ms());
                            })
                            .await;
                            write_job_log(&ctx, &id, "failed", &attempts_info, "").await;
                            finish_slot(&ctx, &id).await;
                            return;
                        }
                    }
                }
            },
        };

        let label = if encoder.is_empty() {
            "image".to_string()
        } else {
            encoder.clone()
        };
        let _ = set_job(&ctx, &id, |job| {
            job.message = Some(format!("正在使用 {label} 转换…"));
        })
        .await;

        let outcome =
            execute_streaming(&ctx, &id, &ffmpeg, argv, job.duration_sec, (0.0, 1.0)).await;
        last_stderr = outcome.stderr.clone();

        let cancelled_now = {
            let inner = state.inner.lock().await;
            inner.cancel_requested.contains(&id)
        };
        if outcome.killed || cancelled_now {
            remove_temp_file(&temp).await;
            set_job(&ctx, &id, |job| {
                job.status = "cancelled".into();
                job.finished_at = Some(now_ms());
                job.progress = 0.0;
                job.message = Some("已取消".into());
                job.speed = None;
                job.eta_sec = None;
            })
            .await;
            write_job_log(&ctx, &id, "cancelled", &attempts_info, &last_stderr).await;
            finish_slot(&ctx, &id).await;
            return;
        }

        if outcome.success {
            used_encoder = Some(label.clone());
            // 显式「覆盖」策略允许替换已有文件（内部先备份再落位），其余策略一律拒绝覆盖。
            let first_publish = if job.conflict == "overwrite" {
                rename_overwrite(&temp, Path::new(&job.output))
            } else {
                rename_atomic(&temp, Path::new(&job.output))
            };
            let size = match first_publish {
                Ok(size) => size,
                // 覆盖是显式选择：失败就如实报错，绝不悄悄改成「加序号」。
                Err(err) if job.conflict == "overwrite" => {
                    remove_temp_file(&temp).await;
                    set_job(&ctx, &id, |job| {
                        job.status = "failed".into();
                        job.error_kind = Some("output".into());
                        job.message = Some(format!("覆盖已有文件失败：{err}"));
                        job.finished_at = Some(now_ms());
                        job.progress = 0.0;
                    })
                    .await;
                    write_job_log(&ctx, &id, "failed", &attempts_info, &last_stderr).await;
                    finish_slot(&ctx, &id).await;
                    return;
                }
                Err(_) => {
                    let used: Vec<PathBuf> = {
                        let inner = state.inner.lock().await;
                        inner
                            .jobs
                            .iter()
                            .filter(|other| other.id != id)
                            .map(|other| PathBuf::from(&other.output))
                            .collect()
                    };
                    let target = unique_output(&dir, file_stem(&job.output).as_str(), &ext, &used);
                    match rename_atomic(&temp, &target) {
                        Ok(size) => {
                            let _ = set_job(&ctx, &id, |job| {
                                job.output = target.to_string_lossy().to_string();
                            })
                            .await;
                            size
                        }
                        Err(err) => {
                            remove_temp_file(&temp).await;
                            set_job(&ctx, &id, |job| {
                                job.status = "failed".into();
                                job.error_kind = Some("output".into());
                                job.message = Some(format!("写出最终文件失败：{err}"));
                                job.finished_at = Some(now_ms());
                            })
                            .await;
                            finish_slot(&ctx, &id).await;
                            return;
                        }
                    }
                }
            };

            set_job(&ctx, &id, |job| {
                job.status = "done".into();
                job.progress = 1.0;
                job.output_size = Some(size);
                job.encoder_used = used_encoder.clone();
                job.finished_at = Some(now_ms());
                job.speed = None;
                job.eta_sec = None;
                job.message = None;
                if !attempts_info.is_empty() {
                    job.fallback_note = Some(attempts_info.join("；"));
                }
            })
            .await;
            write_job_log(&ctx, &id, "done", &attempts_info, &last_stderr).await;
            finish_slot(&ctx, &id).await;
            return;
        }

        let (kind, message) = classify(&outcome.stderr);
        last_kind = kind.clone();
        last_message = message.clone();
        remove_temp_file(&temp).await;

        let hardware = is_hardware_encoder(encoder);
        let fallbackable = (kind == "hw-unavailable"
            || kind == "hw-memory"
            || (hardware && kind == "invalid-params"))
            && idx + 1 < chain.len();
        if fallbackable {
            attempts_info.push(format!(
                "{label} 不可用（{}），已自动回退到 {}",
                if message.chars().count() > 90 {
                    message.chars().take(90).collect::<String>() + "…"
                } else {
                    message.clone()
                },
                chain[idx + 1]
            ));
            let _ = set_job(&ctx, &id, |job| {
                job.progress = 0.0;
                job.fallback_note = Some(attempts_info.join("；"));
            })
            .await;
            continue;
        }
        break;
    }

    let cancelled_after = {
        let inner = state.inner.lock().await;
        inner.cancel_requested.contains(&id)
    };
    if cancelled_after {
        remove_temp_file(&temp).await;
        set_job(&ctx, &id, |job| {
            job.status = "cancelled".into();
            job.finished_at = Some(now_ms());
            job.progress = 0.0;
            job.message = Some("已取消".into());
        })
        .await;
        write_job_log(&ctx, &id, "cancelled", &attempts_info, &last_stderr).await;
        finish_slot(&ctx, &id).await;
        return;
    }

    let final_status = {
        let inner = state.inner.lock().await;
        inner
            .jobs
            .iter()
            .find(|j| j.id == id)
            .map(|j| j.status.clone())
    };
    if final_status.as_deref() == Some("done") || final_status.as_deref() == Some("cancelled") {
        return;
    }

    let guidance = match last_kind.as_str() {
        "hw-unavailable" => "硬件编码器不可用（驱动/设备/参数不被接受）。已尝试回退软件编码；若仍失败，请在设置页重新探测或改用软件编码。",
        "hw-memory" => "显存/内存不足，建议降低分辨率或减少同时进行的硬编任务数。",
        "invalid-params" => "编码器拒绝了当前参数组合，建议降低码率/画质档位或改用其它编码器。",
        "encoder-missing" => "本机没有可用的该编码器，请更换编码器或安装包含对应组件的 FFmpeg。",
        "input" => "源文件无法解码（可能损坏或格式不受支持）。",
        "output" => "写入输出失败（权限或磁盘空间）。",
        "io-temporary" => "文件被占用或磁盘暂时不可用；已自动重试一次，若仍失败请关闭占用程序后手动重试。",
        "external-tool" => "外部工具执行失败或缺失，请检查设置中的工具目录。",
        _ => "转换失败，请查看诊断日志。",
    };
    let _ = set_job(&ctx, &id, |job| {
        job.status = "failed".into();
        job.error_kind = Some(last_kind.clone());
        job.message = Some(format!("{last_message} —— {guidance}"));
        job.finished_at = Some(now_ms());
        job.encoder_used = used_encoder.clone();
        job.progress = 0.0;
        job.speed = None;
        job.eta_sec = None;
        if !attempts_info.is_empty() {
            job.fallback_note = Some(attempts_info.join("；"));
        }
    })
    .await;
    write_job_log(&ctx, &id, "failed", &attempts_info, &last_stderr).await;
    maybe_auto_retry(&ctx, &id, &last_kind).await;
    finish_slot(&ctx, &id).await;
}

pub async fn list(ctx: &Ctx) -> Vec<Job> {
    let state = &ctx.state;
    let inner = state.inner.lock().await;
    inner.jobs.clone()
}

pub async fn set_concurrency(ctx: &Ctx, value: usize) -> usize {
    let state = &ctx.state;
    let clamped = value.clamp(1, 8);
    {
        let mut inner = state.inner.lock().await;
        inner.concurrency = clamped;
    }
    state.notify.notify_one();
    clamped
}

/// 队列级暂停 / 继续。语义：暂停后不再调度新任务，正在运行的任务继续跑完；
/// 继续后从队列里接着跑。不尝试挂起外部 ffmpeg 进程（Windows 无 SIGSTOP，
/// NtSuspendProcess 会留下锁/句柄风险，收益不值得）。
pub async fn set_paused(ctx: &Ctx, paused: bool) -> bool {
    let state = &ctx.state;
    {
        let mut inner = state.inner.lock().await;
        inner.paused = paused;
    }
    state.notify.notify_one();
    paused
}

pub async fn is_paused(ctx: &Ctx) -> bool {
    ctx.state.inner.lock().await.paused
}

pub async fn set_after_action(ctx: &Ctx, action: &str) -> String {
    let normalized = match action.trim().to_ascii_lowercase().as_str() {
        "open-folder" => "open-folder",
        "shutdown" => "shutdown",
        _ => "none",
    };
    *ctx.state.after_action.lock().await = normalized.to_string();
    normalized.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn temp_dir(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("formatforge-test-{tag}-{}", now_ms()));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn unique_output_avoids_existing_file_and_reserved_names() {
        let dir = temp_dir("unique");
        std::fs::write(dir.join("clip.mp4"), b"existing").unwrap();
        let reserved = vec![dir.join("clip (1).mp4")];
        let target = unique_output(&dir, "clip", "mp4", &reserved);
        assert_eq!(target.file_name().unwrap(), "clip (2).mp4");

        let no_conflict = unique_output(&dir, "other", "mp4", &[]);
        assert_eq!(no_conflict.file_name().unwrap(), "other.mp4");

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn unique_output_numbering_without_existing_files() {
        let dir = temp_dir("reserve");
        let reserved = vec![dir.join("same.mp4"), dir.join("same (1).mp4")];
        let target = unique_output(&dir, "same", "mp4", &reserved);
        assert_eq!(target.file_name().unwrap(), "same (2).mp4");
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn unique_output_never_overwrites() {
        let dir = temp_dir("overwrite");
        let existing = dir.join("keep.mp4");
        std::fs::write(&existing, b"original").unwrap();
        let target = unique_output(&dir, "keep", "mp4", &[]);
        assert_ne!(target, existing);
        assert_eq!(std::fs::read(&existing).unwrap(), b"original");
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn rename_atomic_refuses_existing_target() {
        let dir = temp_dir("rename");
        let temp = dir.join(".part.mp4");
        let target = dir.join("final.mp4");
        std::fs::write(&temp, b"new-content").unwrap();
        std::fs::write(&target, b"old-content").unwrap();

        let result = rename_atomic(&temp, &target);
        assert!(result.is_err(), "目标已存在时必须拒绝覆盖");
        assert_eq!(std::fs::read(&target).unwrap(), b"old-content");
        assert!(temp.exists(), "失败时临时文件应保留以便上层改名重试");

        let fresh = dir.join("fresh.mp4");
        assert!(rename_atomic(&temp, &fresh).is_ok());
        assert_eq!(std::fs::read(&fresh).unwrap(), b"new-content");
        assert!(!temp.exists());

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn render_name_supports_documented_placeholders() {
        // 默认模板：保持源文件名。
        assert_eq!(render_name("", "clip", "20261105", 1, "mp4"), "clip");
        // 四个占位符 + 扩展名
        assert_eq!(
            render_name("{name}-{date}-{index}.{ext}", "clip", "20261105", 3, "gif"),
            "clip-20261105-3.gif"
        );
        // 未识别的占位符原样保留，不会被悄悄吃掉
        assert_eq!(
            render_name("{name}-{unknown}", "clip", "20261105", 1, "mp4"),
            "clip-{unknown}"
        );
        // 路径分隔符与 Windows 保留字符必须被替换，避免模板越权写目录
        assert_eq!(
            render_name("{name}/../x:{index}", "clip", "20261105", 2, "mp4"),
            "clip_.._x_2"
        );
        // 渲染结果为空时回退到源文件名，绝不产生空文件名
        assert_eq!(render_name("   ", "clip", "20261105", 1, "mp4"), "clip");
    }

    #[test]
    fn auto_retry_only_covers_retryable_categories() {
        assert!(is_retryable_kind("hw-unavailable"));
        assert!(is_retryable_kind("io-temporary"));
        for kind in [
            "invalid-params",
            "input",
            "encoder-missing",
            "output",
            "hw-memory",
            "unknown",
        ] {
            assert!(!is_retryable_kind(kind), "{kind} 不应自动重试");
        }
    }

    #[test]
    fn conflict_policy_defaults_to_safe_suffix() {
        assert_eq!(conflict_policy(None), "suffix");
        assert_eq!(conflict_policy(Some("")), "suffix");
        assert_eq!(conflict_policy(Some("unknown")), "suffix");
        assert_eq!(conflict_policy(Some("SKIP")), "skip");
        assert_eq!(conflict_policy(Some(" overwrite ")), "overwrite");
    }

    #[test]
    fn output_extension_follows_container_and_audio_format() {
        assert_eq!(kind_ext("video-mp4", &json!({ "container": "mkv" })), "mkv");
        assert_eq!(
            kind_ext("video-mp4", &json!({ "container": "webm" })),
            "webm"
        );
        assert_eq!(kind_ext("video-mp4", &json!({})), "mp4");
        assert_eq!(kind_ext("audio-aac", &json!({ "format": "flac" })), "flac");
        assert_eq!(kind_ext("audio-aac", &json!({ "format": "aac" })), "m4a");
        assert_eq!(kind_ext("audio-mp3", &json!({ "format": "opus" })), "opus");
    }

    #[test]
    fn precheck_rejects_incompatible_container_and_format() {
        let err = precheck_spec(
            "video-mp4",
            &[],
            &json!({ "container": "mp4", "codec": "vp9" }),
            None,
        )
        .unwrap_err();
        assert!(
            err.contains("MP4") && err.contains("vp9"),
            "应拒绝 mp4+vp9：{err}"
        );
        let err = precheck_spec("audio-mp3", &[], &json!({ "format": "amr" }), None).unwrap_err();
        assert!(err.contains("amr"), "应拒绝未知音频格式：{err}");
        assert!(precheck_spec("audio-mp3", &[], &json!({ "format": "opus" }), None).is_ok());
    }

    #[test]
    fn skip_policy_targets_existing_file_without_creating_anything() {
        let dir = temp_dir("skip-policy");
        let existing = dir.join("clip.mp4");
        std::fs::write(&existing, b"original").unwrap();
        // 模拟 enqueue 的 skip 分支：目标已存在则不改名、不写文件。
        let target = dir.join(format!(
            "{}.mp4",
            render_name("", "clip", "20261105", 1, "mp4")
        ));
        assert!(target.exists());
        assert_eq!(std::fs::read(&existing).unwrap(), b"original");
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn overwrite_policy_replaces_only_when_explicitly_requested() {
        let dir = temp_dir("overwrite-policy");
        let temp = dir.join(".part.mp4");
        let target = dir.join("clip.mp4");
        std::fs::write(&temp, b"new-content").unwrap();
        std::fs::write(&target, b"old-content").unwrap();

        // 默认 suffix 路径（rename_atomic）绝不覆盖
        assert!(rename_atomic(&temp, &target).is_err());
        assert_eq!(std::fs::read(&target).unwrap(), b"old-content");

        // 显式 overwrite 才替换，且旧文件备份在成功后不残留
        let size = rename_overwrite(&temp, &target).expect("显式覆盖应成功");
        assert_eq!(size, 11);
        assert_eq!(std::fs::read(&target).unwrap(), b"new-content");
        assert!(!temp.exists());
        let leftover: Vec<_> = std::fs::read_dir(&dir)
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_name().to_string_lossy().contains("ffbak"))
            .collect();
        assert!(leftover.is_empty(), "覆盖成功后不应留下备份文件");

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn utc_date_is_compact_and_sortable() {
        let date = utc_date();
        assert_eq!(date.len(), 8, "{date}");
        assert!(date.chars().all(|c| c.is_ascii_digit()), "{date}");
    }

    // ===== 第九轮：音量 / 分割 / 图片 GIF =====

    #[test]
    fn precheck_rejects_volume_gain_with_audio_copy() {
        let params = json!({
            "audioMode": "copy",
            "volume": { "enabled": true, "gainDb": 6.0, "mute": false }
        });
        let err = precheck_spec("video-mp4", &["in.mp4".into()], &params, None).unwrap_err();
        assert!(err.contains("原样复制"), "{err}");
        assert!(err.contains("无法应用滤镜"), "{err}");

        // 关闭增益后不再因 copy 被拒（容器为空是下一个校验点，说明已越过音量检查）。
        let params = json!({
            "audioMode": "copy",
            "container": "mp4",
            "codec": "h264",
            "volume": { "enabled": false, "gainDb": 6.0, "mute": false }
        });
        assert!(precheck_spec("video-mp4", &["in.mp4".into()], &params, None).is_ok());
    }

    #[test]
    fn precheck_audio_rejects_normalize_with_volume() {
        let params = json!({
            "format": "mp3",
            "normalize": true,
            "volume": { "enabled": true, "gainDb": 6.0, "mute": false }
        });
        let err = precheck_spec("audio-mp3", &["in.wav".into()], &params, None).unwrap_err();
        assert!(err.contains("-16 LUFS"), "{err}");
        assert!(err.contains("请二选一"), "{err}");
    }

    #[test]
    fn precheck_split_validates_mode_range_and_container() {
        let mut params = json!({
            "split": {
                "mode": "copy",
                "container": "mp4",
                "segmentIndex": 0,
                "startSec": 0.0,
                "endSec": 30.0
            }
        });
        assert!(precheck_spec("video-split", &["in.mp4".into()], &params, None).is_ok());

        params["split"]["mode"] = json!("bad");
        let err = precheck_spec("video-split", &["in.mp4".into()], &params, None).unwrap_err();
        assert!(err.contains("分割模式"), "{err}");

        params["split"]["mode"] = json!("copy");
        params["split"]["endSec"] = json!(0.0);
        let err = precheck_spec("video-split", &["in.mp4".into()], &params, None).unwrap_err();
        assert!(err.contains("大于开始时间"), "{err}");

        params["split"]["endSec"] = json!(30.0);
        params["split"]["container"] = json!("flv");
        let err = precheck_spec("video-split", &["in.mp4".into()], &params, None).unwrap_err();
        assert!(err.contains("分割输出容器"), "{err}");
    }

    #[test]
    fn precheck_image_gif_requires_two_images_and_valid_fps() {
        let params = json!({ "fps": 10, "loopCount": 0, "maxWidth": 0, "dither": true });
        let err = precheck_spec("image-gif", &["a.png".into()], &params, None).unwrap_err();
        assert!(err.contains("至少需要 2 张"), "{err}");

        let inputs = vec!["a.png".to_string(), "b.png".to_string()];
        let bad = json!({ "fps": 0, "loopCount": 0, "maxWidth": 0, "dither": true });
        let err = precheck_spec("image-gif", &inputs, &bad, None).unwrap_err();
        assert!(err.contains("1–30"), "{err}");

        let high = json!({ "fps": 31, "loopCount": 0, "maxWidth": 0, "dither": true });
        let err = precheck_spec("image-gif", &inputs, &high, None).unwrap_err();
        assert!(err.contains("1–30"), "{err}");

        assert!(precheck_spec("image-gif", &inputs, &params, None).is_ok());
    }

    #[test]
    fn kind_ext_uses_split_container_and_gif_ext() {
        let split =
            json!({ "split": { "container": "mkv", "mode": "copy", "startSec": 0, "endSec": 3 } });
        assert_eq!(kind_ext("video-split", &split), "mkv");
        let split =
            json!({ "split": { "container": "webm", "mode": "copy", "startSec": 0, "endSec": 3 } });
        assert_eq!(kind_ext("video-split", &split), "webm");
        assert_eq!(kind_ext("image-gif", &json!({})), "gif");
    }

    #[test]
    fn split_seconds_are_short_and_readable() {
        assert_eq!(fmt_sec_short(0.0), "0");
        assert_eq!(fmt_sec_short(2.5), "2.5");
        assert_eq!(fmt_sec_short(10.05), "10.05");
    }
}
