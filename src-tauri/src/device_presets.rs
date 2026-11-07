//! 设备预设模板：只覆盖「容器 / 编码 / 分辨率 / 帧率 / 音轨码率」这几个
//! 与设备兼容性直接相关的参数，其余项沿用面板当前设置（裁剪、水印、变速等）。
//! 不做 profile/level 深度适配，也不预置码率公式——这是模板，不是“自动最优”。

use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DevicePreset {
    pub id: &'static str,
    pub label: &'static str,
    pub note: &'static str,
    pub container: &'static str,
    pub codec: &'static str,
    pub resolution: &'static str,
    pub fps: &'static str,
    pub audio_bitrate_kbps: u32,
}

impl DevicePreset {
    /// 模板要写入任务参数的那部分字段（前端与 headless 共用同一份声明）。
    pub fn params_json(&self) -> serde_json::Value {
        serde_json::json!({
            "container": self.container,
            "codec": self.codec,
            "encoder": "auto",
            "resolution": self.resolution,
            "fps": self.fps,
            "audioMode": "aac",
            "audioBitrateKbps": self.audio_bitrate_kbps,
            "audioRateMode": "bitrate",
            "faststart": true,
        })
    }
}

pub const PRESETS: &[DevicePreset] = &[
    DevicePreset {
        id: "apple-1080p",
        label: "iPhone / iPad（1080p H.264）",
        note: "Apple 设备通用档：MP4 + H.264 + AAC 128 kbps，1080p / 30 fps",
        container: "mp4",
        codec: "h264",
        resolution: "1080",
        fps: "30",
        audio_bitrate_kbps: 128,
    },
    DevicePreset {
        id: "android-720p",
        label: "Android 手机 / 平板（720p H.264）",
        note: "兼容性优先：MP4 + H.264 + AAC 128 kbps，720p / 30 fps",
        container: "mp4",
        codec: "h264",
        resolution: "720",
        fps: "30",
        audio_bitrate_kbps: 128,
    },
    DevicePreset {
        id: "psp-480p",
        label: "PSP（480p H.264）",
        note: "老设备档：MP4 + H.264 + AAC 96 kbps，480p / 30 fps",
        container: "mp4",
        codec: "h264",
        resolution: "480",
        fps: "30",
        audio_bitrate_kbps: 96,
    },
    DevicePreset {
        id: "blackberry-480p",
        label: "BlackBerry（480p H.264）",
        note: "老设备档：MP4 + H.264 + AAC 96 kbps，480p / 30 fps",
        container: "mp4",
        codec: "h264",
        resolution: "480",
        fps: "30",
        audio_bitrate_kbps: 96,
    },
];

pub fn list() -> &'static [DevicePreset] {
    PRESETS
}

pub fn find(id: &str) -> Option<&'static DevicePreset> {
    PRESETS.iter().find(|p| p.id == id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::args::{build_video, video_base_dims, VideoParams};

    #[test]
    fn presets_map_to_expected_dimensions_and_frame_rate() {
        let cases = [
            ("apple-1080p", (1920, 1080)),
            ("android-720p", (1280, 720)),
            ("psp-480p", (854, 480)),
            ("blackberry-480p", (854, 480)),
        ];
        for (id, dims) in cases {
            let preset = find(id).expect("预设必须存在");
            let merged: VideoParams =
                serde_json::from_value(preset.params_json()).expect("模板参数必须可反序列化");
            assert_eq!(
                video_base_dims(&merged, 1280, 720),
                dims,
                "{id} 的模板分辨率应为 {dims:?}"
            );
            assert_eq!(merged.fps, "30", "{id} 的帧率应为 30");
            let argv = build_video("libx264", "in.mp4", "out.mp4", &merged, "aac", None).unwrap();
            // 帧率以 `-r 30` 写入（不是滤镜里的 fps=30）。
            let r_index = argv.iter().position(|a| a == "-r").expect("应写入 -r");
            assert_eq!(argv[r_index + 1], "30", "{id} 应锁定帧率 30");
            assert_eq!(merged.audio_bitrate_kbps, preset.audio_bitrate_kbps);
        }
    }

    #[test]
    fn unknown_preset_is_rejected_and_list_is_stable() {
        assert!(find("nope").is_none());
        assert_eq!(list().len(), 4);
    }
}
