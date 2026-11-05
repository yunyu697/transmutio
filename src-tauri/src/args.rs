use serde::Deserialize;

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct VideoParams {
    /// mp4 | mkv | webm | mov | ts | avi
    pub container: String,
    /// h264 | hevc | av1 | vp9 | vp8 | mpeg4（需与容器兼容）
    pub codec: String,
    /// auto | software | <encoder id>
    pub encoder: String,
    /// quality | bitrate
    pub rate_mode: String,
    /// 18..30（越小越清晰）
    pub quality: u32,
    pub bitrate_kbps: u32,
    /// source | 2160 | 1440 | 1080 | 720 | 480
    pub resolution: String,
    /// source | 30 | 60
    pub fps: String,
    /// source | 16:9 | 1:1 | 9:16（居中裁剪）
    pub crop: String,
    /// 0 | 90 | 180 | 270
    pub rotation: String,
    /// aac | copy | none
    pub audio_mode: String,
    pub audio_bitrate_kbps: u32,
    pub hw_decode: bool,
    pub faststart: bool,
    /// 播放倍速：1.0 表示不变；有效范围 0.25–4.0
    pub speed: f64,
    /// 变速时保持音高（默认 true：用 atempo；false 时音高随速度变化）
    #[serde(default = "default_true")]
    pub keep_pitch: bool,
    /// 水印叠加（默认关闭）
    #[serde(default)]
    pub watermark: WatermarkParams,
    /// 字幕：内嵌（soft）或烧录（hard），默认关闭
    #[serde(default)]
    pub subtitle: SubtitleParams,
    /// 相对音量增益（与音频卡的 loudnorm 语义不同）
    #[serde(default)]
    pub volume: VolumeParams,
    /// 按时间点切分（仅 video-split 任务读取）
    #[serde(default)]
    pub split: SplitParams,
}

/// 字幕参数。`baseHeight` 由后端探测后回填（`#[serde(skip)]`），
/// 用于把「画面高度 × 字号百分比」换算成烧录滤镜需要的像素字号。
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct SubtitleParams {
    pub enabled: bool,
    /// soft | hard；空字符串等价 soft
    pub mode: String,
    /// 主字幕文件（srt / ass / ssa / vtt）
    pub path: String,
    /// 仅 soft：附加字幕轨（多文件合并进同一任务）
    pub extra_paths: Vec<String>,
    /// 默认 "chi"
    pub language: String,
    /// 主字幕标 disposition:default
    pub default_track: bool,
    /// hard：字号 = 输出画面高 × 百分比，合法 1.0–20.0，默认 4.5
    pub font_size_pct: f64,
    /// hard：描边 0–6，默认 2
    pub outline: u32,
    /// hard：bottom | middle | top；空字符串等价 bottom
    pub position: String,
    /// hard：#RRGGBB，默认 #FFFFFF
    pub color: String,
    /// hard：垂直边距，默认 24
    pub margin_v: u32,
    /// 输出画面高度，由后端探测回填
    #[serde(skip)]
    pub base_height: u32,
}

impl Default for SubtitleParams {
    fn default() -> Self {
        Self {
            enabled: false,
            mode: String::new(),
            path: String::new(),
            extra_paths: Vec::new(),
            language: "chi".into(),
            default_track: false,
            font_size_pct: 4.5,
            outline: 2,
            position: String::new(),
            color: "#FFFFFF".into(),
            margin_v: 24,
            base_height: 0,
        }
    }
}

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct AudioParams {
    /// mp3 | aac
    pub format: String,
    pub bitrate_kbps: u32,
    /// source | 44100 | 48000
    pub sample_rate: String,
    /// source | mono | stereo
    pub channels: String,
    /// EBU R128 响度归一化（-16 LUFS）
    pub normalize: bool,
    /// 播放倍速：1.0 表示不变；有效范围 0.25–4.0
    pub speed: f64,
    /// 变速时保持音高（默认 true：用 atempo；false 时音高随速度变化）
    #[serde(default = "default_true")]
    pub keep_pitch: bool,
    /// 相对音量增益（与「响度归一化」互斥，见 validate_audio_volume）
    #[serde(default)]
    pub volume: VolumeParams,
}

/// 相对音量增益。只把整条音轨统一放大/衰减固定 dB，不改变动态范围；
/// 与音频卡「响度归一化」（loudnorm，把整体拉回 -16 LUFS）语义不同。
#[derive(Debug, Clone, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct VolumeParams {
    pub enabled: bool,
    /// 相对增益 dB，合法范围 -20..=20
    pub gain_db: f64,
    /// 静音：直接丢弃音轨，优先于增益
    pub mute: bool,
}

/// 音量增益校验：静音时忽略增益；否则必须有限且在 -20..=20。
pub fn validate_volume(v: &VolumeParams) -> Result<(), String> {
    if v.mute {
        return Ok(());
    }
    if !v.gain_db.is_finite() {
        return Err("音量增益必须是有限数值（-20 至 20 dB）".into());
    }
    if !(-20.0..=20.0).contains(&v.gain_db) {
        return Err(format!(
            "音量增益 {} dB 超出支持范围：-20 至 20 dB",
            fmt_num(v.gain_db)
        ));
    }
    Ok(())
}

/// 按时间点切分参数。一个任务只切一段，前端按段生成多个 JobSpec。
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct SplitParams {
    /// copy（流复制，只能切在关键帧） | encode（重编码，帧级准确）
    pub mode: String,
    /// mp4 | mkv | webm | mov | ts | avi
    pub container: String,
    /// 段序号（0 起），仅用于文件名 -partN
    pub segment_index: u32,
    pub start_sec: f64,
    pub end_sec: f64,
}

impl Default for SplitParams {
    fn default() -> Self {
        Self {
            mode: "copy".into(),
            container: "mp4".into(),
            segment_index: 0,
            start_sec: 0.0,
            end_sec: 0.0,
        }
    }
}

fn default_true() -> bool {
    true
}

/// 图片「压缩到目标大小」参数。`mode=target` 时按 `targetMode` 给出目标字节数，
/// `mode=quality` 时按固定质量重编码；两种模式互斥。
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct CompressParams {
    /// target | quality；空字符串等价 target
    pub mode: String,
    /// absolute | percent，默认 absolute
    pub target_mode: String,
    /// 目标绝对大小 KB（target + absolute 用）
    pub target_kb: u32,
    /// 相对原图百分比 1–99（target + percent 用）
    pub target_percent: f64,
    /// 固定质量 1–100（quality 模式用），默认 80
    pub quality: u32,
    /// 默认 false：剥离 EXIF/ICC；true 时保留原始元数据
    pub keep_metadata: bool,
    /// PNG 调色板量化（8-bit），默认 true
    pub png_palette: bool,
    /// WebP 无损模式；仅固定质量模式可用，target 模式直接拒绝
    pub lossless: bool,
}

impl Default for CompressParams {
    fn default() -> Self {
        Self {
            mode: "target".into(),
            target_mode: "absolute".into(),
            target_kb: 500,
            target_percent: 60.0,
            quality: 80,
            keep_metadata: false,
            png_palette: true,
            lossless: false,
        }
    }
}

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct ImageParams {
    /// jpg | png | webp | bmp | tiff
    pub format: String,
    /// 1..100（jpg/webp 有意义）
    pub quality: u32,
    /// 0 表示保持原尺寸
    pub max_edge: u32,
    /// 0 | 90 | 180 | 270（叠加在 EXIF 自动校正之后）
    pub rotation: String,
    /// 读取 EXIF Orientation 并自动校正；关闭时保留原始像素方向
    pub auto_orient: bool,
    /// 裁剪模式：none | ratio | custom
    pub crop_mode: String,
    /// ratio 模式下的画面比例：1:1 | 16:9 | 9:16 | 4:3
    pub crop_ratio: String,
    /// custom 模式的矩形（像素），原点为左上角
    pub crop_x: i64,
    pub crop_y: i64,
    pub crop_w: u32,
    pub crop_h: u32,
    /// 水印叠加（默认关闭）
    #[serde(default)]
    pub watermark: WatermarkParams,
    /// 压缩参数（仅 image-compress 任务读取；普通图片任务忽略）
    #[serde(default)]
    pub compress: CompressParams,
}

/// 水印参数。`baseWidth/baseHeight/wmWidth/wmHeight` 由后端探测后回填，
/// 前端不传（`#[serde(skip)]`），用于把「百分比缩放 + 九宫格定位」换算成像素坐标。
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct WatermarkParams {
    pub enabled: bool,
    /// 水印图片路径（png / jpg / jpeg / webp）
    pub path: String,
    /// nw | n | ne | w | c | e | sw | s | se | custom；空字符串等价 se
    pub position: String,
    /// custom 模式的左上角坐标（像素）
    pub x: i64,
    pub y: i64,
    /// 0..=1
    pub opacity: f64,
    /// 相对基准画面短边的百分比，默认 15，范围 5–50
    pub scale_percent: u32,
    /// 距画面边缘的留白（像素）
    pub margin: u32,
    /// -180..=180（度）
    pub rotation: f64,
    /// 基准画面尺寸（输出帧），由后端回填
    #[serde(skip)]
    pub base_width: u32,
    #[serde(skip)]
    pub base_height: u32,
    /// 水印原始尺寸，由后端回填
    #[serde(skip)]
    pub wm_width: u32,
    #[serde(skip)]
    pub wm_height: u32,
}

impl Default for WatermarkParams {
    fn default() -> Self {
        Self {
            enabled: false,
            path: String::new(),
            position: "se".into(),
            x: 0,
            y: 0,
            opacity: 0.9,
            scale_percent: 15,
            margin: 24,
            rotation: 0.0,
            base_width: 0,
            base_height: 0,
            wm_width: 0,
            wm_height: 0,
        }
    }
}

/// 图片序列 → 视频。`size_mode`：source（要求全部同尺寸）| first（按首帧）|
/// max（取最大宽高）| fixed（自定义 width/height）。
#[derive(Debug, Clone, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct SeqParams {
    pub fps: u32,
    pub start_number: u32,
    pub size_mode: String,
    pub width: u32,
    pub height: u32,
    pub encoder: String,
    pub quality: u32,
    pub bitrate_kbps: u32,
}

/// 视频 → 图片序列。
#[derive(Debug, Clone, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct FramesParams {
    /// 每秒抽帧数
    pub fps: u32,
    /// 0 表示保持源宽度
    pub width: u32,
    /// png | jpg
    pub format: String,
    /// jpg 质量 1..100
    pub quality: u32,
}

/// 拼图：网格布局、间距、底色、单元尺寸。
#[derive(Debug, Clone, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct CollageParams {
    pub cols: u32,
    /// 0 表示按输入数量自动计算
    pub rows: u32,
    pub cell_width: u32,
    pub cell_height: u32,
    pub gap: u32,
    /// #RRGGBB
    pub background: String,
    /// contain | cover
    pub fit: String,
    pub format: String,
    pub quality: u32,
}

/// 视频合并：`auto` = 同参走 stream copy，不同参先统一转码再拼接；
/// `copy` = 强制 stream copy（不同参直接拒绝）；`transcode` = 一律先统一转码。
#[derive(Debug, Clone, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct MergeParams {
    /// auto | copy | transcode
    pub mode: String,
    /// 统一转码时的目标高度；source 表示按第一条输入
    pub resolution: String,
    /// source | 30 | 60
    pub fps: String,
    /// aac | none（统一转码时的音轨处理；copy 模式沿用源音轨）
    pub audio_mode: String,
    pub codec: String,
    pub quality: u32,
    pub bitrate_kbps: u32,
}

/// 音视频混流 / 替换音轨：视频流永远 copy（不重编码），仅处理音频。
#[derive(Debug, Clone, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct MuxParams {
    /// replace（替换音轨） | add（保留原音轨再追加一条）
    pub mode: String,
    /// aac | copy
    pub audio_mode: String,
    pub audio_bitrate_kbps: u32,
}

/// 视频转 GIF。`optimize` 走 palettegen/paletteuse 两阶段（单次进程内完成），
/// 否则直接按 fps/宽度转码（调色板量化由 GIF 编码器隐式做，色彩明显更差）。
#[derive(Debug, Clone, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct GifParams {
    pub fps: u32,
    /// 0 表示保持源宽度；宽高都按比例缩放
    pub width: u32,
    pub start_sec: f64,
    /// 0 表示到片尾
    pub duration_sec: f64,
    pub optimize: bool,
}

/// 多张图片合成 GIF（palettegen + paletteuse 两遍）。
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ImageGifParams {
    /// 1..=30
    pub fps: u32,
    /// 0 = 无限循环
    pub loop_count: u32,
    /// 0 = 保持首图宽度；前端 >2000 时提示并自动填 1280（后端不改参数）
    pub max_width: u32,
    pub dither: bool,
}

impl Default for ImageGifParams {
    fn default() -> Self {
        Self {
            fps: 10,
            loop_count: 0,
            max_width: 0,
            dither: true,
        }
    }
}

pub fn image_ext(format: &str) -> &'static str {
    match format {
        "png" => "png",
        "webp" => "webp",
        "bmp" => "bmp",
        "tiff" => "tiff",
        _ => "jpg",
    }
}

fn quality_bitrate_hint(height: u32) -> u32 {
    match height {
        0..=480 => 900,
        481..=720 => 1800,
        721..=1080 => 4200,
        1081..=1440 => 8000,
        1441..=2160 => 16000,
        _ => 22000,
    }
}

fn vt_quality(q: u32) -> u32 {
    // VideoToolbox 只暴露 q:v（0..100，越大越好），与 x264 CRF 语义不同，这里是经验映射，待真机校准。
    let q = q.clamp(16, 32);
    (100i64 - (q as i64 - 14) * 4).clamp(30, 90) as u32
}

/// 不同编码器的码控参数差异很大，必须按厂商分别映射，不能统一塞 -crf。
fn rate_args(encoder: &str, p: &VideoParams) -> Vec<String> {
    let quality = p.quality.clamp(14, 40);
    let kbps = if p.bitrate_kbps > 0 {
        p.bitrate_kbps
    } else {
        4200
    };
    let bitrate_mode = p.rate_mode == "bitrate";
    let target_h = p.resolution.parse::<u32>().unwrap_or(1080);
    let hint = quality_bitrate_hint(target_h);

    if encoder.ends_with("_nvenc") {
        return if bitrate_mode {
            vec![
                "-rc".into(),
                "vbr".into(),
                "-b:v".into(),
                format!("{kbps}k"),
                "-maxrate".into(),
                format!("{}k", kbps * 3 / 2),
                "-bufsize".into(),
                format!("{}k", kbps * 2),
            ]
        } else {
            vec![
                "-rc".into(),
                "vbr".into(),
                "-cq".into(),
                quality.to_string(),
                "-b:v".into(),
                "0".into(),
                "-preset".into(),
                "p5".into(),
                "-tune".into(),
                "hq".into(),
                "-multipass".into(),
                "qres".into(),
            ]
        };
    }
    if encoder.ends_with("_amf") {
        return if bitrate_mode {
            vec![
                "-rc".into(),
                "vbr_peak".into(),
                "-b:v".into(),
                format!("{kbps}k"),
                "-quality".into(),
                "quality".into(),
            ]
        } else {
            vec![
                "-rc".into(),
                "cqp".into(),
                "-qp_i".into(),
                quality.to_string(),
                "-qp_p".into(),
                quality.to_string(),
                "-quality".into(),
                "quality".into(),
            ]
        };
    }
    if encoder.ends_with("_qsv") {
        return if bitrate_mode {
            vec!["-b:v".into(), format!("{kbps}k")]
        } else {
            vec![
                "-global_quality".into(),
                quality.to_string(),
                "-preset".into(),
                "medium".into(),
            ]
        };
    }
    if encoder.ends_with("_vaapi") {
        return if bitrate_mode {
            vec!["-b:v".into(), format!("{kbps}k")]
        } else {
            vec![
                "-rc_mode".into(),
                "CQP".into(),
                "-qp".into(),
                quality.to_string(),
            ]
        };
    }
    if encoder.ends_with("_videotoolbox") {
        return if bitrate_mode {
            vec![
                "-b:v".into(),
                format!("{kbps}k"),
                "-allow_sw".into(),
                "0".into(),
            ]
        } else {
            vec![
                "-q:v".into(),
                vt_quality(quality).to_string(),
                "-allow_sw".into(),
                "0".into(),
            ]
        };
    }
    if encoder == "libx264" || encoder == "libx264rgb" || encoder == "libx265" {
        return if bitrate_mode {
            vec![
                "-b:v".into(),
                format!("{kbps}k"),
                "-preset".into(),
                "medium".into(),
            ]
        } else {
            vec![
                "-crf".into(),
                quality.to_string(),
                "-preset".into(),
                "medium".into(),
            ]
        };
    }
    if encoder == "libopenh264" {
        let rate = if bitrate_mode { kbps } else { hint };
        return vec!["-b:v".into(), format!("{rate}k"), "-g".into(), "120".into()];
    }
    if encoder.ends_with("_d3d12va") || encoder.ends_with("_mf") {
        let rate = if bitrate_mode { kbps } else { hint };
        return vec!["-b:v".into(), format!("{rate}k")];
    }
    if bitrate_mode {
        vec!["-b:v".into(), format!("{kbps}k")]
    } else {
        vec!["-b:v".into(), format!("{hint}k")]
    }
}

/// 居中裁剪到指定画面比例。用单引号保护表达式内的逗号，避免被滤镜分隔符切断。
/// 取偶数是 yuv420p 的硬要求（奇数宽高会让编码器报错）。
fn crop_filter(mode: &str) -> Option<String> {
    let (w, h) = match mode {
        "16:9" => ("trunc(min(iw,ih*16/9)/2)*2", "trunc(min(ih,iw*9/16)/2)*2"),
        "1:1" => ("trunc(min(iw,ih)/2)*2", "trunc(min(iw,ih)/2)*2"),
        "9:16" => ("trunc(min(iw,ih*9/16)/2)*2", "trunc(min(ih,iw*16/9)/2)*2"),
        "4:3" => ("trunc(min(iw,ih*4/3)/2)*2", "trunc(min(ih,iw*3/4)/2)*2"),
        _ => return None,
    };
    Some(format!("crop=w='{w}':h='{h}'"))
}

