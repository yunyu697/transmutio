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
    let kbps = if p.bitrate_kbps > 0 { p.bitrate_kbps } else { 4200 };
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
            vec!["-global_quality".into(), quality.to_string(), "-preset".into(), "medium".into()]
        };
    }
    if encoder.ends_with("_vaapi") {
        return if bitrate_mode {
            vec!["-b:v".into(), format!("{kbps}k")]
        } else {
            vec!["-rc_mode".into(), "CQP".into(), "-qp".into(), quality.to_string()]
        };
    }
    if encoder.ends_with("_videotoolbox") {
        return if bitrate_mode {
            vec!["-b:v".into(), format!("{kbps}k"), "-allow_sw".into(), "0".into()]
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
            vec!["-b:v".into(), format!("{kbps}k"), "-preset".into(), "medium".into()]
        } else {
            vec!["-crf".into(), quality.to_string(), "-preset".into(), "medium".into()]
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
        "16:9" => (
            "trunc(min(iw,ih*16/9)/2)*2",
            "trunc(min(ih,iw*9/16)/2)*2",
        ),
        "1:1" => ("trunc(min(iw,ih)/2)*2", "trunc(min(iw,ih)/2)*2"),
        "9:16" => (
            "trunc(min(iw,ih*9/16)/2)*2",
            "trunc(min(ih,iw*16/9)/2)*2",
        ),
        "4:3" => (
            "trunc(min(iw,ih*4/3)/2)*2",
            "trunc(min(ih,iw*3/4)/2)*2",
        ),
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
            other => Err(format!("不支持的裁剪比例「{other}」，可选 1:1 / 16:9 / 9:16 / 4:3")),
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
        other => Err(format!("不支持的裁剪模式「{other}」，可选 none / ratio / custom")),
    }
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
            p.crop_w, p.crop_h, p.crop_x.max(0), p.crop_y.max(0)
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
        let number: u32 = tail
            .parse()
            .map_err(|_| format!("无法解析编号：{input}"))?;
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
pub fn seq_target_size(
    p: &SeqParams,
    dims: &[(u32, u32)],
) -> Result<Option<(u32, u32)>, String> {
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
            rate_mode: if p.bitrate_kbps > 0 { "bitrate".into() } else { "quality".into() },
            quality: if p.quality > 0 { p.quality } else { 23 },
            bitrate_kbps: p.bitrate_kbps,
            resolution: target.map(|(_, h)| h.to_string()).unwrap_or_else(|| "1080".into()),
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
pub fn build_collage(inputs: &[String], output: &str, p: &CollageParams) -> Result<Vec<String>, String> {
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
    let bg = normalize_color(if p.background.trim().is_empty() { "#FFFFFF" } else { &p.background })?;
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
                format!(
                    "{}_{}",
                    col * (cell_w + gap),
                    row * (cell_h + gap)
                )
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

pub fn build_video(
    encoder: &str,
    input: &str,
    output: &str,
    p: &VideoParams,
    container_audio: &str,
    hwaccel: Option<&str>,
) -> Vec<String> {
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
    args.push("-map".into());
    args.push("0:v:0".into());
    if p.audio_mode != "none" {
        args.push("-map".into());
        args.push("0:a:0?".into());
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
    if vaapi_ok {
        filters.push("format=nv12".into());
        filters.push("hwupload".into());
    }
    if !filters.is_empty() {
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
    args.push("-c:v".into());
    args.push(encoder.to_string());
    if !vaapi_encode {
        args.push("-pix_fmt".into());
        args.push("yuv420p".into());
    }
    let mut rate = rate_args(encoder, p);
    args.append(&mut rate);

    match p.audio_mode.as_str() {
        "copy" => {
            args.push("-c:a".into());
            args.push("copy".into());
        }
        "none" => {
            args.push("-an".into());
        }
        _ => {
            let kbps = if p.audio_bitrate_kbps > 0 { p.audio_bitrate_kbps } else { 192 };
            let audio = if container_audio.trim().is_empty() { "aac" } else { container_audio };
            args.push("-c:a".into());
            args.push(audio.into());
            args.push("-b:a".into());
            args.push(format!("{kbps}k"));
        }
    }
    let spec = crate::containers::find_container(if p.container.trim().is_empty() { "mp4" } else { &p.container });
    let muxer = spec.map(|c| c.muxer).unwrap_or("mp4");
    if p.faststart && spec.map(|c| c.faststart).unwrap_or(true) {
        args.push("-movflags".into());
        args.push("+faststart".into());
    }
    args.push("-f".into());
    args.push(muxer.into());
    args.push(output.into());
    args.shrink_to_fit();
    args
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
    let kbps = if p.bitrate_kbps > 0 { p.bitrate_kbps } else { 192 };
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
    if p.normalize {
        // EBU R128 单遍响度归一化；libebur128 缺失时 ffmpeg 会明确报错，任务不会被静默标成功。
        args.push("-af".into());
        args.push("loudnorm=I=-16:TP=-1.5:LRA=11".into());
    }
    args.push(output.into());
    args
}

pub fn build_image(input: &str, output: &str, p: &ImageParams) -> Vec<String> {
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
        "-map".into(),
        "0:v:0".into(),
    ];

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
    if !filters.is_empty() {
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
        }
        "webp" => {
            args.push("-c:v".into());
            args.push("libwebp".into());
            args.push("-quality".into());
            args.push(quality.to_string());
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
        }
    }
    args.push("-frames:v".into());
    args.push("1".into());
    args.push(output.into());
    args
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
        Some(if little { u16::from_le_bytes(raw) } else { u16::from_be_bytes(raw) })
    };
    let read_u32 = |at: usize| -> Option<u32> {
        let raw: [u8; 4] = tiff.get(at..at + 4)?.try_into().ok()?;
        Some(if little { u32::from_le_bytes(raw) } else { u32::from_be_bytes(raw) })
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
        return if (1..=8).contains(&value) { Some(value) } else { None };
    }
    None
}

/// 同参判定用于「合并时能否直接 stream copy」。任一字段不同都返回不兼容原因。
pub fn merge_incompatibility(
    first: &crate::probe::MediaInfo,
    next: &crate::probe::MediaInfo,
) -> Option<String> {
    if first.video_codec != next.video_codec {
        return Some(format!("编码不同（{:?} vs {:?}）", first.video_codec, next.video_codec));
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
            rate_mode: if p.bitrate_kbps > 0 { "bitrate".into() } else { "quality".into() },
            quality: if p.quality > 0 { p.quality } else { 23 },
            bitrate_kbps: p.bitrate_kbps,
            resolution: target_height.map(|h| h.to_string()).unwrap_or_else(|| "1080".into()),
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
        let kbps = if p.audio_bitrate_kbps > 0 { p.audio_bitrate_kbps } else { 192 };
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
        }
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
        let args = build_video("libx264", "in.mp4", "out.mp4", &p, "aac", None);
        let vf = filter_arg(&args).expect("应生成 -vf");
        assert!(vf.starts_with("crop=w='trunc(min(iw,ih*9/16)/2)*2'"), "裁剪应最先应用：{vf}");
        assert!(vf.contains(",transpose=1,"), "旋转应在裁剪之后：{vf}");
        assert!(vf.ends_with(",scale=-2:720:flags=lanczos"), "缩放应最后应用：{vf}");
    }

    #[test]
    fn rotation_180_uses_flip_pair() {
        let mut p = video_params();
        p.rotation = "180".into();
        let args = build_video("libx264", "in.mp4", "out.mp4", &p, "aac", None);
        assert_eq!(filter_arg(&args).as_deref(), Some("hflip,vflip"));
    }

    #[test]
    fn no_adjustments_means_no_filter_graph() {
        let p = video_params();
        let args = build_video("libx264", "in.mp4", "out.mp4", &p, "aac", None);
        assert!(filter_arg(&args).is_none(), "无裁剪/旋转/缩放时不应插入 -vf");
    }

    #[test]
    fn crop_16_9_and_1_1_expressions() {
        let mut p = video_params();
        p.crop = "1:1".into();
        let square = build_video("libx264", "in.mp4", "out.mp4", &p, "aac", None);
        assert_eq!(filter_arg(&square).as_deref(), Some("crop=w='trunc(min(iw,ih)/2)*2':h='trunc(min(iw,ih)/2)*2'"));

        p.crop = "16:9".into();
        let wide = build_video("libx264", "in.mp4", "out.mp4", &p, "aac", None);
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
            assert_eq!(parse_exif_orientation(&exif_jpeg(orientation)), Some(orientation));
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
        let args = build_image(&input.to_string_lossy(), "out.jpg", &params);
        // 关键：ffmpeg 自身的 autorotate 必须关闭，否则会和显式 transpose 叠加。
        assert!(args.contains(&"-noautorotate".to_string()));
        let vf = args[args.iter().position(|a| a == "-vf").unwrap() + 1].clone();
        assert_eq!(vf, "transpose=1");

        // 关闭自动校正时不应出现任何 EXIF 变换。
        let params_off = ImageParams { auto_orient: false, ..params.clone() };
        let args_off = build_image(&input.to_string_lossy(), "out.jpg", &params_off);
        assert!(!args_off.contains(&"-vf".to_string()));

        // 自动校正 + 手动 90°，必须是「EXIF 一次 + 手动一次」，而不是被 ffmpeg 再隐式转一次。
        let params_manual = ImageParams { rotation: "90".into(), ..params };
        let args_manual = build_image(&input.to_string_lossy(), "out.jpg", &params_manual);
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
        assert_eq!(args[args.iter().position(|a| a == "-ss").unwrap() + 1], "1.500");
        assert_eq!(args[args.iter().position(|a| a == "-t").unwrap() + 1], "2.000");
        let vf = args[args.iter().position(|a| a == "-vf").unwrap() + 1].clone();
        assert!(vf.contains("fps=12"));
        assert!(vf.contains("scale=320:-2"));
        assert!(vf.contains("palettegen") && vf.contains("paletteuse"));

        let plain = GifParams { optimize: false, ..p };
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
        let maps: Vec<&String> = args.iter().filter(|a| a.starts_with("0:") || a.starts_with("1:")).collect();
        assert_eq!(maps, vec!["0:v:0", "1:a:0"]);
        let pos = args.iter().position(|a| a == "-c:v").unwrap();
        assert_eq!(args[pos + 1], "copy");
        assert_eq!(args[args.iter().position(|a| a == "-t").unwrap() + 1], "3.000");

        let add = MuxParams { mode: "add".into(), audio_mode: "copy".into(), ..replace };
        let args_add = build_mux("v.mp4", "a.aac", "out.mp4", &add, None);
        let maps_add: Vec<&String> = args_add.iter().filter(|a| a.starts_with("0:") || a.starts_with("1:")).collect();
        assert_eq!(maps_add, vec!["0:v:0", "0:a:0?", "1:a:0"]);
        assert_eq!(args_add[args_add.iter().position(|a| a == "-c:a").unwrap() + 1], "copy");
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
        let different_res = crate::probe::MediaInfo { width: Some(640), height: Some(480), ..base.clone() };
        assert!(merge_incompatibility(&base, &different_res).unwrap().contains("分辨率"));
        let different_codec = crate::probe::MediaInfo { video_codec: Some("hevc".into()), ..base.clone() };
        assert!(merge_incompatibility(&base, &different_codec).unwrap().contains("编码"));
        let no_audio = crate::probe::MediaInfo { audio_codec: None, channels: None, sample_rate: None, ..base.clone() };
        assert!(merge_incompatibility(&base, &no_audio).unwrap().contains("音轨"));
    }

    #[test]
    fn concat_list_uses_forward_slashes_and_escapes_quotes() {
        let list = concat_list(&[
            "D:\\videos\\a.mp4".to_string(),
            "C:\\it's\\b.mp4".to_string(),
        ]);
        assert_eq!(
            list,
            "file 'D:/videos/a.mp4'\nfile 'C:/it'\\''s/b.mp4'\n"
        );
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
        let args = build_image("in.png", "out.png", &p);
        let vf = filter_arg(&args).expect("应生成 -vf");
        assert_eq!(
            vf,
            "crop=300:200:10:20,transpose=1,scale=w=min(iw\\,200):h=min(ih\\,200):force_original_aspect_ratio=decrease:flags=lanczos"
        );
    }

    #[test]
    fn image_crop_validation_rejects_bad_input() {
        let mut p = ImageParams { crop_mode: "custom".into(), ..Default::default() };
        assert!(validate_image_crop(&p).unwrap_err().contains("大于 0"));
        p.crop_w = 100;
        p.crop_h = 100;
        p.crop_x = -1;
        assert!(validate_image_crop(&p).unwrap_err().contains("负数"));

        let ratio = ImageParams { crop_mode: "ratio".into(), crop_ratio: "5:4".into(), ..Default::default() };
        assert!(validate_image_crop(&ratio).unwrap_err().contains("不支持的裁剪比例"));
        let mode = ImageParams { crop_mode: "magic".into(), ..Default::default() };
        assert!(validate_image_crop(&mode).unwrap_err().contains("不支持的裁剪模式"));

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
        let p = SeqParams { size_mode: "source".into(), fps: 10, ..Default::default() };
        assert!(seq_target_size(&p, &[(320, 240), (320, 240)]).is_ok());
        let err = seq_target_size(&p, &[(320, 240), (640, 480)]).unwrap_err();
        assert!(err.contains("要求所有图片同尺寸"), "{err}");

        let fixed = SeqParams { size_mode: "fixed".into(), width: 0, height: 0, ..Default::default() };
        assert!(seq_target_size(&fixed, &[(320, 240)]).unwrap_err().contains("大于 0"));
        let bad = SeqParams { size_mode: "weird".into(), ..Default::default() };
        assert!(seq_target_size(&bad, &[(320, 240)]).unwrap_err().contains("不支持"));
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
        assert_eq!(args[args.iter().position(|a| a == "-framerate").unwrap() + 1], "12");
        assert_eq!(args[args.iter().position(|a| a == "-start_number").unwrap() + 1], "7");
        assert_eq!(args[args.iter().position(|a| a == "-frames:v").unwrap() + 1], "7");
        let vf = args[args.iter().position(|a| a == "-vf").unwrap() + 1].clone();
        assert!(vf.contains("scale=320:240") && vf.contains("pad=320:240"), "{vf}");
    }

    #[test]
    fn frames_build_sets_fps_and_output_pattern() {
        let p = FramesParams { fps: 5, width: 320, format: "png".into(), quality: 90 };
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
        let err = build_collage(&["a.png".into(), "b.png".into(), "c.png".into()], "out.png", &p)
            .unwrap_err();
        assert!(err.contains("放不进"), "{err}");

        let bad_color = CollageParams { background: "white".into(), ..p.clone() };
        assert!(build_collage(&["a.png".into()], "out.png", &bad_color)
            .unwrap_err()
            .contains("RRGGBB"));

        let args = build_collage(&["a.png".into(), "b.png".into()], "out.png", &p).unwrap();
        let graph = args[args.iter().position(|a| a == "-filter_complex").unwrap() + 1].clone();
        assert!(graph.contains("xstack=inputs=2:layout=0_0|100_0"), "{graph}");
        assert_eq!(collage_canvas(2, 2, 100, 80, 10), (210, 170));
    }
}
