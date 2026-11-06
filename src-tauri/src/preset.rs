//! 配置预设导入/导出。预设只包含「面板参数 + 输出策略」，明确排除本机路径、
//! 凭据与任务历史。导入时逐层校验：版本、kind、未知字段、字段类型与枚举值，
//! 任何非法字段都返回具体路径，不静默忽略。

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::path::Path;

pub const PRESET_KIND: &str = "transmutio-preset";
pub const PRESET_VERSION: u64 = 1;

const ROOT_KEYS: [&str; 5] = ["version", "kind", "createdAt", "panels", "output"];
const PANEL_KEYS: [&str; 10] = [
    "video", "audio", "image", "merge", "mux", "gif", "seq", "frames", "collage", "pdf",
];
const OUTPUT_KEYS: [&str; 3] = ["conflict", "nameTemplate", "afterAction"];

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PresetFile {
    pub version: u64,
    pub kind: String,
    pub created_at: u64,
    pub panels: Map<String, Value>,
    pub output: Map<String, Value>,
}

fn ensure_object<'a>(value: &'a Value, label: &str) -> Result<&'a Map<String, Value>, String> {
    value
        .as_object()
        .ok_or_else(|| format!("{label} 必须是对象"))
}

fn check_keys(map: &Map<String, Value>, allowed: &[&str], label: &str) -> Result<(), String> {
    for key in map.keys() {
        if !allowed.contains(&key.as_str()) {
            return Err(format!(
                "{label} 含未知字段「{key}」；允许的字段：{}",
                allowed.join(", ")
            ));
        }
    }
    Ok(())
}

fn validate_output(output: &Map<String, Value>) -> Result<(), String> {
    check_keys(output, &OUTPUT_KEYS, "output")?;
    if let Some(conflict) = output.get("conflict") {
        let text = conflict
            .as_str()
            .ok_or_else(|| "output.conflict 必须是字符串".to_string())?;
        if !["suffix", "skip", "overwrite"].contains(&text) {
            return Err(format!(
                "output.conflict 的值「{text}」非法，可选 suffix / skip / overwrite"
            ));
        }
    }
    if let Some(template) = output.get("nameTemplate") {
        let text = template
            .as_str()
            .ok_or_else(|| "output.nameTemplate 必须是字符串".to_string())?;
        let mut rest = text;
        while let Some(start) = rest.find('{') {
            let end = rest[start..]
                .find('}')
                .map(|e| start + e)
                .ok_or_else(|| format!("output.nameTemplate 的占位符未闭合：{text}"))?;
            let inner = &rest[start + 1..end];
            if !["name", "date", "index", "ext"].contains(&inner) {
                return Err(format!(
                    "output.nameTemplate 含未知占位符「{{{inner}}}」，可选 name/date/index/ext"
                ));
            }
            rest = &rest[end + 1..];
        }
        if text.contains('}') {
            let depth_ok = text.matches('{').count() == text.matches('}').count();
            if !depth_ok {
                return Err(format!("output.nameTemplate 的花括号不匹配：{text}"));
            }
        }
        // 预设里绝不允许出现路径（本机路径不可移植，也可能泄漏用户名）。
        if text.contains('/') || text.contains('\\') || text.contains(':') {
            return Err("output.nameTemplate 不能包含路径分隔符或盘符".into());
        }
    }
    if let Some(action) = output.get("afterAction") {
        let text = action
            .as_str()
            .ok_or_else(|| "output.afterAction 必须是字符串".to_string())?;
        if !["none", "open-folder", "shutdown"].contains(&text) {
            return Err(format!(
                "output.afterAction 的值「{text}」非法，可选 none / open-folder / shutdown"
            ));
        }
    }
    Ok(())
}