/// 图片裁剪参数的结构校验（不依赖源图尺寸）。非法值必须明确报错。
pub fn validate_image_crop(p: &ImageParams) -> Result<(), String> {
    match p.crop_mode.as_str() {
        "" | "none" => Ok(()),
        "ratio" => match p.crop_ratio.as_str() {
            "1:1" | "16:9" | "9:16" | "4:3" => Ok(()),
            other => Err(format!(
                "不支持的裁剪比例「{other}」，可选 1:1 / 16:9 / 9:16 / 4:3"
            )),
        },
        "custom" => {
            if p.crop_w == 0 || p.crop_h == 0 {
                return Err("自定义裁剪的宽高必须大于 0".into());
            }
            if p.crop_x < 0 || p.crop_y < 0 {
                return Err("自定义裁剪的起点不能为负数".into());
            }
            Ok(())
        }
        other => Err(format!(
            "不支持的裁剪模式「{other}」，可选 none / ratio / custom"
        )),
    }
}

/// 图片压缩参数校验。BMP/TIFF 没有可调的压缩编码参数，直接拒绝而不是
/// 「跑一遍再发现文件没变小」。`lossless` 与 target 二分互斥（无损无法靠质量档位收字节）。
pub fn validate_compress(p: &ImageParams) -> Result<(), String> {
    let format = crate::compress::format_key(&p.format);
    if format == "bmp" || format == "tiff" {
        return Err(format!(
            "{} 压缩不适用：BMP/TIFF 没有可调的有损压缩参数，请改用 JPEG / WebP / PNG",
            format.to_ascii_uppercase()
        ));
    }
    let c = &p.compress;
    let mode = if c.mode.trim().is_empty() {
        "target"
    } else {
        c.mode.as_str()
    };
    if !matches!(mode, "target" | "quality") {
        return Err(format!(
            "不支持的压缩模式「{}」，可选 target / quality",
            c.mode
        ));
    }
    if mode == "target" {
        let target_mode = if c.target_mode.trim().is_empty() {
            "absolute"
        } else {
            c.target_mode.as_str()
        };
        if !matches!(target_mode, "absolute" | "percent") {
            return Err(format!(
                "不支持的目标大小模式「{}」，可选 absolute / percent",
                c.target_mode
            ));
        }
        if target_mode == "absolute" && c.target_kb == 0 {
            return Err("目标大小必须大于 0 KB".into());
        }
        if target_mode == "percent" && !(1.0..=99.0).contains(&c.target_percent) {
            return Err(format!(
                "目标百分比必须在 1–99 之间，当前 {}",
                c.target_percent
            ));
        }
        if c.lossless {
            return Err("无损 WebP 无法按目标大小二分压缩：请改用固定质量模式，或关闭无损".into());
        }
    } else if !(1..=100).contains(&c.quality) {
        return Err(format!("固定质量必须在 1–100 之间，当前 {}", c.quality));
    }
    Ok(())
}

/// 自定义矩形是否落在源图范围内。越界直接拒绝，避免 ffmpeg 输出黑边或报错。
pub fn validate_custom_crop(
    x: i64,
    y: i64,
    w: u32,
    h: u32,
    source_w: u32,
    source_h: u32,
) -> Result<(), String> {
    if source_w == 0 || source_h == 0 {
        return Err("无法读取源图尺寸，不能使用自定义裁剪".into());
    }
    if w == 0 || h == 0 {
        return Err("自定义裁剪的宽高必须大于 0".into());
    }
    if x < 0 || y < 0 {
        return Err("自定义裁剪的起点不能为负数".into());
    }
    if x as u64 + w as u64 > source_w as u64 || y as u64 + h as u64 > source_h as u64 {
        return Err(format!(
            "裁剪矩形越界：{w}x{h} @ ({x},{y}) 超出源图 {source_w}x{source_h}"
        ));
    }
    Ok(())
}

fn image_crop_filter(p: &ImageParams) -> Option<String> {
    match p.crop_mode.as_str() {
        "ratio" => crop_filter(&p.crop_ratio),
        "custom" => Some(format!(
            "crop={}:{}:{}:{}",
            p.crop_w,
            p.crop_h,
            p.crop_x.max(0),
            p.crop_y.max(0)
        )),
        _ => None,
    }
}

/// 图片序列的编号连续性校验。返回 (ffmpeg 模式、起始编号、帧数)。
/// 用户选中的文件必须共享同前缀与数字位宽，且编号连续；缺号明确报出缺哪一张。
pub fn validate_sequence(inputs: &[String]) -> Result<(String, u32, u32), String> {
    if inputs.is_empty() {
        return Err("请至少选择 1 张图片".into());
    }
    let first = std::path::Path::new(&inputs[0]);
    let stem = first
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .ok_or_else(|| format!("无法解析文件名：{}", inputs[0]))?;
    let ext = first
        .extension()
        .map(|s| s.to_string_lossy().to_string())
        .ok_or_else(|| format!("无法解析扩展名：{}", inputs[0]))?;
    let digits: String = stem
        .chars()
        .rev()
        .take_while(|c| c.is_ascii_digit())
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect();
    if digits.is_empty() {
        return Err("文件名末尾必须带编号（例如 frame_0001.png），否则无法按序列读取".into());
    }
    let prefix = &stem[..stem.len() - digits.len()];
    let width = digits.len();
    let start: u32 = digits.parse().map_err(|_| "无法解析起始编号".to_string())?;

    let mut numbers: Vec<u32> = Vec::new();
    for input in inputs {
        let path = std::path::Path::new(input);
        let file_stem = path
            .file_stem()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_default();
        let file_ext = path
            .extension()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_default();
        if file_ext != ext {
            return Err(format!("序列内扩展名不一致：{input}"));
        }
        if file_stem.len() <= width
            || !file_stem.starts_with(prefix)
            || file_stem.len() != prefix.len() + width
            || !file_stem[prefix.len()..]
                .chars()
                .all(|c| c.is_ascii_digit())
        {
            return Err(format!("{input} 与序列前缀「{prefix}」不匹配"));
        }
        let tail = &file_stem[prefix.len()..];
        let number: u32 = tail.parse().map_err(|_| format!("无法解析编号：{input}"))?;
        numbers.push(number);
    }
    numbers.sort_unstable();
    if *numbers.first().unwrap() != start {
        return Err(format!(
            "起始编号不一致：首文件是 {start}，列表里最小是 {}",
            numbers[0]
        ));
    }
    for (index, number) in numbers.iter().enumerate() {
        let expected = start + index as u32;
        if *number != expected {
            return Err(format!(
                "序列编号不连续：缺少 {prefix}{:0width$}.{ext}",
                expected,
                width = width
            ));
        }
    }
    let dir = first
        .parent()
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_default();
    let separator = std::path::MAIN_SEPARATOR;
    let pattern = if dir.is_empty() {
        format!("{prefix}%0{width}d.{ext}")
    } else {
        format!("{dir}{separator}{prefix}%0{width}d.{ext}")
    };
    Ok((pattern, start, numbers.len() as u32))
}

/// 序列的统一尺寸策略。`dims` 是每张图的 (宽, 高)。
pub fn seq_target_size(p: &SeqParams, dims: &[(u32, u32)]) -> Result<Option<(u32, u32)>, String> {
    if dims.is_empty() {
        return Err("没有可用的图片尺寸".into());
    }
    match p.size_mode.as_str() {
        "" | "source" => {
            let first = dims[0];
            if let Some((index, other)) = dims.iter().enumerate().find(|(_, d)| **d != first) {
                return Err(format!(
                    "尺寸统一策略为「保持源尺寸」时要求所有图片同尺寸：第 1 张 {}x{}，第 {} 张 {}x{}",
                    first.0,
                    first.1,
                    index + 1,
                    other.0,
                    other.1
                ));
            }
            Ok(Some(first))
        }
        "first" => Ok(Some(dims[0])),
        "max" => Ok(Some((
            dims.iter().map(|d| d.0).max().unwrap_or(0),
            dims.iter().map(|d| d.1).max().unwrap_or(0),
        ))),
        "fixed" => {
            if p.width == 0 || p.height == 0 {
                return Err("自定义尺寸的宽高必须大于 0".into());
            }
            Ok(Some((p.width, p.height)))
        }
        other => Err(format!(
            "不支持的尺寸统一策略「{other}」，可选 source / first / max / fixed"
        )),
    }
}

/// 图片序列 → 视频。
pub fn build_seq_video(
    pattern: &str,
    output: &str,
    p: &SeqParams,
    target: Option<(u32, u32)>,
    frames: u32,
) -> Vec<String> {
    let fps = if p.fps > 0 { p.fps.clamp(1, 60) } else { 25 };
    let start = p.start_number.max(1);
    let mut args: Vec<String> = vec![
        "-hide_banner".into(),
        "-nostdin".into(),
        "-y".into(),
        "-loglevel".into(),
        "warning".into(),
        "-progress".into(),
        "pipe:1".into(),
        "-framerate".into(),
        fps.to_string(),
        "-start_number".into(),
        start.to_string(),
        "-i".into(),
        pattern.into(),
    ];
    let mut filters = Vec::new();
    if let Some((width, height)) = target.filter(|(w, h)| *w > 0 && *h > 0) {
        filters.push(format!(
            "scale={width}:{height}:force_original_aspect_ratio=decrease:flags=lanczos"
        ));
        filters.push(format!(
            "pad={width}:{height}:(ow-iw)/2:(oh-ih)/2:color=black"
        ));
    }
    filters.push("setsar=1".into());
    filters.push("format=yuv420p".into());
    args.push("-vf".into());
    args.push(filters.join(","));
    let encoder = if p.encoder.trim().is_empty() || p.encoder == "auto" {
        "libx264"
    } else {
        p.encoder.as_str()
    };
    args.push("-c:v".into());
    args.push(encoder.into());
    args.push("-pix_fmt".into());
    args.push("yuv420p".into());
    let mut rate = rate_args(
        encoder,
        &VideoParams {
            rate_mode: if p.bitrate_kbps > 0 {
                "bitrate".into()
            } else {
                "quality".into()
            },
            quality: if p.quality > 0 { p.quality } else { 23 },
            bitrate_kbps: p.bitrate_kbps,
            resolution: target
                .map(|(_, h)| h.to_string())
                .unwrap_or_else(|| "1080".into()),
            ..Default::default()
        },
    );
    args.append(&mut rate);
    if frames > 0 {
        args.push("-frames:v".into());
        args.push(frames.to_string());
    }
    args.push("-movflags".into());
    args.push("+faststart".into());
    args.push("-f".into());
    args.push("mp4".into());
    args.push(output.into());
    args
}

/// 视频 → 图片序列。`pattern` 形如 `dir/frame-%04d.png`。
pub fn build_video_frames(input: &str, pattern: &str, p: &FramesParams) -> Vec<String> {
    let fps = if p.fps > 0 { p.fps.clamp(1, 60) } else { 1 };
    let mut filters = vec![format!("fps={fps}")];
    if p.width > 0 {
        filters.push(format!("scale={}:-2:flags=lanczos", p.width));
    }
    let mut args: Vec<String> = vec![
        "-hide_banner".into(),
        "-nostdin".into(),
        "-y".into(),
        "-loglevel".into(),
        "warning".into(),
        "-progress".into(),
        "pipe:1".into(),
        "-i".into(),
        input.into(),
        "-vf".into(),
        filters.join(","),
        "-start_number".into(),
        "1".into(),
    ];
    if p.format == "jpg" {
        let quality = p.quality.clamp(1, 100);
        let q = 31 - ((quality as f64 / 100.0) * 29.0).round() as i32;
        args.push("-c:v".into());
        args.push("mjpeg".into());
        args.push("-q:v".into());
        args.push(q.clamp(2, 31).to_string());
    } else {
        args.push("-c:v".into());
        args.push("png".into());
    }
    args.push(pattern.into());
    args
}

fn normalize_color(value: &str) -> Result<String, String> {
    let raw = value.trim();
    let hex = raw.strip_prefix('#').unwrap_or(raw);
    if hex.len() != 6 || !hex.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err(format!("底色「{raw}」不是合法的 #RRGGBB 颜色"));
    }
    Ok(format!("0x{}", hex.to_uppercase()))
}

/// 拼图：先让每张图按 contain/cover 适配单元格，再按网格 xstack 拼接。
pub fn build_collage(
    inputs: &[String],
    output: &str,
    p: &CollageParams,
) -> Result<Vec<String>, String> {
    if inputs.is_empty() {
        return Err("拼图至少需要 1 张图片".into());
    }
    let cols = p.cols.clamp(1, 8);
    let rows = if p.rows == 0 {
        (inputs.len() as u32).div_ceil(cols)
    } else {
        p.rows.clamp(1, 8)
    };
    if inputs.len() as u32 > cols * rows {
        return Err(format!(
            "{} 张图片放不进 {}x{} 的网格（最多 {} 张）",
            inputs.len(),
            cols,
            rows,
            cols * rows
        ));
    }
    if p.cell_width == 0 || p.cell_height == 0 {
        return Err("拼图单元格的宽高必须大于 0".into());
    }
    let bg = normalize_color(if p.background.trim().is_empty() {
        "#FFFFFF"
    } else {
        &p.background
    })?;
    let gap = p.gap;
    let cell_w = p.cell_width;
    let cell_h = p.cell_height;

    let mut args: Vec<String> = vec!["-hide_banner".into(), "-nostdin".into(), "-y".into()];
    args.push("-loglevel".into());
    args.push("warning".into());
    args.push("-progress".into());
    args.push("pipe:1".into());
    for input in inputs {
        args.push("-i".into());
        args.push(input.clone());
    }

    let mut graph = String::new();
    for index in 0..inputs.len() {
        let fit = if p.fit == "cover" {
            format!(
                "scale={cell_w}:{cell_h}:force_original_aspect_ratio=increase:flags=lanczos,crop={cell_w}:{cell_h}"
            )
        } else {
            format!(
                "scale={cell_w}:{cell_h}:force_original_aspect_ratio=decrease:flags=lanczos,\
                 pad={cell_w}:{cell_h}:(ow-iw)/2:(oh-ih)/2:color={bg}"
            )
        };
        graph.push_str(&format!("[{index}:v]{fit},setsar=1[v{index}];"));
    }
    let labels: String = (0..inputs.len()).map(|i| format!("[v{i}]")).collect();
    if inputs.len() == 1 {
        graph.push_str(&format!("{labels}null[out]"));
    } else {
        let layout: Vec<String> = (0..inputs.len())
            .map(|index| {
                let col = index as u32 % cols;
                let row = index as u32 / cols;
                format!("{}_{}", col * (cell_w + gap), row * (cell_h + gap))
            })
            .collect();
        graph.push_str(&format!(
            "{labels}xstack=inputs={}:layout={}:fill={bg}[out]",
            inputs.len(),
            layout.join("|")
        ));
    }
    args.push("-filter_complex".into());
    args.push(graph);
    args.push("-map".into());
    args.push("[out]".into());
    args.push("-frames:v".into());
    args.push("1".into());
    let quality = p.quality.clamp(1, 100);
    match p.format.as_str() {
        "png" => {
            args.push("-c:v".into());
            args.push("png".into());
        }
        "webp" => {
            args.push("-c:v".into());
            args.push("libwebp".into());
            args.push("-quality".into());
            args.push(quality.to_string());
        }
        _ => {
            let q = 31 - ((quality as f64 / 100.0) * 29.0).round() as i32;
            args.push("-c:v".into());
            args.push("mjpeg".into());
            args.push("-q:v".into());
            args.push(q.clamp(2, 31).to_string());
        }
    }
    args.push(output.into());
    Ok(args)
}

/// 拼图输出画布尺寸，供 UI 预览与断言使用。
pub fn collage_canvas(cols: u32, rows: u32, cell_w: u32, cell_h: u32, gap: u32) -> (u32, u32) {
    (
        cols * cell_w + cols.saturating_sub(1) * gap,
        rows * cell_h + rows.saturating_sub(1) * gap,
    )
}

fn rotation_filters(mode: &str) -> Vec<String> {
    match mode {
        "90" => vec!["transpose=1".into()],
        "270" => vec!["transpose=2".into()],
        // transpose 的 180 选项会先水平翻转再旋转，直接用两次翻转更直观。
        "180" => vec!["hflip".into(), "vflip".into()],
        _ => Vec::new(),
    }
}

/// 把目标倍速拆成若干个 atempo 因子：单次 atempo 只接受 0.5–2.0，
/// 超出范围必须链式串接（例如 0.25 → 0.5×0.5；4.0 → 2.0×2.0；2.5 → 2.0×1.25）。
pub fn atempo_factors(speed: f64) -> Vec<f64> {
    let mut remaining = speed;
    let mut out: Vec<f64> = Vec::new();
    while remaining > 2.0 + 1e-9 {
        out.push(2.0);
        remaining /= 2.0;
    }
    while remaining < 0.5 - 1e-9 {
        out.push(0.5);
        remaining *= 2.0;
    }
    out.push(remaining);
    out
}

/// 数字统一格式化：最多 4 位小数并去掉尾随 0（2.0 → "2"，1.25 → "1.25"）。
fn fmt_num(value: f64) -> String {
    let text = format!("{value:.4}");
    let trimmed = text.trim_end_matches('0').trim_end_matches('.');
    if trimmed.is_empty() || trimmed == "-" {
        "0".to_string()
    } else {
        trimmed.to_string()
    }
}

/// 视频/音频变速的滤镜片段；speed 归一化后为 1.0 时返回 None（不产生任何滤镜）。
pub fn speed_filters(speed: f64, keep_pitch: bool) -> Option<(String, String)> {
    let speed = speed_or_default(speed);
    if (speed - 1.0).abs() < 1e-9 {
        return None;
    }
    let video = format!("setpts=PTS/{}", fmt_num(speed));
    let audio = if keep_pitch {
        atempo_factors(speed)
            .iter()
            .map(|f| format!("atempo={}", fmt_num(*f)))
            .collect::<Vec<_>>()
            .join(",")
    } else {
        // 音高随速度变化：先把音频重采样到 48 kHz，再改采样率解释（时长变为 1/speed，音高 ×speed），
        // 最后重采样回 48 kHz。atempo 不再叠加，否则会把时长再改一次。
        format!(
            "aresample=48000,asetrate={},aresample=48000",
            fmt_num(48000.0 * speed)
        )
    };
    Some((video, audio))
}

