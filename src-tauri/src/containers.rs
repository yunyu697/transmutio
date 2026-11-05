//! 视频容器 / 音频格式的单一事实来源。
//!
//! 之前「视频只有 mp4、音频只有 mp3」的根因是把容器和格式硬编码在 UI 与命令行里。
//! 本模块把「容器、扩展名、muxer、兼容编码器、音频编码器」集中声明，供参数构造、
//! 入队校验与测试共用；前端通过同构清单渲染（见 `src/lib/formats.ts`）。

#[derive(Debug, Clone, Copy)]
pub struct VideoContainer {
    pub id: &'static str,
    pub label: &'static str,
    pub ext: &'static str,
    /// ffmpeg `-f` 的 muxer 名
    pub muxer: &'static str,
    /// 该容器允许的视频编码（与 probe 的 codec 字段一致）
    pub codecs: &'static [&'static str],
    /// 该容器允许的音频编码候选（按优先级）
    pub audio_codecs: &'static [&'static str],
    pub faststart: bool,
}

#[derive(Debug, Clone, Copy)]
pub struct AudioFormat {
    pub id: &'static str,
    pub label: &'static str,
    pub ext: &'static str,
    /// 编码器候选（按优先级）
    pub encoders: &'static [&'static str],
    /// 有损格式才需要码率；无损格式忽略 `-b:a`
    pub lossy: bool,
}

pub const VIDEO_CONTAINERS: &[VideoContainer] = &[
    VideoContainer {
        id: "mp4",
        label: "MP4",
        ext: "mp4",
        muxer: "mp4",
        codecs: &["h264", "hevc", "av1"],
        audio_codecs: &["aac"],
        faststart: true,
    },
    VideoContainer {
        id: "mkv",
        label: "MKV",
        ext: "mkv",
        muxer: "matroska",
        codecs: &["h264", "hevc", "av1", "vp9", "vp8"],
        audio_codecs: &["aac", "libopus", "libvorbis"],
        faststart: false,
    },
    VideoContainer {
        id: "webm",
        label: "WebM",
        ext: "webm",
        muxer: "webm",
        codecs: &["vp9", "vp8", "av1"],
        audio_codecs: &["libopus", "libvorbis"],
        faststart: false,
    },
    VideoContainer {
        id: "mov",
        label: "MOV",
        ext: "mov",
        muxer: "mov",
        codecs: &["h264", "hevc"],
        audio_codecs: &["aac"],
        faststart: true,
    },
    VideoContainer {
        id: "ts",
        label: "TS",
        ext: "ts",
        muxer: "mpegts",
        codecs: &["h264", "hevc"],
        audio_codecs: &["aac"],
        faststart: false,
    },
    VideoContainer {
        id: "avi",
        label: "AVI",
        ext: "avi",
        muxer: "avi",
        codecs: &["h264", "mpeg4"],
        audio_codecs: &["libmp3lame"],
        faststart: false,
    },
];

pub const AUDIO_FORMATS: &[AudioFormat] = &[
    AudioFormat {
        id: "mp3",
        label: "MP3",
        ext: "mp3",
        encoders: &["libmp3lame", "libshine"],
        lossy: true,
    },
    AudioFormat {
        id: "aac",
        label: "AAC（M4A）",
        ext: "m4a",
        encoders: &["aac"],
        lossy: true,
    },
    AudioFormat {
        id: "flac",
        label: "FLAC",
        ext: "flac",
        encoders: &["flac"],
        lossy: false,
    },
    AudioFormat {
        id: "wav",
        label: "WAV",
        ext: "wav",
        encoders: &["pcm_s16le"],
        lossy: false,
    },
    AudioFormat {
        id: "ogg",
        label: "OGG（Vorbis）",
        ext: "ogg",
        encoders: &["libvorbis"],
        lossy: true,
    },
    AudioFormat {
        id: "opus",
        label: "Opus",
        ext: "opus",
        encoders: &["libopus"],
        lossy: true,
    },
];

pub fn find_container(id: &str) -> Option<&'static VideoContainer> {
    VIDEO_CONTAINERS.iter().find(|c| c.id == id)
}

pub fn find_audio(id: &str) -> Option<&'static AudioFormat> {
    AUDIO_FORMATS.iter().find(|f| f.id == id)
}

pub fn container_ext(id: &str) -> Option<&'static str> {
    find_container(id).map(|c| c.ext)
}

pub fn audio_ext(id: &str) -> Option<&'static str> {
    find_audio(id).map(|f| f.ext)
}

fn list_containers() -> String {
    VIDEO_CONTAINERS
        .iter()
        .map(|c| c.id)
        .collect::<Vec<_>>()
        .join(" / ")
}

fn list_audio_formats() -> String {
    AUDIO_FORMATS
        .iter()
        .map(|f| f.id)
        .collect::<Vec<_>>()
        .join(" / ")
}