fn validate_panel(panel: &Value, name: &str) -> Result<(), String> {
    let map = ensure_object(panel, &format!("panels.{name}"))?;
    let allowed: &[&str] = match name {
        "video" => &[
            "container",
            "codec",
            "encoder",
            "rateMode",
            "quality",
            "bitrateKbps",
            "resolution",
            "fps",
            "crop",
            "rotation",
            "audioMode",
            "audioBitrateKbps",
            "audioRateMode",
            "hwDecode",
            "faststart",
        ],
        "audio" => &[
            "format",
            "bitrateKbps",
            "rateMode",
            "sampleRate",
            "channels",
            "normalize",
        ],
        "image" => &[
            "format",
            "quality",
            "maxEdge",
            "rotation",
            "autoOrient",
            "cropMode",
            "cropRatio",
            "cropX",
            "cropY",
            "cropW",
            "cropH",
        ],
        "merge" => &[
            "mode",
            "resolution",
            "fps",
            "audioMode",
            "codec",
            "quality",
            "bitrateKbps",
        ],
        "mux" => &["mode", "audioMode", "audioBitrateKbps", "audioRateMode"],
        "gif" => &["fps", "width", "startSec", "durationSec", "optimize"],
        "seq" => &[
            "fps",
            "sizeMode",
            "width",
            "height",
            "encoder",
            "quality",
            "bitrateKbps",
        ],
        "frames" => &["fps", "width", "format", "quality"],
        "collage" => &[
            "cols",
            "rows",
            "cellWidth",
            "cellHeight",
            "gap",
            "background",
            "fit",
            "format",
            "quality",
        ],
        "pdf" => &["mergeMode", "splitMode", "ranges", "imageFormat", "dpi"],
        _ => unreachable!(),
    };
    check_keys(map, allowed, &format!("panels.{name}"))?;
    // 面板结构体本身有默认值，这里用对应类型反序列化做类型检查；
    // 未知字段已在上面拒绝，不会走到 serde 的「忽略未知字段」。
    let typed: Result<(), String> = match name {
        "video" => serde_json::from_value::<crate::args::VideoParams>(panel.clone())
            .map(|_| ())
            .map_err(|e| format!("panels.video 字段类型错误：{e}")),
        "audio" => serde_json::from_value::<crate::args::AudioParams>(panel.clone())
            .map(|_| ())
            .map_err(|e| format!("panels.audio 字段类型错误：{e}")),
        "image" => serde_json::from_value::<crate::args::ImageParams>(panel.clone())
            .map(|_| ())
            .map_err(|e| format!("panels.image 字段类型错误：{e}")),
        "merge" => serde_json::from_value::<crate::args::MergeParams>(panel.clone())
            .map(|_| ())
            .map_err(|e| format!("panels.merge 字段类型错误：{e}")),
        "mux" => serde_json::from_value::<crate::args::MuxParams>(panel.clone())
            .map(|_| ())
            .map_err(|e| format!("panels.mux 字段类型错误：{e}")),
        "gif" => serde_json::from_value::<crate::args::GifParams>(panel.clone())
            .map(|_| ())
            .map_err(|e| format!("panels.gif 字段类型错误：{e}")),
        "seq" => serde_json::from_value::<crate::args::SeqParams>(panel.clone())
            .map(|_| ())
            .map_err(|e| format!("panels.seq 字段类型错误：{e}")),
        "frames" => serde_json::from_value::<crate::args::FramesParams>(panel.clone())
            .map(|_| ())
            .map_err(|e| format!("panels.frames 字段类型错误：{e}")),
        "collage" => serde_json::from_value::<crate::args::CollageParams>(panel.clone())
            .map(|_| ())
            .map_err(|e| format!("panels.collage 字段类型错误：{e}")),
        "pdf" => Ok(()),
        _ => unreachable!(),
    };
    typed
}

/// 构造并校验一份预设。返回可直接写盘的结构。
pub fn build(panels: Value, output: Value) -> Result<PresetFile, String> {
    let panels_map = ensure_object(&panels, "panels")?;
    check_keys(panels_map, &PANEL_KEYS, "panels")?;
    for (name, value) in panels_map {
        validate_panel(value, name)?;
    }
    let output_map = ensure_object(&output, "output")?;
    validate_output(output_map)?;

    Ok(PresetFile {
        version: PRESET_VERSION,
        kind: PRESET_KIND.to_string(),
        created_at: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0),
        panels: panels_map.clone(),
        output: output_map.clone(),
    })
}

/// 解析导入文件的 JSON 文本并做全量校验。
pub fn parse(text: &str) -> Result<PresetFile, String> {
    let value: Value = serde_json::from_str(text).map_err(|e| format!("预设不是合法 JSON：{e}"))?;
    let map = ensure_object(&value, "预设根")?;
    check_keys(map, &ROOT_KEYS, "预设根")?;

    let version = map
        .get("version")
        .ok_or_else(|| "预设缺少 version 字段".to_string())?
        .as_u64()
        .ok_or_else(|| "version 必须是整数".to_string())?;
    if version != PRESET_VERSION {
        return Err(format!(
            "预设版本 {version} 不受支持；当前只接受版本 {PRESET_VERSION}"
        ));
    }
    let kind = map
        .get("kind")
        .ok_or_else(|| "预设缺少 kind 字段".to_string())?
        .as_str()
        .ok_or_else(|| "kind 必须是字符串".to_string())?;
    if kind != PRESET_KIND {
        return Err(format!(
            "预设 kind 为「{kind}」，不是 Transmutio 预设（期望 {PRESET_KIND}）"
        ));
    }
    let panels = map
        .get("panels")
        .ok_or_else(|| "预设缺少 panels 字段".to_string())?;
    let output = map
        .get("output")
        .ok_or_else(|| "预设缺少 output 字段".to_string())?;
    let built = build(panels.clone(), output.clone())?;
    Ok(PresetFile {
        created_at: map
            .get("createdAt")
            .and_then(Value::as_u64)
            .unwrap_or(built.created_at),
        ..built
    })
}