/// 音频滤镜链：音量增益最前（相对增益），随后变速（atempo），最后响度归一化（仅音频卡）。
/// 返回 None 表示整条链为空，调用方不应插入 `-af`。
fn audio_filters(
    volume: &VolumeParams,
    speed: f64,
    keep_pitch: bool,
    normalize: bool,
) -> Option<String> {
    let mut chain: Vec<String> = Vec::new();
    if volume.mute {
        // 音频任务不能像视频那样 -an（会没有输出音轨），改用 volume=0 输出等长静音。
        chain.push("volume=0".into());
    } else if volume.enabled {
        chain.push(format!("volume={}dB", fmt_num(volume.gain_db)));
    }
    if let Some((_, audio_filter)) = speed_filters(speed, keep_pitch) {
        chain.push(audio_filter);
    }
    if normalize {
        chain.push("loudnorm=I=-16:TP=-1.5:LRA=11".into());
    }
    if chain.is_empty() {
        None
    } else {
        Some(chain.join(","))
    }
}

/// 0 / 负数 / NaN 视为「未启用」，返回 1.0。
pub fn speed_or_default(speed: f64) -> f64 {
    if !speed.is_finite() || speed <= 0.0 {
        1.0
    } else {
        speed
    }
}

/// 音频卡专用：音量增益与响度归一化不能同时开启。
/// loudnorm 是「测量后把整体响度拉回 -16 LUFS」，相对增益会先被它抵消，二者语义冲突。
pub fn validate_audio_volume(p: &AudioParams) -> Result<(), String> {
    if p.normalize && p.volume.enabled {
        return Err(
            "音量增益与响度归一化不能同时开启：loudnorm 会把整体响度拉回 -16 LUFS，增益会被抵消；两者语义不同，请二选一"
                .into(),
        );
    }
    Ok(())
}

/// 入队校验：0 视为未启用（通过）；非 0 时必须在 0.25–4.0，否则拒绝并给出可读原因。
pub fn validate_speed(speed: f64) -> Result<(), String> {
    if !speed.is_finite() || speed <= 0.0 {
        return Ok(());
    }
    if !(0.25..=4.0).contains(&speed) {
        return Err(format!(
            "倍速 {speed} 超出支持范围：0.25–4.0（1.0 表示不变速）"
        ));
    }
    Ok(())
}

/// 探测后回填的水印尺寸（输出帧 + 水印源图）。
#[derive(Debug, Clone, Copy, Default)]
pub struct WatermarkDims {
    pub base_width: u32,
    pub base_height: u32,
    pub wm_width: u32,
    pub wm_height: u32,
}

/// 水印不透明度归一化：0 / 非法值视为默认 0.9。
pub fn watermark_opacity(raw: f64) -> f64 {
    if !raw.is_finite() || raw <= 0.0 {
        0.9
    } else {
        raw.clamp(0.0, 1.0)
    }
}

/// 水印目标尺寸：按基准画面短边百分比缩放并保持原图宽高比；
/// 结果不会超出基准画面（超出时按同一比例整体缩小），最小 1 像素。
pub fn watermark_target_size(
    base_width: u32,
    base_height: u32,
    wm_width: u32,
    wm_height: u32,
    scale_percent: u32,
) -> (u32, u32) {
    if base_width == 0 || base_height == 0 || wm_width == 0 || wm_height == 0 {
        return (1, 1);
    }
    let percent = if (5..=50).contains(&scale_percent) {
        scale_percent
    } else {
        15
    };
    let base_short = base_width.min(base_height) as f64;
    let target_short = (base_short * percent as f64 / 100.0).max(1.0);
    let wm_short = wm_width.min(wm_height) as f64;
    let mut factor = target_short / wm_short;
    // 水印不能盖满/超出画面：宽高任一越界都按同一比例整体缩小。
    factor = factor
        .min(base_width as f64 / wm_width as f64)
        .min(base_height as f64 / wm_height as f64);
    let width = (wm_width as f64 * factor).round().max(1.0) as u32;
    let height = (wm_height as f64 * factor).round().max(1.0) as u32;
    (width.min(base_width), height.min(base_height))
}

/// 旋转后的外接矩形尺寸（`rotate` 使用 ow=rotw/oh=roth 时的实际输出大小）。
pub fn watermark_rotated_dims(width: u32, height: u32, degrees: f64) -> (u32, u32) {
    if !degrees.is_finite() || degrees.abs() < 0.01 {
        return (width, height);
    }
    let rad = degrees.to_radians();
    let (sin, cos) = (rad.sin().abs(), rad.cos().abs());
    let w = (width as f64 * cos + height as f64 * sin).round().max(1.0);
    let h = (width as f64 * sin + height as f64 * cos).round().max(1.0);
    (w as u32, h as u32)
}

/// 九宫格 / 自定义坐标（左上角原点）。custom 会被收紧到画面内；
/// 画面小于「水印 + 间距」时，坐标同样不会越界。
pub fn watermark_position(
    position: &str,
    margin: u32,
    base_width: u32,
    base_height: u32,
    wm_width: u32,
    wm_height: u32,
    x: i64,
    y: i64,
) -> (i32, i32) {
    let margin = margin as i64;
    let max_x = (base_width as i64 - wm_width as i64).max(0);
    let max_y = (base_height as i64 - wm_height as i64).max(0);
    let position = if position.is_empty() { "se" } else { position };
    let (px, py) = if position == "custom" {
        (x.clamp(0, max_x), y.clamp(0, max_y))
    } else {
        // 留白最多占可用空间的一半，画面过小时也不会出现「贴左」与「贴右」不一致。
        let margin_x = margin.min(max_x / 2);
        let margin_y = margin.min(max_y / 2);
        let left = margin_x;
        let right = (max_x - margin_x).max(0);
        let top = margin_y;
        let bottom = (max_y - margin_y).max(0);
        let center_x = max_x / 2;
        let center_y = max_y / 2;
        let px = match position {
            "nw" | "w" | "sw" => left,
            "ne" | "e" | "se" => right,
            _ => center_x,
        };
        let py = match position {
            "nw" | "n" | "ne" => top,
            "sw" | "s" | "se" => bottom,
            _ => center_y,
        };
        (px, py)
    };
    (px as i32, py as i32)
}

/// 入队校验：关闭时一律放行；开启时必须给出真实存在且格式受支持的图片。
pub fn validate_watermark(w: &WatermarkParams) -> Result<(), String> {
    if !w.enabled {
        return Ok(());
    }
    let path = w.path.trim();
    if path.is_empty() || !std::path::Path::new(path).is_file() {
        return Err(format!(
            "水印图片不存在：{}（请重新选择水印图片，或关闭水印）",
            w.path
        ));
    }
    let ext = std::path::Path::new(path)
        .extension()
        .map(|e| e.to_string_lossy().to_ascii_lowercase())
        .unwrap_or_default();
    if !["png", "jpg", "jpeg", "webp"].contains(&ext.as_str()) {
        return Err(format!(
            "不支持的水印图片格式「{ext}」，可选 png / jpg / jpeg / webp"
        ));
    }
    if !w.opacity.is_finite() || !(0.0..=1.0).contains(&w.opacity) {
        return Err(format!("水印不透明度 {} 超出范围 0–1", w.opacity));
    }
    if !(5..=50).contains(&w.scale_percent) {
        return Err(format!("水印比例 {}% 超出范围 5–50", w.scale_percent));
    }
    if !w.rotation.is_finite() || !(-180.0..=180.0).contains(&w.rotation) {
        return Err(format!("水印旋转 {}° 超出范围 -180–180", w.rotation));
    }
    match w.position.as_str() {
        "" | "nw" | "n" | "ne" | "w" | "c" | "e" | "sw" | "s" | "se" | "custom" => Ok(()),
        other => Err(format!(
            "不支持的水印位置「{other}」，可选 nw / n / ne / w / c / e / sw / s / se / custom"
        )),
    }
}

/// 字幕模式：空字符串等价 soft。非法值由 `validate_subtitle` 拒绝。
pub fn subtitle_mode(s: &SubtitleParams) -> &str {
    let mode = s.mode.trim();
    if mode.is_empty() {
        "soft"
    } else {
        mode
    }
}

fn subtitle_language(s: &SubtitleParams) -> &str {
    let language = s.language.trim();
    if language.is_empty() {
        "chi"
    } else {
        language
    }
}

/// 烧录字号像素：输出画面高 × 百分比，四舍五入且不小于 8。
pub fn subtitle_font_px(base_height: u32, font_size_pct: f64) -> u32 {
    let pct = if font_size_pct.is_finite() && (1.0..=20.0).contains(&font_size_pct) {
        font_size_pct
    } else {
        4.5
    };
    (base_height as f64 * pct / 100.0).round().max(8.0) as u32
}

/// ASS Alignment：bottom → 2，middle → 5，top → 8。
pub fn subtitle_alignment(position: &str) -> u32 {
    match position.trim() {
        "top" => 8,
        "middle" => 5,
        _ => 2,
    }
}

/// `#RRGGBB` → ASS 颜色 `&HBBGGRR`。
pub fn subtitle_ass_color(color: &str) -> Result<String, String> {
    let raw = color.trim();
    let hex = raw.strip_prefix('#').unwrap_or(raw);
    if hex.len() != 6 || !hex.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err(format!("字幕颜色「{raw}」不是合法的 #RRGGBB 颜色"));
    }
    let upper = hex.to_ascii_uppercase();
    Ok(format!(
        "&H{}{}{}",
        &upper[4..6],
        &upper[2..4],
        &upper[0..2]
    ))
}

/// ffmpeg 滤镜参数会经过两层解析，Windows 路径必须按实测可行的方式转义：
/// 常规路径用单引号包裹 + 反斜杠加倍 + 冒号加反斜杠；含单引号的路径改用
/// 无引号形式（正斜杠）并做三层转义（单引号在两层解析里无法用引号包裹表达）。
fn escape_subtitle_path(path: &str) -> String {
    if path.contains('\'') {
        let slashed = path.replace('\\', "/");
        return slashed.replace(':', r"\\:").replace('\'', r"\\\'");
    }
    format!("'{}'", path.replace('\\', r"\\").replace(':', r"\:"))
}

/// 烧录字幕的滤镜片段。`subtitles` 必须在 scale 之后、水印 overlay 之前。
pub fn hard_subtitle_filter(s: &SubtitleParams) -> Result<String, String> {
    if s.base_height == 0 {
        return Err("无法读取输入画面尺寸，不能按画面高度换算烧录字号".into());
    }
    let px = subtitle_font_px(s.base_height, s.font_size_pct);
    let color = subtitle_ass_color(&s.color)?;
    let style = format!(
        "Fontname=Arial,Fontsize={px},Outline={},Alignment={},PrimaryColour={},MarginV={}",
        s.outline.min(6),
        subtitle_alignment(&s.position),
        color,
        s.margin_v
    );
    Ok(format!(
        "subtitles={}:force_style='{style}'",
        escape_subtitle_path(&s.path)
    ))
}

fn validate_subtitle_file(path: &str) -> Result<(), String> {
    let trimmed = path.trim();
    if trimmed.is_empty() || !std::path::Path::new(trimmed).is_file() {
        return Err(format!(
            "字幕文件不存在：{path}（请重新选择字幕文件，或关闭字幕）"
        ));
    }
    let ext = std::path::Path::new(trimmed)
        .extension()
        .map(|e| e.to_string_lossy().to_ascii_lowercase())
        .unwrap_or_default();
    if !["srt", "ass", "ssa", "vtt"].contains(&ext.as_str()) {
        return Err(format!(
            "不支持的字幕格式「{ext}」，可选 srt / ass / ssa / vtt"
        ));
    }
    Ok(())
}

/// 字幕入队校验：文件、格式、容器能力、变速 + 烧录的组合都在这里拒绝。
pub fn validate_subtitle(s: &SubtitleParams, container: &str, speed: f64) -> Result<(), String> {
    if !s.enabled {
        return Ok(());
    }
    let mode = subtitle_mode(s);
    if !matches!(mode, "soft" | "hard") {
        return Err(format!("不支持的字幕模式「{mode}」，可选 soft / hard"));
    }
    validate_subtitle_file(&s.path)?;
    for extra in &s.extra_paths {
        validate_subtitle_file(extra)?;
    }
    if mode == "hard" {
        if !s.extra_paths.is_empty() {
            return Err("烧录模式只支持一份字幕文件：请移除附加字幕轨，或改为内嵌字幕".into());
        }
        if (speed_or_default(speed) - 1.0).abs() > 1e-9 {
            return Err(
                "变速时不能烧录字幕（字幕时间轴会与变速后的画面/音频不同步）：请改为内嵌字幕，或把倍速设为 1"
                    .into(),
            );
        }
        if !s.font_size_pct.is_finite() || !(1.0..=20.0).contains(&s.font_size_pct) {
            return Err(format!("烧录字幕字号 {}% 超出范围 1–20", s.font_size_pct));
        }
        if s.outline > 6 {
            return Err(format!("字幕描边 {} 超出范围 0–6", s.outline));
        }
        match s.position.trim() {
            "" | "bottom" | "middle" | "top" => {}
            other => {
                return Err(format!(
                    "不支持的字幕位置「{other}」，可选 bottom / middle / top"
                ))
            }
        }
        subtitle_ass_color(&s.color)?;
        return Ok(());
    }
    if crate::containers::subtitle_codec(container).is_none() {
        return Err(subtitle_container_error(container));
    }
    Ok(())
}

fn subtitle_container_error(container: &str) -> String {
    let id = if container.trim().is_empty() {
        "mp4"
    } else {
        container
    };
    let label = crate::containers::find_container(id)
        .map(|c| c.label)
        .unwrap_or("该容器");
    format!("容器 {label} 不承载内嵌字幕轨：请改用 MKV/MP4/MOV，或把字幕烧录进画面")
}

/// 运行期能力校验：hard 模式必须由带 libass 的 FFmpeg 构建执行。
pub fn validate_subtitle_burn(s: &SubtitleParams, burn_available: bool) -> Result<(), String> {
    if !s.enabled || subtitle_mode(s) != "hard" || burn_available {
        return Ok(());
    }
    Err(
        "当前 FFmpeg 构建不包含 libass，无法烧录字幕：请安装带 libass 的 FFmpeg 构建，或把字幕模式改为内嵌字幕"
            .into(),
    )
}

/// 裁剪后的宽高（与 `crop_filter` 的表达式一致：宽高取偶）。
fn crop_dimensions(mode: &str, width: u32, height: u32) -> (u32, u32) {
    let (w, h) = (width as u64, height as u64);
    let (cw, ch) = match mode {
        "16:9" => (w.min(h * 16 / 9), h.min(w * 9 / 16)),
        "1:1" => {
            let side = w.min(h);
            (side, side)
        }
        "9:16" => (w.min(h * 9 / 16), h.min(w * 16 / 9)),
        "4:3" => (w.min(h * 4 / 3), h.min(w * 3 / 4)),
        _ => (w, h),
    };
    let out_w = (((cw / 2) * 2) as u32).max(2).min(width.max(2));
    let out_h = (((ch / 2) * 2) as u32).max(2).min(height.max(2));
    (out_w, out_h)
}

/// 旋转后的画面宽高（90 / 270 交换宽高，180 与翻转不改尺寸）。
fn rotation_dimensions(mode: &str, width: u32, height: u32) -> (u32, u32) {
    match mode {
        "90" | "270" => (height, width),
        _ => (width, height),
    }
}

/// 视频「最终画面」尺寸：裁剪 → 旋转 → 分辨率缩放（scale=-2:h，宽度取偶）。
pub fn video_base_dims(p: &VideoParams, src_width: u32, src_height: u32) -> (u32, u32) {
    let (w, h) = crop_dimensions(&p.crop, src_width, src_height);
    let (mut w, mut h) = rotation_dimensions(&p.rotation, w, h);
    if p.resolution != "source" && !p.resolution.is_empty() {
        if let Ok(target_h) = p.resolution.parse::<u32>() {
            if target_h > 0 {
                let desired = w as f64 * target_h as f64 / h as f64;
                w = ((desired / 2.0).round() as u32 * 2).max(2);
                h = target_h.max(2);
            }
        }
    }
    (w.max(2), h.max(2))
}

/// 图片「最终画面」尺寸：EXIF 方向 → 裁剪 → 手动旋转 → 最长边缩放。
pub fn image_base_dims(
    p: &ImageParams,
    input: &str,
    src_width: u32,
    src_height: u32,
) -> (u32, u32) {
    let (mut w, mut h) = (src_width, src_height);
    // EXIF Orientation 5–8 会交换宽高（与 build_image 的滤镜顺序一致）。
    if p.auto_orient {
        if let Some(orientation) = exif_orientation(input) {
            if matches!(orientation, 5 | 6 | 7 | 8) {
                std::mem::swap(&mut w, &mut h);
            }
        }
    }
    let (cw, ch) = match p.crop_mode.as_str() {
        "ratio" => crop_dimensions(&p.crop_ratio, w, h),
        "custom" => (p.crop_w, p.crop_h),
        _ => (w, h),
    };
    let (rw, rh) = rotation_dimensions(&p.rotation, cw, ch);
    w = rw;
    h = rh;
    if p.max_edge > 0 {
        let edge = p.max_edge as f64;
        let factor = (edge / w as f64).min(edge / h as f64).min(1.0);
        w = (w as f64 * factor).round().max(1.0) as u32;
        h = (h as f64 * factor).round().max(1.0) as u32;
    }
    (w.max(1), h.max(1))
}

/// 生成两段滤镜：`[1:v]` 的水印预处理链与 overlay 定位参数。
/// `shortest` 仅视频需要（静态图输入配 `-loop 1`），图片只取一帧。
fn watermark_plan(w: &WatermarkParams, shortest: bool) -> Result<(String, String), String> {
    if w.base_width == 0 || w.base_height == 0 {
        return Err("无法读取输入画面尺寸，不能按位置叠加水印".into());
    }
    if w.wm_width == 0 || w.wm_height == 0 {
        return Err(format!(
            "无法读取水印图片尺寸：{}（文件可能已损坏，请重新选择）",
            w.path
        ));
    }
    let (tw, th) = watermark_target_size(
        w.base_width,
        w.base_height,
        w.wm_width,
        w.wm_height,
        w.scale_percent,
    );
    let (rw, rh) = watermark_rotated_dims(tw, th, w.rotation);
    let (x, y) = watermark_position(
        &w.position,
        w.margin,
        w.base_width,
        w.base_height,
        rw,
        rh,
        w.x,
        w.y,
    );
    let opacity = fmt_num(watermark_opacity(w.opacity));
    let mut chain = vec![
        format!("scale={tw}:{th}:flags=lanczos"),
        "format=rgba".into(),
        format!("colorchannelmixer=aa={opacity}"),
    ];
    if w.rotation.is_finite() && w.rotation.abs() >= 0.01 {
        let rad = fmt_num(w.rotation.to_radians());
        chain.push(format!("rotate={rad}:ow=rotw({rad}):oh=roth({rad}):c=none"));
    }
    let overlay = format!(
        "overlay={x}:{y}:format=auto{}",
        if shortest { ":shortest=1" } else { "" }
    );
    Ok((chain.join(","), overlay))
}