/// 校验容器与视频编码是否兼容；不兼容组合在入队阶段直接拒绝。
pub fn validate_video(container: &str, codec: &str) -> Result<(), String> {
    let container = if container.trim().is_empty() {
        "mp4"
    } else {
        container
    };
    let spec = find_container(container).ok_or_else(|| {
        format!(
            "不支持的视频容器「{container}」，可选：{}",
            list_containers()
        )
    })?;
    let codec = if codec.trim().is_empty() {
        "h264"
    } else {
        codec
    };
    if !spec.codecs.contains(&codec) {
        return Err(format!(
            "容器 {} 不支持 {} 编码，可组合：{}",
            spec.label,
            codec,
            spec.codecs.join(" / ")
        ));
    }
    Ok(())
}

pub fn validate_audio(format: &str) -> Result<(), String> {
    let format = if format.trim().is_empty() {
        "mp3"
    } else {
        format
    };
    if find_audio(format).is_none() {
        return Err(format!(
            "不支持的音频格式「{format}」，可选：{}",
            list_audio_formats()
        ));
    }
    Ok(())
}

/// 从探测结果里为该音频格式挑一个可用编码器；都不满足时返回首选编码器，
/// 让任务失败时能给出「编码器不可用」而不是静默改成别的格式。
pub fn pick_audio_encoder<'a>(format: &str, available: &[String]) -> &'a str {
    let spec = match find_audio(format) {
        Some(spec) => spec,
        None => return "libmp3lame",
    };
    spec.encoders
        .iter()
        .find(|id| available.iter().any(|a| a == *id))
        .copied()
        .unwrap_or(spec.encoders[0])
}

/// 为容器选择音频编码器。`preferred` 为空时用首个候选（如 webm → libopus）。
pub fn pick_container_audio_encoder<'a>(
    container: &str,
    preferred: &str,
    available: &[String],
) -> &'a str {
    let spec = match find_container(container) {
        Some(spec) => spec,
        None => return "aac",
    };
    if !preferred.trim().is_empty() {
        if let Some(cand) = spec.audio_codecs.iter().find(|id| **id == preferred) {
            if available.iter().any(|a| a == cand) || available.is_empty() {
                return cand;
            }
        }
    }
    spec.audio_codecs
        .iter()
        .find(|id| available.iter().any(|a| a == *id) || available.is_empty())
        .copied()
        .unwrap_or(spec.audio_codecs[0])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn at_least_four_video_containers() {
        assert!(VIDEO_CONTAINERS.len() >= 4, "视频容器至少 4 个");
        for id in ["mp4", "mkv", "webm", "mov"] {
            assert!(find_container(id).is_some(), "缺少容器 {id}");
        }
    }

    #[test]
    fn at_least_five_audio_formats() {
        assert!(AUDIO_FORMATS.len() >= 5, "音频格式至少 5 个");
        for id in ["mp3", "aac", "flac", "wav", "ogg", "opus"] {
            assert!(find_audio(id).is_some(), "缺少音频格式 {id}");
        }
        assert_eq!(audio_ext("aac"), Some("m4a"), "AAC 输出 m4a");
    }

    #[test]
    fn incompatible_container_codec_is_rejected() {
        assert!(validate_video("mp4", "vp9").is_err(), "mp4 不允许 vp9");
        assert!(validate_video("webm", "h264").is_err(), "webm 不允许 h264");
        assert!(validate_video("webm", "vp9").is_ok());
        assert!(validate_video("mkv", "vp9").is_ok());
        assert!(validate_video("mp4", "h264").is_ok());
        assert!(validate_video("avi", "mpeg4").is_ok());
    }

    #[test]
    fn unknown_container_and_audio_format_are_rejected() {
        let err = validate_video("flv", "h264").unwrap_err();
        assert!(err.contains("可选"), "错误应列出可用容器：{err}");
        let err = validate_audio("amr").unwrap_err();
        assert!(err.contains("可选"), "错误应列出可用格式：{err}");
        assert!(validate_audio("opus").is_ok());
    }

    #[test]
    fn container_audio_candidates_are_compatible() {
        assert_eq!(pick_container_audio_encoder("webm", "", &[]), "libopus");
        assert_eq!(pick_container_audio_encoder("mp4", "", &[]), "aac");
        assert_eq!(pick_container_audio_encoder("avi", "", &[]), "libmp3lame");
        // webm 下用户偏好 aac 时必须被拒绝并退回 opus，不能产出非法文件。
        assert_eq!(pick_container_audio_encoder("webm", "aac", &[]), "libopus");
    }

    #[test]
    fn audio_encoder_picks_only_allowed_candidates() {
        let available = vec!["libshine".to_string()];
        assert_eq!(pick_audio_encoder("mp3", &available), "libshine");
        let none: Vec<String> = Vec::new();
        assert_eq!(pick_audio_encoder("flac", &none), "flac");
        assert_eq!(pick_audio_encoder("wav", &none), "pcm_s16le");
    }
}