pub fn write(path: &Path, preset: &PresetFile) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("无法创建目录（{}）：{e}", parent.display()))?;
    }
    let text = serde_json::to_string_pretty(preset).map_err(|e| format!("序列化预设失败：{e}"))?;
    std::fs::write(path, text).map_err(|e| format!("写入预设失败（{}）：{e}", path.display()))
}

pub fn read(path: &Path) -> Result<PresetFile, String> {
    let text = std::fs::read_to_string(path)
        .map_err(|e| format!("无法读取预设文件（{}）：{e}", path.display()))?;
    parse(&text)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn temp_path(tag: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!(
            "transmutio-preset-{tag}-{}.json",
            std::process::id()
        ))
    }

    #[test]
    fn roundtrip_keeps_panels_and_output_policy() {
        let preset = build(
            json!({
                "video": { "codec": "h264", "quality": 23, "resolution": "1080" },
                "image": { "format": "webp", "maxEdge": 1600, "cropMode": "ratio", "cropRatio": "1:1" },
                "pdf": { "splitMode": "ranges", "ranges": "1-3", "dpi": 150 }
            }),
            json!({ "conflict": "suffix", "nameTemplate": "{name}-{index}" }),
        )
        .unwrap();
        let path = temp_path("roundtrip");
        write(&path, &preset).unwrap();
        let loaded = read(&path).unwrap();
        assert_eq!(loaded.version, 1);
        assert_eq!(loaded.kind, PRESET_KIND);
        assert_eq!(loaded.output.get("conflict").unwrap(), "suffix");
        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn rejects_wrong_version_kind_and_unknown_fields() {
        let base = r#"{"version":1,"kind":"transmutio-preset","panels":{},"output":{}}"#;
        let err = parse(&base.replace("\"version\":1", "\"version\":2")).unwrap_err();
        assert!(err.contains("版本 2 不受支持"), "{err}");

        let err = parse(&base.replace("transmutio-preset", "other-app")).unwrap_err();
        assert!(err.contains("不是 Transmutio 预设"), "{err}");

        let err =
            parse(r#"{"version":1,"kind":"transmutio-preset","panels":{},"output":{},"extra":1}"#)
                .unwrap_err();
        assert!(err.contains("未知字段「extra」"), "{err}");

        let err = parse(
            r#"{"version":1,"kind":"transmutio-preset","panels":{"unknown":{}},"output":{}}"#,
        )
        .unwrap_err();
        assert!(err.contains("未知字段「unknown」"), "{err}");
    }

    #[test]
    fn rejects_invalid_field_values_explicitly() {
        let err = parse(r#"{"version":1,"kind":"transmutio-preset","panels":{"video":{"colorSpace":"bt709"}},"output":{}}"#)
            .unwrap_err();
        assert!(
            err.contains("panels.video 含未知字段「colorSpace」"),
            "{err}"
        );

        let err = parse(r#"{"version":1,"kind":"transmutio-preset","panels":{},"output":{"conflict":"rename"}}"#)
            .unwrap_err();
        assert!(err.contains("conflict"), "{err}");

        let err = parse(r#"{"version":1,"kind":"transmutio-preset","panels":{},"output":{"nameTemplate":"{name}-{bad}"}}"#)
            .unwrap_err();
        assert!(err.contains("未知占位符「{bad}」"), "{err}");

        let err = parse(r#"{"version":1,"kind":"transmutio-preset","panels":{},"output":{"nameTemplate":"C:\\out\\{name}"}}"#)
            .unwrap_err();
        assert!(err.contains("不能包含路径"), "{err}");

        let err = parse(r#"{"version":1,"kind":"transmutio-preset","panels":{"image":{"maxEdge":"big"}},"output":{}}"#)
            .unwrap_err();
        assert!(err.contains("panels.image 字段类型错误"), "{err}");
    }

    #[test]
    fn export_never_contains_paths_from_panels() {
        let preset = build(
            json!({ "video": { "codec": "h264" } }),
            json!({ "conflict": "skip" }),
        )
        .unwrap();
        let text = serde_json::to_string(&preset).unwrap();
        assert!(!text.contains("outputDir"));
        assert!(!text.contains("lastDir"));
        assert!(!text.contains("C:\\"));
    }
}