pub fn build_video(
    encoder: &str,
    input: &str,
    output: &str,
    p: &VideoParams,
    container_audio: &str,
    hwaccel: Option<&str>,
) -> Result<Vec<String>, String> {
    let mut args: Vec<String> = vec![
        "-hide_banner".into(),
        "-nostdin".into(),
        "-y".into(),
        "-loglevel".into(),
        "warning".into(),
        "-progress".into(),
        "pipe:1".into(),
    ];
    if p.hw_decode {
        if let Some(accel) = hwaccel {
            args.push("-hwaccel".into());
            args.push(accel.to_string());
            if accel == "vaapi" {
                let dev = "/dev/dri/renderD128";
                if std::path::Path::new(dev).exists() {
                    args.push("-hwaccel_device".into());
                    args.push(dev.into());
                }
            }
        }
    }
    // VAAPI 编码必须显式指定设备并把系统内存帧上传；未做这一步时 h264_vaapi 一律失败。
    // 设备路径按文档默认 renderD128，Linux 真机需按实际 /dev/dri 调整（属于待验证项）。
    let vaapi_encode = encoder.ends_with("_vaapi");
    let vaapi_device = "/dev/dri/renderD128";
    let vaapi_ok = vaapi_encode && std::path::Path::new(vaapi_device).exists();
    if vaapi_ok {
        args.push("-vaapi_device".into());
        args.push(vaapi_device.into());
    }
    args.push("-i".into());
    args.push(input.into());
    let wm_plan = if p.watermark.enabled {
        Some(watermark_plan(&p.watermark, true)?)
    } else {
        None
    };
    if wm_plan.is_some() {
        // 静态水印图循环读取，铺满整个视频时长。
        args.push("-loop".into());
        args.push("1".into());
        args.push("-i".into());
        args.push(p.watermark.path.clone());
    }
    let subtitle_kind = if p.subtitle.enabled {
        subtitle_mode(&p.subtitle)
    } else {
        ""
    };
    let soft_subtitle = subtitle_kind == "soft";
    let hard_subtitle = subtitle_kind == "hard";
    let soft_codec = if soft_subtitle {
        Some(
            crate::containers::subtitle_codec(&p.container)
                .ok_or_else(|| subtitle_container_error(&p.container))?,
        )
    } else {
        None
    };
    // 字幕文件作为独立输入追加；主输入是 0，水印（若启用）占 1。
    let mut subtitle_input_start = 0usize;
    if soft_subtitle {
        subtitle_input_start = 1 + usize::from(wm_plan.is_some());
        args.push("-i".into());
        args.push(p.subtitle.path.clone());
        for extra in &p.subtitle.extra_paths {
            args.push("-i".into());
            args.push(extra.clone());
        }
    }
    args.push("-map".into());
    if wm_plan.is_some() {
        args.push("[outv]".into());
    } else {
        args.push("0:v:0".into());
    }
    // 静音时不映射源音轨（无论 audio_mode），输出端另行追加 -an。
    if p.audio_mode != "none" && !p.volume.mute {
        args.push("-map".into());
        args.push("0:a:0?".into());
    }
    if soft_subtitle {
        args.push("-map".into());
        args.push(format!("{subtitle_input_start}:s:0"));
        for index in 0..p.subtitle.extra_paths.len() {
            args.push("-map".into());
            args.push(format!("{}:s:0", subtitle_input_start + 1 + index));
        }
    }
    let mut filters: Vec<String> = Vec::new();
    if let Some(crop) = crop_filter(&p.crop) {
        filters.push(crop);
    }
    filters.extend(rotation_filters(&p.rotation));
    if p.resolution != "source" && !p.resolution.is_empty() {
        if let Ok(h) = p.resolution.parse::<u32>() {
            if h > 0 {
                filters.push(format!("scale=-2:{h}:flags=lanczos"));
            }
        }
    }
    if hard_subtitle {
        // 烧录必须发生在 scale 之后（字号按最终画面高度换算），且不能与变速共存。
        if (speed_or_default(p.speed) - 1.0).abs() > 1e-9 {
            return Err(
                "变速时不能烧录字幕（字幕时间轴会与变速后的画面/音频不同步）：请改为内嵌字幕，或把倍速设为 1"
                    .into(),
            );
        }
        filters.push(hard_subtitle_filter(&p.subtitle)?);
    }
    if let Some((video_filter, _)) = speed_filters(p.speed, p.keep_pitch) {
        filters.push(video_filter);
    }
    if vaapi_ok && wm_plan.is_none() {
        filters.push("format=nv12".into());
        filters.push("hwupload".into());
    }
    if let Some((wm_chain, overlay)) = &wm_plan {
        // 主链从 [0:v] 出发（无滤镜时补 null），水印链从 [1:v] 出发，最后 overlay 输出 [outv]。
        // VAAPI 的格式转换与上传必须放在 overlay 之后（overlay 只处理软件帧）。
        let main_chain = if filters.is_empty() {
            "[0:v]null".to_string()
        } else {
            format!("[0:v]{}", filters.join(","))
        };
        let tail = if vaapi_ok {
            ",format=nv12,hwupload"
        } else {
            ""
        };
        let complex =
            format!("{main_chain}[base];[1:v]{wm_chain}[wm];[base][wm]{overlay}{tail}[outv]");
        args.push("-filter_complex".into());
        args.push(complex);
    } else if !filters.is_empty() {
        args.push("-vf".into());
        args.push(filters.join(","));
    }
    if p.fps != "source" && !p.fps.is_empty() {
        if let Ok(fps) = p.fps.parse::<u32>() {
            if fps > 0 {
                args.push("-r".into());
                args.push(fps.to_string());
            }
        }
    }
    if wm_plan.is_some() {
        // 没有 :shortest=1 时静态水印会无限循环，任务永远结束不了。
        args.push("-shortest".into());
    }
    args.push("-c:v".into());
    args.push(encoder.to_string());
    if !vaapi_encode {
        args.push("-pix_fmt".into());
        args.push("yuv420p".into());
    }
    let mut rate = rate_args(encoder, p);
    args.append(&mut rate);

    if p.volume.mute {
        // 静音优先：丢弃音轨，不产生任何音频滤镜（也不再有 -map 0:a）。
        args.push("-an".into());
    } else {
        match p.audio_mode.as_str() {
            "copy" => {
                args.push("-c:a".into());
                args.push("copy".into());
            }
            "none" => {
                args.push("-an".into());
            }
            _ => {
                let kbps = if p.audio_bitrate_kbps > 0 {
                    p.audio_bitrate_kbps
                } else {
                    192
                };
                let audio = if container_audio.trim().is_empty() {
                    "aac"
                } else {
                    container_audio
                };
                // 音量增益最前、变速其后；「原样复制」+ 音量由 precheck 拒绝。
                // 全链只插入一次 -af。
                if let Some(audio_filter) = audio_filters(&p.volume, p.speed, p.keep_pitch, false) {
                    args.push("-af".into());
                    args.push(audio_filter);
                }
                args.push("-c:a".into());
                args.push(audio.into());
                args.push("-b:a".into());
                args.push(format!("{kbps}k"));
            }
        }
    }
    if let Some(codec) = soft_codec {
        args.push("-c:s".into());
        args.push(codec.into());
        let language = subtitle_language(&p.subtitle);
        for index in 0..=p.subtitle.extra_paths.len() {
            args.push(format!("-metadata:s:s:{index}"));
            args.push(format!("language={language}"));
        }
        if p.subtitle.default_track {
            args.push("-disposition:s:0".into());
            args.push("default".into());
        }
    }
    let spec = crate::containers::find_container(if p.container.trim().is_empty() {
        "mp4"
    } else {
        &p.container
    });
    let muxer = spec.map(|c| c.muxer).unwrap_or("mp4");
    if p.faststart && spec.map(|c| c.faststart).unwrap_or(true) {
        args.push("-movflags".into());
        args.push("+faststart".into());
    }
    args.push("-f".into());
    args.push(muxer.into());
    args.push(output.into());
    args.shrink_to_fit();
    Ok(args)
}

pub fn audio_encoder(p: &AudioParams) -> &'static str {
    match p.format.as_str() {
        "aac" => "aac",
        "flac" => "flac",
        "wav" => "pcm_s16le",
        "ogg" => "libvorbis",
        "opus" => "libopus",
        _ => "libmp3lame",
    }
}

pub fn build_audio(encoder: &str, input: &str, output: &str, p: &AudioParams) -> Vec<String> {
    let kbps = if p.bitrate_kbps > 0 {
        p.bitrate_kbps
    } else {
        192
    };
    let spec = crate::containers::find_audio(&p.format);
    let lossy = spec.map(|f| f.lossy).unwrap_or(true);
    let mut args: Vec<String> = vec![
        "-hide_banner".into(),
        "-nostdin".into(),
        "-y".into(),
        "-loglevel".into(),
        "warning".into(),
        "-progress".into(),
        "pipe:1".into(),
        "-i".into(),
        input.into(),
        "-vn".into(),
        "-map".into(),
        "0:a:0?".into(),
        "-c:a".into(),
        encoder.into(),
    ];
    // FLAC / WAV 是无损格式，`-b:a` 对它们无意义（部分构建会直接报错）。
    if lossy {
        args.push("-b:a".into());
        args.push(format!("{kbps}k"));
    }
    if p.sample_rate != "source" && !p.sample_rate.is_empty() {
        if let Ok(sr) = p.sample_rate.parse::<u32>() {
            if sr > 0 {
                args.push("-ar".into());
                args.push(sr.to_string());
            }
        }
    }
    match p.channels.as_str() {
        "mono" => {
            args.push("-ac".into());
            args.push("1".into());
        }
        "stereo" => {
            args.push("-ac".into());
            args.push("2".into());
        }
        _ => {}
    }
    // 音量增益最前、变速其后、响度归一化最后；非空时只插入一次 -af。
    // 与 loudnorm 的组合由 validate_audio_volume 拒绝（增益会被拉回 -16 LUFS 抵消）。
    if let Some(audio_filter) = audio_filters(&p.volume, p.speed, p.keep_pitch, p.normalize) {
        args.push("-af".into());
        args.push(audio_filter);
    }
    args.push(output.into());
    args
}

/// 按时间点切分。copy 走流复制（受关键帧约束），encode 重编码（帧级准确）。
/// 一个任务只切一段；多段由前端拆成多个 JobSpec。
pub fn build_split(input: &str, output: &str, p: &SplitParams, encoder: &str) -> Vec<String> {
    let container = if p.container.trim().is_empty() {
        "mp4"
    } else {
        &p.container
    };
    let spec = crate::containers::find_container(container);
    let muxer = spec.map(|c| c.muxer).unwrap_or("mp4");
    let mut args: Vec<String> = vec![
        "-hide_banner".into(),
        "-nostdin".into(),
        "-y".into(),
        "-loglevel".into(),
        "warning".into(),
        "-progress".into(),
        "pipe:1".into(),
        "-ss".into(),
        fmt_num(p.start_sec),
        "-to".into(),
        fmt_num(p.end_sec),
        "-i".into(),
        input.into(),
    ];
    if p.mode == "encode" {
        args.push("-map".into());
        args.push("0:v:0".into());
        args.push("-map".into());
        args.push("0:a:0?".into());
        args.push("-c:v".into());
        args.push(encoder.into());
        args.push("-preset".into());
        args.push("veryfast".into());
        args.push("-crf".into());
        args.push("20".into());
        args.push("-pix_fmt".into());
        args.push("yuv420p".into());
        args.push("-c:a".into());
        args.push("aac".into());
        args.push("-b:a".into());
        args.push("192k".into());
        if container == "mp4" {
            args.push("-movflags".into());
            args.push("+faststart".into());
        }
    } else {
        args.push("-map".into());
        args.push("0".into());
        args.push("-c".into());
        args.push("copy".into());
        // 输入侧 -ss 会让输出从非零时间戳开始；make_zero 让每段都从 0 起，播放器兼容性更好。
        args.push("-avoid_negative_ts".into());
        args.push("make_zero".into());
    }
    args.push("-f".into());
    args.push(muxer.into());
    args.push(output.into());
    args
}

/// 切分参数校验。`source_duration` 未知时传 f64::INFINITY（跳过时长上界）。
pub fn validate_split(p: &SplitParams, source_duration: f64) -> Result<(), String> {
    if !p.start_sec.is_finite() || p.start_sec < 0.0 {
        return Err("切分开始时间必须 ≥ 0".into());
    }
    if !p.end_sec.is_finite() || p.end_sec <= p.start_sec {
        return Err(format!(
            "切分结束时间必须大于开始时间（{}s → {}s）",
            fmt_num(p.start_sec),
            fmt_num(p.end_sec)
        ));
    }
    if source_duration.is_finite() && p.end_sec > source_duration + 0.05 {
        return Err(format!(
            "切分结束时间 {}s 超出源视频时长 {}s（允许 +0.05s 误差）",
            fmt_num(p.end_sec),
            fmt_num(source_duration)
        ));
    }
    if !["copy", "encode"].contains(&p.mode.as_str()) {
        return Err(format!(
            "不支持的分割模式「{}」，可选 copy（流复制）/ encode（重编码）",
            p.mode
        ));
    }
    let container = if p.container.trim().is_empty() {
        "mp4"
    } else {
        &p.container
    };
    if crate::containers::find_container(container).is_none() {
        return Err(format!(
            "不支持的分割输出容器「{container}」，可选 mp4 / mkv / webm / mov / ts / avi"
        ));
    }
    Ok(())
}

pub fn build_image(input: &str, output: &str, p: &ImageParams) -> Result<Vec<String>, String> {
    let mut args: Vec<String> = vec![
        "-hide_banner".into(),
        "-nostdin".into(),
        "-y".into(),
        "-loglevel".into(),
        "warning".into(),
        "-progress".into(),
        "pipe:1".into(),
        // 由本模块显式接管方向：无论是否开启自动校正，都关闭 ffmpeg 的隐式 autorotate，
        // 避免「ffmpeg 自动转一次 + 我们再转一次」的双重旋转。
        "-noautorotate".into(),
        "-i".into(),
        input.into(),
    ];
    let wm_plan = if p.watermark.enabled {
        Some(watermark_plan(&p.watermark, false)?)
    } else {
        None
    };
    if wm_plan.is_some() {
        args.push("-i".into());
        args.push(p.watermark.path.clone());
    }
    args.push("-map".into());
    if wm_plan.is_some() {
        args.push("[out]".into());
    } else {
        args.push("0:v:0".into());
    }

    let mut filters: Vec<String> = Vec::new();
    if p.auto_orient {
        if let Some(orientation) = exif_orientation(input) {
            filters.extend(exif_filters(orientation).iter().map(|s| s.to_string()));
        }
    }
    if let Some(crop) = image_crop_filter(p) {
        // 与视频面板一致：先按 EXIF 摆正，再裁剪，然后手动旋转，最后缩放。
        filters.push(crop);
    }
    filters.extend(rotation_filters(&p.rotation));
    if p.max_edge > 0 {
        filters.push(format!(
            "scale=w=min(iw\\,{}):h=min(ih\\,{}):force_original_aspect_ratio=decrease:flags=lanczos",
            p.max_edge, p.max_edge
        ));
    }
    if let Some((wm_chain, overlay)) = &wm_plan {
        let main_chain = if filters.is_empty() {
            "[0:v]null".to_string()
        } else {
            format!("[0:v]{}", filters.join(","))
        };
        let complex = format!("{main_chain}[base];[1:v]{wm_chain}[wm];[base][wm]{overlay}[out]");
        args.push("-filter_complex".into());
        args.push(complex);
    } else if !filters.is_empty() {
        args.push("-vf".into());
        args.push(filters.join(","));
    }
    let quality = p.quality.clamp(1, 100);
    match p.format.as_str() {
        "png" => {
            args.push("-c:v".into());
            args.push("png".into());
            args.push("-compression_level".into());
            args.push("6".into());
            if wm_plan.is_some() {
                // overlay 之后帧是 rgba，png 编码器默认会保留 alpha；这里固定 rgb24 输出。
                args.push("-pix_fmt".into());
                args.push("rgb24".into());
            }
        }
        "webp" => {
            args.push("-c:v".into());
            args.push("libwebp".into());
            args.push("-quality".into());
            args.push(quality.to_string());
            if wm_plan.is_some() {
                args.push("-pix_fmt".into());
                args.push("yuv420p".into());
            }
        }
        "bmp" => {
            args.push("-c:v".into());
            args.push("bmp".into());
        }
        "tiff" => {
            args.push("-c:v".into());
            args.push("tiff".into());
        }
        _ => {
            // jpg：ffmpeg 的 q 值 1..31，越小越好，这里把 1..100 的"质量"反向映射。
            let q = 31 - ((quality as f64 / 100.0) * 29.0).round() as i32;
            args.push("-c:v".into());
            args.push("mjpeg".into());
            args.push("-q:v".into());
            args.push(q.clamp(2, 31).to_string());
            if wm_plan.is_some() {
                // mjpeg 不接受 rgba；jpg 的完整色度需要 yuvj420p。
                args.push("-pix_fmt".into());
                args.push("yuvj420p".into());
            }
        }
    }
    args.push("-frames:v".into());
    args.push("1".into());
    args.push(output.into());
    Ok(args)
}

/// EXIF Orientation → 像素变换（与 ffmpeg 的官方映射一致）。
/// 注意：这里只消费方向元数据，不做任何隐式旋转；调用方需同时传 `-noautorotate`。
fn exif_filters(orientation: u16) -> &'static [&'static str] {
    match orientation {
        2 => &["hflip"],
        3 => &["hflip", "vflip"],
        4 => &["vflip"],
        5 => &["transpose=0"],
        6 => &["transpose=1"],
        7 => &["transpose=3"],
        8 => &["transpose=2"],
        _ => &[],
    }
}

/// 最小 EXIF 解析：只取 JPEG APP1 里的 IFD0 Orientation(0x0112)，不做其他元数据处理。
/// 返回 1..=8；不是 JPEG、没有 EXIF 或解析失败都返回 None。
pub fn exif_orientation(path: &str) -> Option<u16> {
    use std::io::Read;
    let mut file = std::fs::File::open(path).ok()?;
    // EXIF 一定在文件头部附近；只读前 256 KiB，避免把超大图片整个读进内存。
    let mut head = vec![0u8; 256 * 1024];
    let mut read = 0usize;
    loop {
        match file.read(&mut head[read..]) {
            Ok(0) => break,
            Ok(n) => {
                read += n;
                if read == head.len() {
                    break;
                }
            }
            Err(_) => return None,
        }
    }
    head.truncate(read);
    parse_exif_orientation(&head)
}

fn parse_exif_orientation(bytes: &[u8]) -> Option<u16> {
    if bytes.len() < 4 || bytes[0] != 0xFF || bytes[1] != 0xD8 {
        return None;
    }
    let mut pos = 2usize;
    while pos + 4 <= bytes.len() {
        if bytes[pos] != 0xFF {
            return None;
        }
        let marker = bytes[pos + 1];
        // SOS(0xDA) 之后是压缩数据，元数据不会再出现。
        if marker == 0xDA || marker == 0xD9 {
            return None;
        }
        let len = u16::from_be_bytes([bytes[pos + 2], bytes[pos + 3]]) as usize;
        if len < 2 || pos + 2 + len > bytes.len() {
            return None;
        }
        if marker == 0xE1 {
            let app1 = &bytes[pos + 4..pos + 2 + len];
            if app1.len() > 6 && &app1[..6] == b"Exif\0\0" {
                return parse_tiff_orientation(&app1[6..]);
            }
        }
        pos += 2 + len;
    }
    None
}

fn parse_tiff_orientation(tiff: &[u8]) -> Option<u16> {
    if tiff.len() < 8 {
        return None;
    }
    let little = match &tiff[..2] {
        b"II" => true,
        b"MM" => false,
        _ => return None,
    };
    let read_u16 = |at: usize| -> Option<u16> {
        let raw: [u8; 2] = tiff.get(at..at + 2)?.try_into().ok()?;
        Some(if little {
            u16::from_le_bytes(raw)
        } else {
            u16::from_be_bytes(raw)
        })
    };
    let read_u32 = |at: usize| -> Option<u32> {
        let raw: [u8; 4] = tiff.get(at..at + 4)?.try_into().ok()?;
        Some(if little {
            u32::from_le_bytes(raw)
        } else {
            u32::from_be_bytes(raw)
        })
    };
    let magic = read_u16(2)?;
    if magic != 42 {
        return None;
    }
    let ifd0 = read_u32(4)? as usize;
    let count = read_u16(ifd0)? as usize;
    for index in 0..count {
        let entry = ifd0 + 2 + index * 12;
        let tag = read_u16(entry)?;
        if tag != 0x0112 {
            continue;
        }
        let kind = read_u16(entry + 2)?;
        let value_count = read_u32(entry + 4)?;
        if kind != 3 || value_count == 0 {
            return None;
        }
        let value = read_u16(entry + 8)?;
        return if (1..=8).contains(&value) {
            Some(value)
        } else {
            None
        };
    }
    None
}

/// 同参判定用于「合并时能否直接 stream copy」。任一字段不同都返回不兼容原因。
pub fn merge_incompatibility(
    first: &crate::probe::MediaInfo,
    next: &crate::probe::MediaInfo,
) -> Option<String> {
    if first.video_codec != next.video_codec {
        return Some(format!(
            "编码不同（{:?} vs {:?}）",
            first.video_codec, next.video_codec
        ));
    }
    if first.width != next.width || first.height != next.height {
        return Some(format!(
            "分辨率不同（{}x{} vs {}x{}）",
            first.width.unwrap_or(0),
            first.height.unwrap_or(0),
            next.width.unwrap_or(0),
            next.height.unwrap_or(0)
        ));
    }
    let fps_round = |v: Option<f64>| v.map(|f| (f * 100.0).round() / 100.0);
    if fps_round(first.fps) != fps_round(next.fps) {
        return Some(format!("帧率不同（{:?} vs {:?}）", first.fps, next.fps));
    }
    if first.audio_codec != next.audio_codec
        || first.sample_rate != next.sample_rate
        || first.channels != next.channels
    {
        return Some(format!(
            "音轨参数不同（{:?}/{:?}ch/{}Hz vs {:?}/{:?}ch/{}Hz）",
            first.audio_codec,
            first.channels,
            first.sample_rate.unwrap_or(0),
            next.audio_codec,
            next.channels,
            next.sample_rate.unwrap_or(0)
        ));
    }
    None
}

/// concat demuxer 的清单文件内容。单引号内的路径需把 `'` 转义成 `'\''`。
pub fn concat_list(inputs: &[String]) -> String {
    let mut text = String::new();
    for input in inputs {
        // concat demuxer 用 av_get_token 解析清单：Windows 反斜杠会被当成转义符吃掉，
        // 导致 D:\dir\a.mp4 被读成 D:dira.mp4。统一成正斜杠（ffmpeg 在 Windows 上同样接受）。
        let normalized = input.replace('\\', "/");
        let escaped = normalized.replace('\'', "'\\''");
        text.push_str(&format!("file '{escaped}'\n"));
    }
    text
}

/// 直接拼接：要求所有输入编码/分辨率/帧率/音轨参数一致。
pub fn build_concat_copy(list_file: &str, output: &str) -> Vec<String> {
    vec![
        "-hide_banner".into(),
        "-nostdin".into(),
        "-y".into(),
        "-loglevel".into(),
        "warning".into(),
        "-progress".into(),
        "pipe:1".into(),
        "-f".into(),
        "concat".into(),
        "-safe".into(),
        "0".into(),
        "-i".into(),
        list_file.into(),
        "-map".into(),
        "0:v:0".into(),
        "-map".into(),
        "0:a:0?".into(),
        "-c".into(),
        "copy".into(),
        "-movflags".into(),
        "+faststart".into(),
        "-f".into(),
        "mp4".into(),
        output.into(),
    ]
}

/// 合并前的统一转码片段：固定编码/像素格式/帧率/音频，使后续 concat copy 成立。
pub fn build_merge_segment(
    encoder: &str,
    input: &str,
    output: &str,
    p: &MergeParams,
    target_height: Option<u32>,
    target_fps: Option<u32>,
) -> Vec<String> {
    let mut args: Vec<String> = vec![
        "-hide_banner".into(),
        "-nostdin".into(),
        "-y".into(),
        "-loglevel".into(),
        "warning".into(),
        "-progress".into(),
        "pipe:1".into(),
        "-i".into(),
        input.into(),
        "-map".into(),
        "0:v:0".into(),
        "-map".into(),
        "0:a:0?".into(),
    ];
    let mut filters: Vec<String> = Vec::new();
    if let Some(height) = target_height.filter(|h| *h > 0) {
        filters.push(format!("scale=-2:{height}:flags=lanczos"));
    }
    if let Some(fps) = target_fps.filter(|f| *f > 0) {
        filters.push(format!("fps={fps}"));
    }
    filters.push("format=yuv420p".into());
    args.push("-vf".into());
    args.push(filters.join(","));
    args.push("-c:v".into());
    args.push(encoder.into());
    args.push("-pix_fmt".into());
    args.push("yuv420p".into());
    let mut rate = rate_args(
        encoder,
        &VideoParams {
            rate_mode: if p.bitrate_kbps > 0 {
                "bitrate".into()
            } else {
                "quality".into()
            },
            quality: if p.quality > 0 { p.quality } else { 23 },
            bitrate_kbps: p.bitrate_kbps,
            resolution: target_height
                .map(|h| h.to_string())
                .unwrap_or_else(|| "1080".into()),
            ..Default::default()
        },
    );
    args.append(&mut rate);
    if p.audio_mode == "none" {
        args.push("-an".into());
    } else {
        args.push("-c:a".into());
        args.push("aac".into());
        args.push("-b:a".into());
        args.push("192k".into());
        args.push("-ar".into());
        args.push("48000".into());
        args.push("-ac".into());
        args.push("2".into());
    }
    args.push("-f".into());
    args.push("mp4".into());
    args.push(output.into());
    args
}

/// 混流：视频流 copy，音频按 replace/add 处理。
/// 输出时长以源视频为准（`-t`），避免音轨更长时把视频“拖长”。
pub fn build_mux(
    video: &str,
    audio: &str,
    output: &str,
    p: &MuxParams,
    video_duration: Option<f64>,
) -> Vec<String> {
    let mut args: Vec<String> = vec![
        "-hide_banner".into(),
        "-nostdin".into(),
        "-y".into(),
        "-loglevel".into(),
        "warning".into(),
        "-progress".into(),
        "pipe:1".into(),
        "-i".into(),
        video.into(),
        "-i".into(),
        audio.into(),
        "-map".into(),
        "0:v:0".into(),
    ];
    if p.mode == "add" {
        // 原视频可能没有音轨，用可选映射；追加新音轨作为第 1 条音频流。
        args.push("-map".into());
        args.push("0:a:0?".into());
        args.push("-map".into());
        args.push("1:a:0".into());
    } else {
        args.push("-map".into());
        args.push("1:a:0".into());
    }
    args.push("-c:v".into());
    args.push("copy".into());
    if p.audio_mode == "copy" {
        args.push("-c:a".into());
        args.push("copy".into());
    } else {
        let kbps = if p.audio_bitrate_kbps > 0 {
            p.audio_bitrate_kbps
        } else {
            192
        };
        args.push("-c:a".into());
        args.push("aac".into());
        args.push("-b:a".into());
        args.push(format!("{kbps}k"));
    }
    if let Some(duration) = video_duration.filter(|d| *d > 0.0) {
        args.push("-t".into());
        args.push(format!("{duration:.3}"));
    }
    args.push("-movflags".into());
    args.push("+faststart".into());
    args.push("-f".into());
    args.push("mp4".into());
    args.push(output.into());
    args
}

/// 视频转 GIF。optimize=true 时在同一次进程里用 split 复制两路：
/// 一路 palettegen 生成全局调色板，另一路与调色板合成为 paletteuse 输出。
pub fn build_gif(input: &str, output: &str, p: &GifParams) -> Vec<String> {
    let fps = if p.fps > 0 { p.fps.clamp(1, 60) } else { 10 };
    let width = p.width;
    let mut filters: Vec<String> = vec![format!("fps={fps}")];
    if width > 0 {
        filters.push(format!("scale={width}:-2:flags=lanczos"));
    }
    let mut filter = filters.join(",");
    if p.optimize {
        // 带标签的滤镜必须用分号另起一条链，不能用逗号拼在同一个链里。
        filter.push_str(
            ",split[s0][s1];[s0]palettegen=max_colors=256:stats_mode=diff[p];[s1][p]paletteuse=dither=sierra2_4a",
        );
    }
    let mut args: Vec<String> = vec![
        "-hide_banner".into(),
        "-nostdin".into(),
        "-y".into(),
        "-loglevel".into(),
        "warning".into(),
        "-progress".into(),
        "pipe:1".into(),
    ];
    if p.start_sec > 0.0 {
        args.push("-ss".into());
        args.push(format!("{:.3}", p.start_sec));
    }
    args.push("-i".into());
    args.push(input.into());
    if p.duration_sec > 0.0 {
        args.push("-t".into());
        args.push(format!("{:.3}", p.duration_sec));
    }
    args.push("-vf".into());
    args.push(filter);
    args.push("-loop".into());
    args.push("0".into());
    args.push(output.into());
    args
}

/// 多图 GIF 的 concat 清单：每张图 `file` + `duration 1/fps`，最后一张重复一次
/// （concat demuxer 要求最后一条不带 duration 的 file 行，否则尾帧会被截断）。
pub fn gif_concat_list(inputs: &[String], fps: u32) -> String {
    let fps = if fps > 0 { fps } else { 10 };
    let mut text = String::new();
    for input in inputs {
        // 与 concat_list 一致：反斜杠转正斜杠，单引号按 concat demuxer 规则转义。
        let normalized = input.replace('\\', "/");
        let escaped = normalized.replace('\'', "'\\''");
        text.push_str(&format!("file '{escaped}'\n"));
        text.push_str(&format!("duration {}\n", fmt_num(1.0 / f64::from(fps))));
    }
    if let Some(last) = inputs.last() {
        let normalized = last.replace('\\', "/");
        let escaped = normalized.replace('\'', "'\\''");
        text.push_str(&format!("file '{escaped}'\n"));
    }
    text
}

/// 目标宽度：max_width>0 用它，否则用首图宽度。宽度为 0 时省略 scale 滤镜。
pub fn gif_target_width(max_width: u32, first_width: u32) -> u32 {
    if max_width > 0 {
        max_width
    } else {
        first_width
    }
}

/// GIF 第一遍：从 concat 清单生成全局调色板。
pub fn build_gif_pass1(
    concat_list: &str,
    palette_path: &str,
    p: &ImageGifParams,
    width: u32,
) -> Vec<String> {
    let fps = if p.fps > 0 { p.fps.clamp(1, 30) } else { 10 };
    let mut filters: Vec<String> = vec![format!("fps={fps}")];
    if width > 0 {
        filters.push(format!("scale={width}:-2:flags=lanczos"));
    }
    filters.push("palettegen=stats_mode=diff".into());
    vec![
        "-hide_banner".into(),
        "-nostdin".into(),
        "-y".into(),
        "-loglevel".into(),
        "warning".into(),
        "-progress".into(),
        "pipe:1".into(),
        "-f".into(),
        "concat".into(),
        "-safe".into(),
        "0".into(),
        "-i".into(),
        concat_list.into(),
        "-vf".into(),
        filters.join(","),
        palette_path.into(),
    ]
}

/// 给 GIF 第二遍命令加帧数上限。
/// concat 清单末尾重复的 file 行会让 fps 滤镜多输出一帧（尾帧重复），
/// 按输入图片数量封顶后，帧数严格等于图片张数。
pub fn limit_gif_frames(argv: &mut Vec<String>, frames: u32) {
    if frames == 0 || argv.is_empty() {
        return;
    }
    let out_index = argv.len() - 1;
    argv.splice(
        out_index..out_index,
        ["-frames:v".to_string(), frames.to_string()],
    );
}

/// GIF 第二遍：用第一遍的全局调色板合成 GIF（禁止单遍 -vf gif）。
pub fn build_gif_pass2(
    concat_list: &str,
    palette_path: &str,
    out: &str,
    p: &ImageGifParams,
    width: u32,
) -> Vec<String> {
    let fps = if p.fps > 0 { p.fps.clamp(1, 30) } else { 10 };
    let mut filters: Vec<String> = vec![format!("fps={fps}")];
    if width > 0 {
        filters.push(format!("scale={width}:-2:flags=lanczos"));
    }
    let dither = if p.dither { "sierra2_4a" } else { "none" };
    filters.push(format!("paletteuse=dither={dither}"));
    vec![
        "-hide_banner".into(),
        "-nostdin".into(),
        "-y".into(),
        "-loglevel".into(),
        "warning".into(),
        "-progress".into(),
        "pipe:1".into(),
        "-f".into(),
        "concat".into(),
        "-safe".into(),
        "0".into(),
        "-i".into(),
        concat_list.into(),
        "-i".into(),
        palette_path.into(),
        "-lavfi".into(),
        filters.join(","),
        "-loop".into(),
        p.loop_count.to_string(),
        out.into(),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn video_params() -> VideoParams {
        VideoParams {
            container: "mp4".into(),
            codec: "h264".into(),
            encoder: "libx264".into(),
            rate_mode: "quality".into(),
            quality: 23,
            bitrate_kbps: 0,
            resolution: "source".into(),
            fps: "source".into(),
            crop: "source".into(),
            rotation: "0".into(),
            audio_mode: "none".into(),
            audio_bitrate_kbps: 192,
            hw_decode: false,
            faststart: true,
            speed: 1.0,
            keep_pitch: true,
            watermark: WatermarkParams::default(),
            subtitle: SubtitleParams::default(),
            volume: VolumeParams::default(),
            split: SplitParams::default(),
        }
    }

    fn audio_filter_arg(args: &[String]) -> Option<String> {
        let pos = args.iter().position(|a| a == "-af")?;
        args.get(pos + 1).cloned()
    }

    fn count_flag(args: &[String], flag: &str) -> usize {
        args.iter().filter(|a| a.as_str() == flag).count()
    }

    #[test]
    fn speed_one_adds_no_filter() {
        let mut p = video_params();
        p.speed = 1.0;
        p.audio_mode = "aac".into();
        let args = build_video("libx264", "in.mp4", "out.mp4", &p, "aac", None).unwrap();
        assert!(filter_arg(&args).is_none(), "原速不应插入 -vf");
        assert!(audio_filter_arg(&args).is_none(), "原速不应插入 -af");
    }

    #[test]
    fn speed_1_5_single_atempo() {
        let mut p = video_params();
        p.speed = 1.5;
        p.audio_mode = "aac".into();
        let args = build_video("libx264", "in.mp4", "out.mp4", &p, "aac", None).unwrap();
        let vf = filter_arg(&args).expect("应生成 -vf");
        assert_eq!(vf, "setpts=PTS/1.5");
        assert_eq!(audio_filter_arg(&args).as_deref(), Some("atempo=1.5"));
    }

    #[test]
    fn speed_0_25_chains_two_half() {
        let mut p = video_params();
        p.speed = 0.25;
        p.audio_mode = "aac".into();
        let args = build_video("libx264", "in.mp4", "out.mp4", &p, "aac", None).unwrap();
        assert_eq!(
            audio_filter_arg(&args).as_deref(),
            Some("atempo=0.5,atempo=0.5")
        );
    }

    #[test]
    fn speed_4_chains_two_double() {
        let mut p = video_params();
        p.speed = 4.0;
        p.audio_mode = "aac".into();
        let args = build_video("libx264", "in.mp4", "out.mp4", &p, "aac", None).unwrap();
        assert_eq!(
            audio_filter_arg(&args).as_deref(),
            Some("atempo=2,atempo=2")
        );
    }

    #[test]
    fn speed_2_5_chains_two_and_1_25() {
        let mut p = video_params();
        p.speed = 2.5;
        p.audio_mode = "aac".into();
        let args = build_video("libx264", "in.mp4", "out.mp4", &p, "aac", None).unwrap();
        assert_eq!(
            audio_filter_arg(&args).as_deref(),
            Some("atempo=2,atempo=1.25")
        );
    }

    #[test]
    fn atempo_product_matches_target() {
        for target in [0.25, 0.4, 0.5, 1.5, 2.0, 2.5, 3.0, 4.0] {
            let factors = atempo_factors(target);
            assert!(!factors.is_empty(), "{target} 应至少一个因子");
            for factor in &factors {
                assert!(
                    (0.5..=2.0).contains(factor),
                    "因子 {factor} 超出 0.5–2.0（目标 {target}）"
                );
            }
            let product: f64 = factors.iter().product();
            assert!(
                (product - target).abs() < 1e-6,
                "乘积 {product} 与目标 {target} 不一致"
            );
        }
    }

    #[test]
    fn speed_with_crop_rotate_filter_order() {
        let mut p = video_params();
        p.crop = "9:16".into();
        p.rotation = "90".into();
        p.resolution = "720".into();
        p.speed = 2.0;
        p.audio_mode = "aac".into();
        let args = build_video("libx264", "in.mp4", "out.mp4", &p, "aac", None).unwrap();
        let vf = filter_arg(&args).expect("应生成 -vf");
        assert!(vf.starts_with("crop="), "裁剪应最先应用：{vf}");
        assert!(vf.contains("transpose=1"), "旋转应在裁剪之后：{vf}");
        assert!(vf.contains("scale=-2:720"), "缩放应在旋转之后：{vf}");
        assert!(vf.ends_with("setpts=PTS/2"), "变速应最后应用：{vf}");
        assert_eq!(vf.matches("setpts=").count(), 1, "{vf}");
        assert_eq!(count_flag(&args, "-af"), 1, "{args:?}");
    }

    #[test]
    fn pitch_off_uses_asetrate() {
        let mut p = video_params();
        p.speed = 2.0;
        p.keep_pitch = false;
        p.audio_mode = "aac".into();
        let args = build_video("libx264", "in.mp4", "out.mp4", &p, "aac", None).unwrap();
        assert_eq!(
            audio_filter_arg(&args).as_deref(),
            Some("aresample=48000,asetrate=96000,aresample=48000")
        );
    }

    #[test]
    fn validate_speed_rejects_out_of_range() {
        assert!(validate_speed(0.1).is_err());
        assert!(validate_speed(5.0).is_err());
        assert!(validate_speed(0.25).is_ok());
        assert!(validate_speed(4.0).is_ok());
        assert!(validate_speed(0.0).is_ok());
    }

    #[test]
    fn keep_pitch_defaults_to_true_when_omitted() {
        // 前端未传 keepPitch 时必须默认保持音高，否则升速会意外变调。
        let video: VideoParams = serde_json::from_str(r#"{"speed":1.5}"#).unwrap();
        assert!(video.keep_pitch, "视频参数缺 keepPitch 时应默认保持音高");
        let audio: AudioParams = serde_json::from_str(r#"{"format":"mp3","speed":1.5}"#).unwrap();
        assert!(audio.keep_pitch, "音频参数缺 keepPitch 时应默认保持音高");
        let explicit: AudioParams = serde_json::from_str(r#"{"keepPitch":false}"#).unwrap();
        assert!(!explicit.keep_pitch, "显式 false 必须生效");
    }

    #[test]
    fn audio_speed_precedes_loudnorm() {
        let p = AudioParams {
            format: "mp3".into(),
            bitrate_kbps: 192,
            sample_rate: "source".into(),
            channels: "source".into(),
            normalize: true,
            speed: 1.5,
            keep_pitch: true,
            volume: VolumeParams::default(),
        };
        let args = build_audio("libmp3lame", "in.wav", "out.mp3", &p);
        assert_eq!(count_flag(&args, "-af"), 1);
        assert_eq!(
            audio_filter_arg(&args).as_deref(),
            Some("atempo=1.5,loudnorm=I=-16:TP=-1.5:LRA=11")
        );
    }

    fn filter_arg(args: &[String]) -> Option<String> {
        let pos = args.iter().position(|a| a == "-vf")?;
        args.get(pos + 1).cloned()
    }

    #[test]
    fn video_filter_order_is_crop_rotate_scale() {
        let mut p = video_params();
        p.crop = "9:16".into();
        p.rotation = "90".into();
        p.resolution = "720".into();
        let args = build_video("libx264", "in.mp4", "out.mp4", &p, "aac", None).unwrap();
        let vf = filter_arg(&args).expect("应生成 -vf");
        assert!(
            vf.starts_with("crop=w='trunc(min(iw,ih*9/16)/2)*2'"),
            "裁剪应最先应用：{vf}"
        );
        assert!(vf.contains(",transpose=1,"), "旋转应在裁剪之后：{vf}");
        assert!(
            vf.ends_with(",scale=-2:720:flags=lanczos"),
            "缩放应最后应用：{vf}"
        );
    }

    #[test]
    fn rotation_180_uses_flip_pair() {
        let mut p = video_params();
        p.rotation = "180".into();
        let args = build_video("libx264", "in.mp4", "out.mp4", &p, "aac", None).unwrap();
        assert_eq!(filter_arg(&args).as_deref(), Some("hflip,vflip"));
    }

    #[test]
    fn no_adjustments_means_no_filter_graph() {
        let p = video_params();
        let args = build_video("libx264", "in.mp4", "out.mp4", &p, "aac", None).unwrap();
        assert!(
            filter_arg(&args).is_none(),
            "无裁剪/旋转/缩放时不应插入 -vf"
        );
    }

    #[test]
    fn crop_16_9_and_1_1_expressions() {
        let mut p = video_params();
        p.crop = "1:1".into();
        let square = build_video("libx264", "in.mp4", "out.mp4", &p, "aac", None).unwrap();
        assert_eq!(
            filter_arg(&square).as_deref(),
            Some("crop=w='trunc(min(iw,ih)/2)*2':h='trunc(min(iw,ih)/2)*2'")
        );

        p.crop = "16:9".into();
        let wide = build_video("libx264", "in.mp4", "out.mp4", &p, "aac", None).unwrap();
        let vf = filter_arg(&wide).expect("应生成 -vf");
        assert!(vf.contains("ih*16/9") && vf.contains("iw*9/16"), "{vf}");
    }

    #[test]
    fn audio_normalize_adds_loudnorm() {
        let p = AudioParams {
            format: "mp3".into(),
            bitrate_kbps: 192,
            sample_rate: "source".into(),
            channels: "source".into(),
            normalize: true,
            speed: 1.0,
            keep_pitch: true,
            volume: VolumeParams::default(),
        };
        let args = build_audio("libmp3lame", "in.wav", "out.mp3", &p);
        let pos = args.iter().position(|a| a == "-af").expect("应插入 -af");
        assert_eq!(args[pos + 1], "loudnorm=I=-16:TP=-1.5:LRA=11");
    }

    fn exif_jpeg(orientation: u16) -> Vec<u8> {
        let mut bytes = vec![0xFF, 0xD8]; // SOI
        bytes.extend_from_slice(&[0xFF, 0xE1, 0x00, 0x20]); // APP1 + length 32
        bytes.extend_from_slice(b"Exif\0\0");
        bytes.extend_from_slice(&[0x49, 0x49, 0x2A, 0x00, 0x08, 0x00, 0x00, 0x00]); // II, 42, IFD0=8
        bytes.extend_from_slice(&[0x01, 0x00]); // 1 个条目
        bytes.extend_from_slice(&[0x12, 0x01, 0x03, 0x00]); // tag 0x0112, SHORT
        bytes.extend_from_slice(&[0x01, 0x00, 0x00, 0x00]); // count=1
        bytes.extend_from_slice(&orientation.to_le_bytes());
        bytes.extend_from_slice(&[0x00, 0x00]); // value padding
        bytes.extend_from_slice(&[0x00, 0x00, 0x00, 0x00]); // 下一个 IFD
        bytes.extend_from_slice(&[0xFF, 0xD9]); // EOI
        bytes
    }

    #[test]
    fn exif_orientation_parser_reads_ifd0() {
        for orientation in 1u16..=8 {
            assert_eq!(
                parse_exif_orientation(&exif_jpeg(orientation)),
                Some(orientation)
            );
        }
        // 非 JPEG / 缺 EXIF 一律返回 None
        assert_eq!(parse_exif_orientation(b"not a jpeg"), None);
        let png = [0x89, b'P', b'N', b'G'];
        assert_eq!(parse_exif_orientation(&png), None);
    }

    #[test]
    fn exif_orientation_maps_to_official_filters() {
        assert_eq!(exif_filters(1), &[] as &[&str]);
        assert_eq!(exif_filters(3), &["hflip", "vflip"]);
        assert_eq!(exif_filters(6), &["transpose=1"]);
        assert_eq!(exif_filters(8), &["transpose=2"]);
    }

    #[test]
    fn image_build_disables_ffmpeg_autorotate_to_prevent_double_rotation() {
        let dir = std::env::temp_dir().join(format!("ff-args-exif-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let input = dir.join("oriented.jpg");
        std::fs::write(&input, exif_jpeg(6)).unwrap();

        let params = ImageParams {
            format: "jpg".into(),
            quality: 90,
            max_edge: 0,
            rotation: "0".into(),
            auto_orient: true,
            crop_mode: "none".into(),
            ..Default::default()
        };
        let args = build_image(&input.to_string_lossy(), "out.jpg", &params).unwrap();
        // 关键：ffmpeg 自身的 autorotate 必须关闭，否则会和显式 transpose 叠加。
        assert!(args.contains(&"-noautorotate".to_string()));
        let vf = args[args.iter().position(|a| a == "-vf").unwrap() + 1].clone();
        assert_eq!(vf, "transpose=1");

        // 关闭自动校正时不应出现任何 EXIF 变换。
        let params_off = ImageParams {
            auto_orient: false,
            ..params.clone()
        };
        let args_off = build_image(&input.to_string_lossy(), "out.jpg", &params_off).unwrap();
        assert!(!args_off.contains(&"-vf".to_string()));

        // 自动校正 + 手动 90°，必须是「EXIF 一次 + 手动一次」，而不是被 ffmpeg 再隐式转一次。
        let params_manual = ImageParams {
            rotation: "90".into(),
            ..params
        };
        let args_manual = build_image(&input.to_string_lossy(), "out.jpg", &params_manual).unwrap();
        let vf_manual = args_manual
            .iter()
            .position(|a| a == "-vf")
            .map(|i| args_manual[i + 1].clone())
            .unwrap_or_default();
        assert_eq!(vf_manual.matches("transpose=").count(), 2, "{vf_manual}");

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn gif_build_uses_palette_two_pass_only_when_optimized() {
        let p = GifParams {
            fps: 12,
            width: 320,
            start_sec: 1.5,
            duration_sec: 2.0,
            optimize: true,
        };
        let args = build_gif("in.mp4", "out.gif", &p);
        assert_eq!(
            args[args.iter().position(|a| a == "-ss").unwrap() + 1],
            "1.500"
        );
        assert_eq!(
            args[args.iter().position(|a| a == "-t").unwrap() + 1],
            "2.000"
        );
        let vf = args[args.iter().position(|a| a == "-vf").unwrap() + 1].clone();
        assert!(vf.contains("fps=12"));
        assert!(vf.contains("scale=320:-2"));
        assert!(vf.contains("palettegen") && vf.contains("paletteuse"));

        let plain = GifParams {
            optimize: false,
            ..p
        };
        let args_plain = build_gif("in.mp4", "out.gif", &plain);
        let vf_plain = args_plain[args_plain.iter().position(|a| a == "-vf").unwrap() + 1].clone();
        assert!(!vf_plain.contains("palettegen"), "{vf_plain}");
    }

    #[test]
    fn mux_build_replaces_or_adds_audio_without_reencoding_video() {
        let replace = MuxParams {
            mode: "replace".into(),
            audio_mode: "aac".into(),
            audio_bitrate_kbps: 192,
        };
        let args = build_mux("v.mp4", "a.wav", "out.mp4", &replace, Some(3.0));
        let maps: Vec<&String> = args
            .iter()
            .filter(|a| a.starts_with("0:") || a.starts_with("1:"))
            .collect();
        assert_eq!(maps, vec!["0:v:0", "1:a:0"]);
        let pos = args.iter().position(|a| a == "-c:v").unwrap();
        assert_eq!(args[pos + 1], "copy");
        assert_eq!(
            args[args.iter().position(|a| a == "-t").unwrap() + 1],
            "3.000"
        );

        let add = MuxParams {
            mode: "add".into(),
            audio_mode: "copy".into(),
            ..replace
        };
        let args_add = build_mux("v.mp4", "a.aac", "out.mp4", &add, None);
        let maps_add: Vec<&String> = args_add
            .iter()
            .filter(|a| a.starts_with("0:") || a.starts_with("1:"))
            .collect();
        assert_eq!(maps_add, vec!["0:v:0", "0:a:0?", "1:a:0"]);
        assert_eq!(
            args_add[args_add.iter().position(|a| a == "-c:a").unwrap() + 1],
            "copy"
        );
    }

    #[test]
    fn merge_incompatibility_flags_each_dimension() {
        let base = crate::probe::MediaInfo {
            video_codec: Some("h264".into()),
            audio_codec: Some("aac".into()),
            width: Some(1280),
            height: Some(720),
            fps: Some(30.0),
            sample_rate: Some(48_000),
            channels: Some(2),
            ..Default::default()
        };
        assert!(merge_incompatibility(&base, &base).is_none());
        let different_res = crate::probe::MediaInfo {
            width: Some(640),
            height: Some(480),
            ..base.clone()
        };
        assert!(merge_incompatibility(&base, &different_res)
            .unwrap()
            .contains("分辨率"));
        let different_codec = crate::probe::MediaInfo {
            video_codec: Some("hevc".into()),
            ..base.clone()
        };
        assert!(merge_incompatibility(&base, &different_codec)
            .unwrap()
            .contains("编码"));
        let no_audio = crate::probe::MediaInfo {
            audio_codec: None,
            channels: None,
            sample_rate: None,
            ..base.clone()
        };
        assert!(merge_incompatibility(&base, &no_audio)
            .unwrap()
            .contains("音轨"));
    }

    #[test]
    fn concat_list_uses_forward_slashes_and_escapes_quotes() {
        let list = concat_list(&[
            "D:\\videos\\a.mp4".to_string(),
            "C:\\it's\\b.mp4".to_string(),
        ]);
        assert_eq!(list, "file 'D:/videos/a.mp4'\nfile 'C:/it'\\''s/b.mp4'\n");
    }

    #[test]
    fn image_crop_order_is_crop_rotate_scale() {
        let p = ImageParams {
            format: "png".into(),
            quality: 90,
            max_edge: 200,
            rotation: "90".into(),
            auto_orient: false,
            crop_mode: "custom".into(),
            crop_x: 10,
            crop_y: 20,
            crop_w: 300,
            crop_h: 200,
            ..Default::default()
        };
        let args = build_image("in.png", "out.png", &p).unwrap();
        let vf = filter_arg(&args).expect("应生成 -vf");
        assert_eq!(
            vf,
            "crop=300:200:10:20,transpose=1,scale=w=min(iw\\,200):h=min(ih\\,200):force_original_aspect_ratio=decrease:flags=lanczos"
        );
    }

    #[test]
    fn image_crop_validation_rejects_bad_input() {
        let mut p = ImageParams {
            crop_mode: "custom".into(),
            ..Default::default()
        };
        assert!(validate_image_crop(&p).unwrap_err().contains("大于 0"));
        p.crop_w = 100;
        p.crop_h = 100;
        p.crop_x = -1;
        assert!(validate_image_crop(&p).unwrap_err().contains("负数"));

        let ratio = ImageParams {
            crop_mode: "ratio".into(),
            crop_ratio: "5:4".into(),
            ..Default::default()
        };
        assert!(validate_image_crop(&ratio)
            .unwrap_err()
            .contains("不支持的裁剪比例"));
        let mode = ImageParams {
            crop_mode: "magic".into(),
            ..Default::default()
        };
        assert!(validate_image_crop(&mode)
            .unwrap_err()
            .contains("不支持的裁剪模式"));

        assert!(validate_custom_crop(0, 0, 640, 480, 640, 480).is_ok());
        assert!(validate_custom_crop(10, 0, 640, 480, 640, 480)
            .unwrap_err()
            .contains("越界"));
        assert!(validate_custom_crop(0, 0, 0, 480, 640, 480)
            .unwrap_err()
            .contains("大于 0"));
    }

    #[test]
    fn sequence_validation_detects_gaps_and_prefix_mismatch() {
        let (pattern, start, count) = validate_sequence(&[
            "/tmp/seq/frame_0001.png".into(),
            "/tmp/seq/frame_0002.png".into(),
            "/tmp/seq/frame_0003.png".into(),
        ])
        .unwrap();
        assert!(pattern.ends_with("frame_%04d.png"), "{pattern}");
        assert_eq!(start, 1);
        assert_eq!(count, 3);

        let gap = validate_sequence(&[
            "/tmp/seq/frame_0001.png".into(),
            "/tmp/seq/frame_0003.png".into(),
        ])
        .unwrap_err();
        assert!(gap.contains("缺少 frame_0002.png"), "{gap}");

        let mixed = validate_sequence(&[
            "/tmp/seq/frame_0001.png".into(),
            "/tmp/seq/other_0002.png".into(),
        ])
        .unwrap_err();
        assert!(mixed.contains("不匹配"), "{mixed}");

        let no_digits = validate_sequence(&["/tmp/seq/frame.png".into()]).unwrap_err();
        assert!(no_digits.contains("末尾必须带编号"), "{no_digits}");
    }

    #[test]
    fn sequence_size_policy_requires_equal_dims_for_source_mode() {
        let p = SeqParams {
            size_mode: "source".into(),
            fps: 10,
            ..Default::default()
        };
        assert!(seq_target_size(&p, &[(320, 240), (320, 240)]).is_ok());
        let err = seq_target_size(&p, &[(320, 240), (640, 480)]).unwrap_err();
        assert!(err.contains("要求所有图片同尺寸"), "{err}");

        let fixed = SeqParams {
            size_mode: "fixed".into(),
            width: 0,
            height: 0,
            ..Default::default()
        };
        assert!(seq_target_size(&fixed, &[(320, 240)])
            .unwrap_err()
            .contains("大于 0"));
        let bad = SeqParams {
            size_mode: "weird".into(),
            ..Default::default()
        };
        assert!(seq_target_size(&bad, &[(320, 240)])
            .unwrap_err()
            .contains("不支持"));
    }

    #[test]
    fn seq_video_build_sets_framerate_and_frame_limit() {
        let p = SeqParams {
            fps: 12,
            start_number: 7,
            size_mode: "fixed".into(),
            width: 320,
            height: 240,
            encoder: "libx264".into(),
            quality: 20,
            bitrate_kbps: 0,
        };
        let args = build_seq_video("dir/frame_%04d.png", "out.mp4", &p, Some((320, 240)), 7);
        assert_eq!(
            args[args.iter().position(|a| a == "-framerate").unwrap() + 1],
            "12"
        );
        assert_eq!(
            args[args.iter().position(|a| a == "-start_number").unwrap() + 1],
            "7"
        );
        assert_eq!(
            args[args.iter().position(|a| a == "-frames:v").unwrap() + 1],
            "7"
        );
        let vf = args[args.iter().position(|a| a == "-vf").unwrap() + 1].clone();
        assert!(
            vf.contains("scale=320:240") && vf.contains("pad=320:240"),
            "{vf}"
        );
    }

    #[test]
    fn frames_build_sets_fps_and_output_pattern() {
        let p = FramesParams {
            fps: 5,
            width: 320,
            format: "png".into(),
            quality: 90,
        };
        let args = build_video_frames("in.mp4", "dir/frame-%04d.png", &p);
        let vf = args[args.iter().position(|a| a == "-vf").unwrap() + 1].clone();
        assert_eq!(vf, "fps=5,scale=320:-2:flags=lanczos");
        assert_eq!(args.last().unwrap(), "dir/frame-%04d.png");
    }

    #[test]
    fn collage_rejects_invalid_layout_and_color() {
        let p = CollageParams {
            cols: 2,
            rows: 1,
            cell_width: 100,
            cell_height: 100,
            gap: 0,
            background: "#FFFFFF".into(),
            fit: "contain".into(),
            format: "png".into(),
            quality: 90,
        };
        let err = build_collage(
            &["a.png".into(), "b.png".into(), "c.png".into()],
            "out.png",
            &p,
        )
        .unwrap_err();
        assert!(err.contains("放不进"), "{err}");

        let bad_color = CollageParams {
            background: "white".into(),
            ..p.clone()
        };
        assert!(build_collage(&["a.png".into()], "out.png", &bad_color)
            .unwrap_err()
            .contains("RRGGBB"));

        let args = build_collage(&["a.png".into(), "b.png".into()], "out.png", &p).unwrap();
        let graph = args[args.iter().position(|a| a == "-filter_complex").unwrap() + 1].clone();
        assert!(
            graph.contains("xstack=inputs=2:layout=0_0|100_0"),
            "{graph}"
        );
        assert_eq!(collage_canvas(2, 2, 100, 80, 10), (210, 170));
    }

    fn complex_arg(args: &[String]) -> Option<String> {
        let pos = args.iter().position(|a| a == "-filter_complex")?;
        args.get(pos + 1).cloned()
    }

    /// 640x480 画面 + 96x96 水印 + 24 间距：九宫格坐标（左上角原点）。
    #[test]
    fn watermark_positions_cover_nine_grid() {
        let base = (640, 480);
        let wm = (96, 96);
        let cases = [
            ("nw", (24, 24)),
            ("n", (272, 24)),
            ("ne", (520, 24)),
            ("w", (24, 192)),
            ("c", (272, 192)),
            ("e", (520, 192)),
            ("sw", (24, 360)),
            ("s", (272, 360)),
            ("se", (520, 360)),
        ];
        for (position, expected) in cases {
            assert_eq!(
                watermark_position(position, 24, base.0, base.1, wm.0, wm.1, 0, 0),
                expected,
                "{position} 位置错误"
            );
        }
        // 空字符串等价 se（默认值）
        assert_eq!(
            watermark_position("", 24, base.0, base.1, wm.0, wm.1, 0, 0),
            (520, 360)
        );
        // 画面比「水印 + 间距」还小时，留白被收紧到可用空间的一半，不会出现负坐标
        assert_eq!(watermark_position("se", 24, 100, 100, 96, 96, 0, 0), (2, 2));
        assert_eq!(
            watermark_position("nw", 200, 100, 100, 96, 96, 0, 0),
            (2, 2)
        );
    }

    #[test]
    fn watermark_custom_coords_are_clamped() {
        let base = (640, 480);
        let wm = (96, 96);
        assert_eq!(
            watermark_position("custom", 24, base.0, base.1, wm.0, wm.1, 100, 50),
            (100, 50)
        );
        // 越界 / 负数都被收紧到 [0, base - wm]
        assert_eq!(
            watermark_position("custom", 24, base.0, base.1, wm.0, wm.1, 900, 900),
            (544, 384)
        );
        assert_eq!(
            watermark_position("custom", 24, base.0, base.1, wm.0, wm.1, -50, -50),
            (0, 0)
        );
    }

    #[test]
    fn watermark_scale_is_relative_to_base_short_edge() {
        // 640x480 短边 480，20% → 96（原型实测）
        assert_eq!(watermark_target_size(640, 480, 512, 512, 20), (96, 96));
        // 默认 15% → 72
        assert_eq!(watermark_target_size(640, 480, 512, 512, 15), (72, 72));
        // 非等比水印保持宽高比（宽 2:1）
        let (w, h) = watermark_target_size(640, 480, 800, 400, 20);
        assert_eq!(h, 96);
        assert_eq!(w, 192);
        // 水印原图比目标还小时仍按「短边百分比」缩小，不会占满画面
        assert_eq!(watermark_target_size(100, 100, 400, 400, 50), (50, 50));
        // 非法百分比回落到默认 15%
        assert_eq!(watermark_target_size(640, 480, 512, 512, 0), (72, 72));
    }

    #[test]
    fn watermark_rotation_dims_swap_at_right_angle() {
        assert_eq!(watermark_rotated_dims(96, 96, 0.0), (96, 96));
        assert_eq!(watermark_rotated_dims(96, 96, 90.0), (96, 96));
        // 45° 的正方形外接矩形约为 136x136
        let (w, h) = watermark_rotated_dims(96, 96, 45.0);
        assert_eq!(w, h);
        assert!((135..=137).contains(&w), "{w}");
        // 非正方形 90° 交换宽高
        assert_eq!(watermark_rotated_dims(120, 60, 90.0), (60, 120));
    }

    #[test]
    fn watermark_video_uses_single_filter_complex_and_shortest() {
        let mut p = video_params();
        p.speed = 1.5;
        p.audio_mode = "aac".into();
        p.watermark = WatermarkParams {
            enabled: true,
            path: "wm.png".into(),
            scale_percent: 20,
            base_width: 640,
            base_height: 480,
            wm_width: 512,
            wm_height: 512,
            ..Default::default()
        };
        let args = build_video("libx264", "in.mp4", "out.mp4", &p, "aac", None).unwrap();
        assert_eq!(count_flag(&args, "-vf"), 0, "水印必须走 filter_complex");
        assert_eq!(count_flag(&args, "-filter_complex"), 1);
        let graph = complex_arg(&args).expect("应生成 -filter_complex");
        assert_eq!(graph.matches("setpts=").count(), 1, "{graph}");
        assert!(graph.contains("[0:v]setpts=PTS/1.5[base]"), "{graph}");
        assert!(
            graph.contains(
                "[1:v]scale=96:96:flags=lanczos,format=rgba,colorchannelmixer=aa=0.9[wm]"
            ),
            "{graph}"
        );
        assert!(
            graph.contains("[base][wm]overlay=520:360:format=auto:shortest=1[outv]"),
            "{graph}"
        );
        assert_eq!(
            count_flag(&args, "-shortest"),
            1,
            "静态水印必须加 -shortest"
        );
        // 水印输入以 -loop 1 循环读取，并映射到 [outv]
        let map_pos = args
            .iter()
            .position(|a| a == "[outv]")
            .expect("应映射 [outv]");
        assert_eq!(args[map_pos - 1], "-map");
        assert!(args.contains(&"wm.png".to_string()));
    }

    #[test]
    fn watermark_video_appends_rotation_filter() {
        let mut p = video_params();
        p.watermark = WatermarkParams {
            enabled: true,
            path: "wm.png".into(),
            rotation: 30.0,
            opacity: 0.5,
            position: "custom".into(),
            x: 10,
            y: 20,
            base_width: 640,
            base_height: 480,
            wm_width: 96,
            wm_height: 96,
            ..Default::default()
        };
        let args = build_video("libx264", "in.mp4", "out.mp4", &p, "aac", None).unwrap();
        let graph = complex_arg(&args).expect("应生成 -filter_complex");
        assert!(graph.contains("colorchannelmixer=aa=0.5"), "{graph}");
        assert!(
            graph.contains("rotate=0.5236:ow=rotw(0.5236):oh=roth(0.5236):c=none"),
            "{graph}"
        );
        assert!(
            graph.contains("[base][wm]overlay=10:20:format=auto:shortest=1"),
            "{graph}"
        );
        // 旋转 0° 时不插入 rotate
        let mut plain = p.clone();
        plain.watermark.rotation = 0.0;
        let graph_plain =
            complex_arg(&build_video("libx264", "in.mp4", "out.mp4", &plain, "aac", None).unwrap())
                .expect("应生成 -filter_complex");
        assert!(!graph_plain.contains("rotate="), "{graph_plain}");
    }

    #[test]
    fn watermark_video_reports_missing_dims() {
        let mut p = video_params();
        p.watermark = WatermarkParams {
            enabled: true,
            path: "wm.png".into(),
            ..Default::default()
        };
        let err = build_video("libx264", "in.mp4", "out.mp4", &p, "aac", None).unwrap_err();
        assert!(err.contains("无法读取输入画面尺寸"), "{err}");

        p.watermark.base_width = 640;
        p.watermark.base_height = 480;
        let err = build_video("libx264", "in.mp4", "out.mp4", &p, "aac", None).unwrap_err();
        assert!(err.contains("无法读取水印图片尺寸"), "{err}");
    }

    #[test]
    fn watermark_image_build_sets_pix_fmt_per_format() {
        for (format, expect) in [("jpg", "yuvj420p"), ("webp", "yuv420p"), ("png", "rgb24")] {
            let p = ImageParams {
                format: format.into(),
                quality: 90,
                watermark: WatermarkParams {
                    enabled: true,
                    path: "wm.png".into(),
                    scale_percent: 20,
                    base_width: 640,
                    base_height: 480,
                    wm_width: 512,
                    wm_height: 512,
                    ..Default::default()
                },
                ..Default::default()
            };
            let args = build_image("in.png", "out.img", &p).unwrap();
            assert_eq!(count_flag(&args, "-vf"), 0, "{format} 应走 filter_complex");
            let graph = complex_arg(&args).expect("应生成 -filter_complex");
            assert!(graph.starts_with("[0:v]null[base];"), "{graph}");
            assert!(
                graph.contains("[base][wm]overlay=520:360:format=auto[out]"),
                "{graph}"
            );
            assert_eq!(args.last().unwrap(), "out.img");
            let pix_pos = args
                .iter()
                .position(|a| a == "-pix_fmt")
                .unwrap_or_else(|| panic!("{format} 缺少 -pix_fmt"));
            assert_eq!(args[pix_pos + 1], expect, "{format} 的 pix_fmt 错误");
            assert_eq!(count_flag(&args, "-shortest"), 0, "图片不需要 -shortest");
        }
    }

    #[test]
    fn watermark_validation_rejects_bad_input() {
        let dir = std::env::temp_dir().join(format!("ff-wm-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let png = dir.join("ok.png");
        std::fs::write(&png, b"png").unwrap();
        let gif = dir.join("bad.gif");
        std::fs::write(&gif, b"gif").unwrap();

        // 关闭时一律放行
        assert!(validate_watermark(&WatermarkParams::default()).is_ok());

        let mut w = WatermarkParams {
            enabled: true,
            path: String::new(),
            ..Default::default()
        };
        let err = validate_watermark(&w).unwrap_err();
        assert!(err.contains("水印图片不存在"), "{err}");
        assert!(err.contains("请重新选择水印图片"), "{err}");

        w.path = png.to_string_lossy().to_string();
        assert!(validate_watermark(&w).is_ok());

        w.path = gif.to_string_lossy().to_string();
        let err = validate_watermark(&w).unwrap_err();
        assert!(err.contains("不支持的水印图片格式"), "{err}");

        w.path = png.to_string_lossy().to_string();
        w.opacity = 1.5;
        assert!(validate_watermark(&w).unwrap_err().contains("不透明度"));
        w.opacity = 0.9;
        w.scale_percent = 4;
        assert!(validate_watermark(&w).unwrap_err().contains("比例"));
        w.scale_percent = 51;
        assert!(validate_watermark(&w).unwrap_err().contains("比例"));
        w.scale_percent = 15;
        w.rotation = 181.0;
        assert!(validate_watermark(&w).unwrap_err().contains("旋转"));
        w.rotation = -181.0;
        assert!(validate_watermark(&w).unwrap_err().contains("旋转"));
        w.rotation = 0.0;
        w.position = "magic".into();
        assert!(validate_watermark(&w).unwrap_err().contains("位置"));
        for position in [
            "", "nw", "n", "ne", "w", "c", "e", "sw", "s", "se", "custom",
        ] {
            w.position = position.into();
            assert!(validate_watermark(&w).is_ok(), "{position} 应被接受");
        }

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn watermark_defaults_match_ui_spec() {
        let w = WatermarkParams::default();
        assert!(!w.enabled);
        assert_eq!(w.position, "se");
        assert!((w.opacity - 0.9).abs() < 1e-9);
        assert_eq!(w.scale_percent, 15);
        assert_eq!(w.margin, 24);
        assert_eq!(w.rotation, 0.0);
        // 前端不传尺寸，反序列化后保持 0（由后端回填）
        let parsed: WatermarkParams = serde_json::from_str("{\"enabled\":true}").unwrap();
        assert_eq!(parsed.base_width, 0);
        assert_eq!(parsed.scale_percent, 15);
    }

    #[test]
    fn watermark_base_dims_follow_filter_chain() {
        let mut p = video_params();
        p.crop = "1:1".into();
        assert_eq!(video_base_dims(&p, 1280, 720), (720, 720));
        p.rotation = "90".into();
        assert_eq!(video_base_dims(&p, 1280, 720), (720, 720));
        p.resolution = "480".into();
        assert_eq!(video_base_dims(&p, 1280, 720), (480, 480));

        let mut img = ImageParams {
            format: "png".into(),
            max_edge: 200,
            crop_mode: "custom".into(),
            crop_x: 10,
            crop_y: 20,
            crop_w: 300,
            crop_h: 200,
            ..Default::default()
        };
        assert_eq!(image_base_dims(&img, "in.png", 640, 480), (200, 133));
        img.rotation = "90".into();
        assert_eq!(image_base_dims(&img, "in.png", 640, 480), (133, 200));
    }

    fn temp_subtitle(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("ff-sub-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join(name);
        std::fs::write(
            &path,
            "1\n00:00:00,000 --> 00:00:02,000\nhello\n\n2\n00:00:02,000 --> 00:00:04,000\nworld\n",
        )
        .unwrap();
        path
    }

    fn map_targets(args: &[String]) -> Vec<String> {
        args.iter()
            .enumerate()
            .filter(|(_, a)| a.as_str() == "-map")
            .filter_map(|(i, _)| args.get(i + 1).cloned())
            .collect()
    }

    fn flag_value(args: &[String], flag: &str) -> Option<String> {
        let pos = args.iter().position(|a| a.as_str() == flag)?;
        args.get(pos + 1).cloned()
    }

    #[test]
    fn subtitle_soft_mp4_maps_mov_text_and_language() {
        let file = temp_subtitle("soft.mp4.srt");
        let mut p = video_params();
        p.audio_mode = "aac".into();
        p.subtitle = SubtitleParams {
            enabled: true,
            mode: "soft".into(),
            path: file.to_string_lossy().to_string(),
            language: "chi".into(),
            default_track: true,
            ..Default::default()
        };
        let args = build_video("libx264", "in.mp4", "out.mp4", &p, "aac", None).unwrap();
        let maps = map_targets(&args);
        assert!(maps.contains(&"0:v:0".to_string()), "{maps:?}");
        assert!(maps.contains(&"0:a:0?".to_string()), "{maps:?}");
        assert!(maps.contains(&"1:s:0".to_string()), "{maps:?}");
        assert_eq!(flag_value(&args, "-c:s").as_deref(), Some("mov_text"));
        assert!(args.contains(&"language=chi".to_string()));
        assert_eq!(
            count_flag(&args, "-disposition:s:0"),
            1,
            "主字幕应设 default disposition"
        );
        assert_eq!(
            flag_value(&args, "-disposition:s:0").as_deref(),
            Some("default")
        );
    }

    #[test]
    fn subtitle_soft_mkv_copies_stream() {
        let file = temp_subtitle("soft.mkv.srt");
        let mut p = video_params();
        p.container = "mkv".into();
        p.subtitle = SubtitleParams {
            enabled: true,
            mode: "soft".into(),
            path: file.to_string_lossy().to_string(),
            ..Default::default()
        };
        let args = build_video("libx265", "in.mp4", "out.mkv", &p, "aac", None).unwrap();
        assert_eq!(flag_value(&args, "-c:s").as_deref(), Some("copy"));
        assert!(map_targets(&args).contains(&"1:s:0".to_string()));
    }

    #[test]
    fn subtitle_soft_multiple_files_map_and_metadata_each() {
        let main = temp_subtitle("multi-1.srt");
        let second = temp_subtitle("multi-2.srt");
        let third = temp_subtitle("multi-3.srt");
        let mut p = video_params();
        p.audio_mode = "aac".into();
        p.subtitle = SubtitleParams {
            enabled: true,
            mode: "soft".into(),
            path: main.to_string_lossy().to_string(),
            extra_paths: vec![
                second.to_string_lossy().to_string(),
                third.to_string_lossy().to_string(),
            ],
            language: "chi".into(),
            default_track: true,
            ..Default::default()
        };
        let args = build_video("libx264", "in.mp4", "out.mp4", &p, "aac", None).unwrap();
        let maps = map_targets(&args);
        assert_eq!(
            maps.iter().filter(|m| m.contains(":s:0")).count(),
            3,
            "{maps:?}"
        );
        assert!(maps.contains(&"1:s:0".to_string()), "{maps:?}");
        assert!(maps.contains(&"2:s:0".to_string()), "{maps:?}");
        assert!(maps.contains(&"3:s:0".to_string()), "{maps:?}");
        let metadata_flags = args
            .iter()
            .filter(|a| a.starts_with("-metadata:s:s:"))
            .count();
        assert_eq!(metadata_flags, 3, "每条字幕轨都要写 language");
        assert_eq!(
            count_flag(&args, "-disposition:s:0"),
            1,
            "只有主字幕轨设默认"
        );
    }

    #[test]
    fn subtitle_container_matrix_accepts_only_mp4_mov_mkv() {
        let file = temp_subtitle("matrix.srt");
        let s = SubtitleParams {
            enabled: true,
            mode: "soft".into(),
            path: file.to_string_lossy().to_string(),
            ..Default::default()
        };
        for container in ["mp4", "mov", "mkv"] {
            assert!(
                validate_subtitle(&s, container, 1.0).is_ok(),
                "{container} 应支持内嵌字幕"
            );
        }
        for container in ["webm", "avi", "ts"] {
            let err = validate_subtitle(&s, container, 1.0).unwrap_err();
            assert!(err.contains("不承载内嵌字幕轨"), "{container}: {err}");
        }
    }

    #[test]
    fn subtitle_hard_filter_string_has_font_alignment_and_color() {
        let mut p = video_params();
        p.subtitle = SubtitleParams {
            enabled: true,
            mode: "hard".into(),
            path: "sub.srt".into(),
            font_size_pct: 4.5,
            outline: 2,
            position: "bottom".into(),
            color: "#FF0000".into(),
            margin_v: 24,
            base_height: 1080,
            ..Default::default()
        };
        let args = build_video("libx264", "in.mp4", "out.mp4", &p, "aac", None).unwrap();
        let vf = filter_arg(&args).expect("hard 应生成 -vf");
        assert!(vf.contains("subtitles="), "{vf}");
        assert!(vf.contains("force_style="), "{vf}");
        assert!(vf.contains("Fontsize=49"), "1080 × 4.5% ≈ 49，实际 {vf}");
        assert!(vf.contains("Outline=2"), "{vf}");
        assert!(vf.contains("Alignment=2"), "{vf}");
        assert!(
            vf.contains("PrimaryColour=&H0000FF"),
            "#FF0000 → &H0000FF，实际 {vf}"
        );
        assert!(vf.contains("MarginV=24"), "{vf}");

        assert_eq!(subtitle_alignment("bottom"), 2);
        assert_eq!(subtitle_alignment(""), 2);
        assert_eq!(subtitle_alignment("middle"), 5);
        assert_eq!(subtitle_alignment("top"), 8);
        assert_eq!(subtitle_ass_color("#123456").unwrap(), "&H563412");
        assert_eq!(subtitle_font_px(1080, 4.5), 49);
        assert_eq!(subtitle_font_px(240, 4.5), 11);
        assert_eq!(subtitle_font_px(100, 1.0), 8, "最小 8 像素");
    }

    #[test]
    fn subtitle_hard_filter_sits_after_scale_and_before_watermark() {
        let mut p = video_params();
        p.resolution = "1080".into();
        p.watermark = WatermarkParams {
            enabled: true,
            path: "wm.png".into(),
            base_width: 1920,
            base_height: 1080,
            wm_width: 96,
            wm_height: 96,
            ..Default::default()
        };
        p.subtitle = SubtitleParams {
            enabled: true,
            mode: "hard".into(),
            path: "sub.srt".into(),
            base_height: 1080,
            ..Default::default()
        };
        let args = build_video("libx264", "in.mp4", "out.mp4", &p, "aac", None).unwrap();
        let graph = complex_arg(&args).expect("水印 + 烧录应生成 -filter_complex");
        let scale = graph.find("scale=-2:1080").expect("应含 scale");
        let subtitles = graph.find("subtitles=").expect("应含 subtitles");
        let overlay = graph.find("overlay=").expect("应含 overlay");
        assert!(
            scale < subtitles && subtitles < overlay,
            "链顺序必须是 scale → subtitles → overlay：{graph}"
        );
        assert_eq!(graph.matches("subtitles=").count(), 1, "{graph}");
    }

    #[test]
    fn subtitle_hard_rejects_speed_change_but_soft_allows_it() {
        let file = temp_subtitle("speed.srt");
        let mut s = SubtitleParams {
            enabled: true,
            mode: "hard".into(),
            path: file.to_string_lossy().to_string(),
            ..Default::default()
        };
        let err = validate_subtitle(&s, "mp4", 1.5).unwrap_err();
        assert_eq!(
            err,
            "变速时不能烧录字幕（字幕时间轴会与变速后的画面/音频不同步）：请改为内嵌字幕，或把倍速设为 1"
        );
        assert!(validate_subtitle(&s, "mp4", 1.0).is_ok());

        s.mode = "soft".into();
        assert!(validate_subtitle(&s, "mp4", 1.5).is_ok());
        let mut p = video_params();
        p.speed = 1.5;
        p.audio_mode = "aac".into();
        p.subtitle = s.clone();
        let args = build_video("libx264", "in.mp4", "out.mp4", &p, "aac", None).unwrap();
        let vf = filter_arg(&args).expect("变速应生成 -vf");
        assert_eq!(vf.matches("setpts=").count(), 1, "{vf}");
        assert!(map_targets(&args).contains(&"1:s:0".to_string()));
    }

    #[test]
    fn subtitle_hard_without_libass_is_rejected() {
        let s = SubtitleParams {
            enabled: true,
            mode: "hard".into(),
            path: "sub.srt".into(),
            ..Default::default()
        };
        let err = validate_subtitle_burn(&s, false).unwrap_err();
        assert!(err.contains("libass"), "{err}");
        assert!(err.contains("内嵌字幕"), "{err}");
        assert!(validate_subtitle_burn(&s, true).is_ok());
        let soft = SubtitleParams {
            enabled: true,
            mode: "soft".into(),
            ..Default::default()
        };
        assert!(
            validate_subtitle_burn(&soft, false).is_ok(),
            "软字幕不依赖 libass"
        );
    }

    #[test]
    fn subtitle_validation_rejects_missing_files_and_bad_extension() {
        let missing = SubtitleParams {
            enabled: true,
            mode: "soft".into(),
            path: "not-there.srt".into(),
            ..Default::default()
        };
        let err = validate_subtitle(&missing, "mp4", 1.0).unwrap_err();
        assert!(err.contains("字幕文件不存在"), "{err}");
        assert!(err.contains("请重新选择字幕文件"), "{err}");

        let bad = temp_subtitle("bad.txt");
        let s = SubtitleParams {
            enabled: true,
            mode: "soft".into(),
            path: bad.to_string_lossy().to_string(),
            ..Default::default()
        };
        assert!(validate_subtitle(&s, "mp4", 1.0)
            .unwrap_err()
            .contains("不支持的字幕格式"));

        let main = temp_subtitle("extra-main.srt");
        let s = SubtitleParams {
            enabled: true,
            mode: "soft".into(),
            path: main.to_string_lossy().to_string(),
            extra_paths: vec!["missing-extra.srt".into()],
            ..Default::default()
        };
        assert!(validate_subtitle(&s, "mp4", 1.0)
            .unwrap_err()
            .contains("字幕文件不存在"));
    }

    // ===== 第九轮：音量增益 / 视频分割 / 图片 GIF =====

    #[test]
    fn volume_filter_is_first_and_single_af() {
        let mut p = video_params();
        p.audio_mode = "aac".into();
        p.speed = 1.5;
        p.volume = VolumeParams {
            enabled: true,
            gain_db: 6.0,
            mute: false,
        };
        let args = build_video("libx264", "in.mp4", "out.mp4", &p, "aac", None).unwrap();
        assert_eq!(count_flag(&args, "-af"), 1, "只允许一个 -af");
        assert_eq!(
            audio_filter_arg(&args).as_deref(),
            Some("volume=6dB,atempo=1.5"),
            "音量必须在变速之前"
        );
    }

    #[test]
    fn volume_mute_emits_an_without_af_or_audio_map() {
        let mut p = video_params();
        p.audio_mode = "aac".into();
        p.volume = VolumeParams {
            enabled: true,
            gain_db: 6.0,
            mute: true,
        };
        let args = build_video("libx264", "in.mp4", "out.mp4", &p, "aac", None).unwrap();
        assert_eq!(count_flag(&args, "-an"), 1, "静音必须生成 -an");
        assert_eq!(count_flag(&args, "-af"), 0, "静音不应产生音频滤镜");
        assert!(
            !args.contains(&"0:a:0?".to_string()),
            "静音时不应再映射源音轨: {args:?}"
        );
    }

    #[test]
    fn volume_watermark_hard_subtitle_keep_single_complex_and_af() {
        let mut p = video_params();
        p.audio_mode = "aac".into();
        p.speed = 1.0;
        p.volume = VolumeParams {
            enabled: true,
            gain_db: 6.0,
            mute: false,
        };
        p.watermark = WatermarkParams {
            enabled: true,
            path: "wm.png".into(),
            base_width: 640,
            base_height: 480,
            wm_width: 96,
            wm_height: 96,
            ..Default::default()
        };
        p.subtitle = SubtitleParams {
            enabled: true,
            mode: "hard".into(),
            path: "sub.srt".into(),
            base_height: 480,
            ..Default::default()
        };
        let args = build_video("libx264", "in.mp4", "out.mp4", &p, "aac", None).unwrap();
        assert_eq!(count_flag(&args, "-filter_complex"), 1);
        assert_eq!(count_flag(&args, "-af"), 1);
        assert_eq!(
            audio_filter_arg(&args).as_deref(),
            Some("volume=6dB"),
            "无变速时音量链只有 volume"
        );
        let graph = complex_arg(&args).unwrap();
        assert!(graph.contains("subtitles="), "{graph}");
        assert!(
            graph.contains("[base][wm]overlay=") || graph.contains("overlay="),
            "{graph}"
        );
    }

    #[test]
    fn audio_normalize_conflicts_with_volume_gain() {
        let mut p = AudioParams {
            format: "mp3".into(),
            bitrate_kbps: 192,
            sample_rate: "source".into(),
            channels: "source".into(),
            normalize: true,
            speed: 1.0,
            keep_pitch: true,
            volume: VolumeParams {
                enabled: true,
                gain_db: 6.0,
                mute: false,
            },
        };
        let err = validate_audio_volume(&p).unwrap_err();
        assert!(err.contains("音量增益与响度归一化不能同时开启"), "{err}");
        assert!(err.contains("-16 LUFS"), "{err}");
        p.volume.enabled = false;
        assert!(validate_audio_volume(&p).is_ok());
    }

    #[test]
    fn audio_volume_precedes_speed_and_is_single_af() {
        let p = AudioParams {
            format: "mp3".into(),
            bitrate_kbps: 192,
            sample_rate: "source".into(),
            channels: "source".into(),
            normalize: false,
            speed: 1.5,
            keep_pitch: true,
            volume: VolumeParams {
                enabled: true,
                gain_db: 6.0,
                mute: false,
            },
        };
        let args = build_audio("libmp3lame", "in.wav", "out.mp3", &p);
        assert_eq!(count_flag(&args, "-af"), 1);
        assert_eq!(
            audio_filter_arg(&args).as_deref(),
            Some("volume=6dB,atempo=1.5")
        );
    }

    #[test]
    fn validate_volume_accepts_bounds_and_rejects_out_of_range() {
        let mut p = VolumeParams {
            enabled: true,
            gain_db: -20.0,
            mute: false,
        };
        assert!(validate_volume(&p).is_ok());
        p.gain_db = 20.0;
        assert!(validate_volume(&p).is_ok());
        p.gain_db = -21.0;
        assert!(validate_volume(&p).unwrap_err().contains("超出支持范围"));
        p.gain_db = 20.1;
        assert!(validate_volume(&p).unwrap_err().contains("超出支持范围"));
        p.gain_db = f64::NAN;
        assert!(validate_volume(&p).unwrap_err().contains("有限数值"));
        // 静音时忽略增益
        p.gain_db = 999.0;
        p.mute = true;
        assert!(validate_volume(&p).is_ok());
    }

    #[test]
    fn split_copy_uses_timestamps_stream_copy_and_muxer() {
        let p = SplitParams {
            mode: "copy".into(),
            container: "mkv".into(),
            segment_index: 0,
            start_sec: 0.0,
            end_sec: 30.0,
        };
        let args = build_split("in.mp4", "out.mkv", &p, "libx264");
        assert!(
            args.windows(2).any(|w| w[0] == "-ss" && w[1] == "0"),
            "{args:?}"
        );
        assert!(
            args.windows(2).any(|w| w[0] == "-to" && w[1] == "30"),
            "{args:?}"
        );
        assert!(
            args.windows(2).any(|w| w[0] == "-c" && w[1] == "copy"),
            "{args:?}"
        );
        assert!(
            args.windows(2)
                .any(|w| w[0] == "-avoid_negative_ts" && w[1] == "make_zero"),
            "{args:?}"
        );
        assert!(
            args.windows(2).any(|w| w[0] == "-f" && w[1] == "matroska"),
            "mkv 的 muxer 应为 matroska: {args:?}"
        );
        assert!(!args.iter().any(|a| a == "-c:v"), "copy 模式不应重编码");
    }

    #[test]
    fn split_encode_uses_encoder_and_faststart_for_mp4() {
        let p = SplitParams {
            mode: "encode".into(),
            container: "mp4".into(),
            segment_index: 1,
            start_sec: 2.0,
            end_sec: 4.0,
        };
        let args = build_split("in.mp4", "out.mp4", &p, "libopenh264");
        assert!(args
            .windows(2)
            .any(|w| w[0] == "-c:v" && w[1] == "libopenh264"));
        assert!(args.windows(2).any(|w| w[0] == "-crf" && w[1] == "20"));
        assert!(args
            .windows(2)
            .any(|w| w[0] == "-pix_fmt" && w[1] == "yuv420p"));
        assert!(args
            .windows(2)
            .any(|w| w[0] == "-movflags" && w[1] == "+faststart"));
        assert!(args.windows(2).any(|w| w[0] == "-c:a" && w[1] == "aac"));
    }

    #[test]
    fn validate_split_rejects_bad_ranges_modes_and_containers() {
        let mut p = SplitParams::default();
        p.start_sec = -1.0;
        p.end_sec = 1.0;
        assert!(validate_split(&p, 10.0).unwrap_err().contains("开始时间"));
        p.start_sec = 5.0;
        p.end_sec = 5.0;
        assert!(validate_split(&p, 10.0)
            .unwrap_err()
            .contains("大于开始时间"));
        p.end_sec = 10.1;
        assert!(validate_split(&p, 10.0)
            .unwrap_err()
            .contains("超出源视频时长"));
        p.mode = "whatever".into();
        p.start_sec = 0.0;
        p.end_sec = 3.0;
        assert!(validate_split(&p, 10.0).unwrap_err().contains("分割模式"));
        p.mode = "copy".into();
        p.container = "flv".into();
        assert!(validate_split(&p, 10.0)
            .unwrap_err()
            .contains("分割输出容器"));
        // +0.05s 容差内通过
        p.container = "mp4".into();
        p.start_sec = 0.0;
        p.end_sec = 10.05;
        assert!(validate_split(&p, 10.0).is_ok());
        assert!(validate_split(&p, f64::INFINITY).is_ok());
    }

    #[test]
    fn gif_two_passes_use_palettegen_and_paletteuse() {
        let p = ImageGifParams {
            fps: 10,
            loop_count: 0,
            max_width: 0,
            dither: true,
        };
        let pass1 = build_gif_pass1("list.txt", "palette.png", &p, 320);
        assert!(
            pass1
                .iter()
                .any(|a| a.contains("palettegen=stats_mode=diff")),
            "{pass1:?}"
        );
        assert!(
            pass1
                .iter()
                .any(|a| a.contains("fps=10,scale=320:-2:flags=lanczos")),
            "{pass1:?}"
        );
        assert!(!pass1.iter().any(|a| a.contains("paletteuse")));

        let pass2 = build_gif_pass2("list.txt", "palette.png", "out.gif", &p, 320);
        assert!(
            pass2
                .iter()
                .any(|a| a.contains("paletteuse=dither=sierra2_4a")),
            "{pass2:?}"
        );
        assert!(pass2.windows(2).any(|w| w[0] == "-loop" && w[1] == "0"));
        assert!(pass2
            .windows(2)
            .any(|w| w[0] == "-i" && w[1] == "palette.png"));
        // concat 末行重复多出的一帧用 -frames:v 封顶，且必须插在输出文件之前。
        let mut capped = pass2;
        limit_gif_frames(&mut capped, 3);
        assert!(
            capped
                .windows(2)
                .any(|w| w[0] == "-frames:v" && w[1] == "3"),
            "{capped:?}"
        );
        assert_eq!(capped.last().map(String::as_str), Some("out.gif"));

        // 关闭 dither
        let p2 = ImageGifParams { dither: false, ..p };
        let pass2 = build_gif_pass2("list.txt", "palette.png", "out.gif", &p2, 320);
        assert!(
            pass2.iter().any(|a| a.contains("paletteuse=dither=none")),
            "{pass2:?}"
        );
    }

    #[test]
    fn gif_target_width_and_concat_list() {
        assert_eq!(gif_target_width(0, 480), 480);
        assert_eq!(gif_target_width(1280, 480), 1280);

        let list = gif_concat_list(&["C:\\dir\\a.png".into(), "C:\\dir\\b.png".into()], 10);
        let expected = "file 'C:/dir/a.png'\nduration 0.1\nfile 'C:/dir/b.png'\nduration 0.1\nfile 'C:/dir/b.png'\n";
        assert_eq!(list, expected);

        let quoted = gif_concat_list(&["/tmp/it's.png".into()], 5);
        assert!(quoted.contains("file '/tmp/it'\\''s.png'"), "{quoted}");
    }
}
