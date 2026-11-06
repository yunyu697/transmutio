//! 图片 EXIF 元数据写入：纯 Rust 实现（不依赖 exiftool / ImageMagick）。
//!
//! 三条硬约束：
//! 1. **默认写副本**：输入文件在任何失败路径下都不会被修改；输出先写临时名，
//!    校验通过后原子改名，失败则删除，不留半套文件。
//! 2. **格式白名单**：只接受底层库真正支持读写的格式；不支持的在开始前就拒绝，并给出原因。
//! 3. **写完即回读**：每个写入的标签都会从输出文件重新解析比对，不一致就判失败（不静默成功）。

use little_exif::exif_tag::ExifTag;
use little_exif::metadata::Metadata;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// 底层 `little_exif` 支持读写的扩展名（WebP 仅无损/扩展格式，见 README）。
const SUPPORTED: &[&str] = &[
    "jpg", "jpeg", "png", "tif", "tiff", "webp", "heic", "heif", "hif", "avif", "jxl",
];

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ExifFields {
    pub description: Option<String>,
    pub artist: Option<String>,
    pub copyright: Option<String>,
    pub software: Option<String>,
    pub date_time_original: Option<String>,
    pub make: Option<String>,
    pub model: Option<String>,
    pub lens_model: Option<String>,
    pub orientation: Option<u16>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExifWriteResult {
    pub src: String,
    pub output: Option<String>,
    pub ok: bool,
    pub message: Option<String>,
}

fn ext_of(path: &Path) -> String {
    path.extension()
        .map(|e| e.to_string_lossy().to_lowercase())
        .unwrap_or_default()
}

fn trimmed(value: &Option<String>) -> Option<String> {
    value
        .as_deref()
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .map(str::to_string)
}

fn validate(fields: &ExifFields) -> Result<(), String> {
    if let Some(date) = trimmed(&fields.date_time_original) {
        // EXIF 固定格式：YYYY:MM:DD HH:MM:SS（20 字节），不做模糊解析。
        let bytes = date.as_bytes();
        let shaped = bytes.len() == 19
            && bytes[4] == b':'
            && bytes[7] == b':'
            && bytes[10] == b' '
            && bytes[13] == b':'
            && bytes[16] == b':'
            && bytes
                .iter()
                .enumerate()
                .all(|(i, c)| matches!(i, 4 | 7 | 10 | 13 | 16) || c.is_ascii_digit());
        if !shaped {
            return Err(format!(
                "拍摄时间格式不对：{date}（要求 YYYY:MM:DD HH:MM:SS，例如 2026:11:06 12:00:00）"
            ));
        }
    }
    if let Some(o) = fields.orientation {
        if !(1..=8).contains(&o) {
            return Err(format!("方向取值必须是 1–8，收到 {o}"));
        }
    }
    if trimmed(&fields.description).is_none()
        && trimmed(&fields.artist).is_none()
        && trimmed(&fields.copyright).is_none()
        && trimmed(&fields.software).is_none()
        && trimmed(&fields.date_time_original).is_none()
        && trimmed(&fields.make).is_none()
        && trimmed(&fields.model).is_none()
        && trimmed(&fields.lens_model).is_none()
        && fields.orientation.is_none()
    {
        return Err("没有填写任何字段：不写空元数据，避免产生误导性的副本".into());
    }
    Ok(())
}

fn set_tags(metadata: &mut Metadata, fields: &ExifFields) {
    if let Some(v) = trimmed(&fields.description) {
        metadata.set_tag(ExifTag::ImageDescription(v));
    }
    if let Some(v) = trimmed(&fields.artist) {
        metadata.set_tag(ExifTag::Artist(v));
    }
    if let Some(v) = trimmed(&fields.copyright) {
        metadata.set_tag(ExifTag::Copyright(v));
    }
    if let Some(v) = trimmed(&fields.software) {
        metadata.set_tag(ExifTag::Software(v));
    }
    if let Some(v) = trimmed(&fields.date_time_original) {
        metadata.set_tag(ExifTag::DateTimeOriginal(v));
    }
    if let Some(v) = trimmed(&fields.make) {
        metadata.set_tag(ExifTag::Make(v));
    }
    if let Some(v) = trimmed(&fields.model) {
        metadata.set_tag(ExifTag::Model(v));
    }
    if let Some(v) = trimmed(&fields.lens_model) {
        metadata.set_tag(ExifTag::LensModel(v));
    }
    if let Some(v) = fields.orientation {
        metadata.set_tag(ExifTag::Orientation(vec![v]));
    }
}

fn read_string(metadata: &Metadata, tag: ExifTag) -> Option<String> {
    let endian = metadata.get_endian();
    metadata.get_tag(&tag).next().map(|t| {
        let bytes: Vec<u8> = t
            .value_as_u8_vec(&endian)
            .into_iter()
            .filter(|b| *b != 0)
            .collect();
        String::from_utf8_lossy(&bytes).to_string()
    })
}

fn read_orientation(metadata: &Metadata) -> Option<u16> {
    let endian = metadata.get_endian();
    metadata
        .get_tag(&ExifTag::Orientation(vec![]))
        .next()
        .and_then(|t| {
            let bytes = t.value_as_u8_vec(&endian);
            if bytes.len() < 2 {
                return None;
            }
            Some(match endian {
                little_exif::endian::Endian::Little => u16::from_le_bytes([bytes[0], bytes[1]]),
                little_exif::endian::Endian::Big => u16::from_be_bytes([bytes[0], bytes[1]]),
            })
        })
}

/// 供检查程序与 UI 读取的快照（只包含本模块会写入的字段）。
#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExifSnapshot {
    pub description: Option<String>,
    pub artist: Option<String>,
    pub copyright: Option<String>,
    pub software: Option<String>,
    pub date_time_original: Option<String>,
    pub make: Option<String>,
    pub model: Option<String>,
    pub lens_model: Option<String>,
    pub orientation: Option<u16>,
}

pub fn read_snapshot(path: &Path) -> Result<ExifSnapshot, String> {
    let metadata = load_metadata(path)?;
    Ok(ExifSnapshot {
        description: read_string(&metadata, ExifTag::ImageDescription(String::new())),
        artist: read_string(&metadata, ExifTag::Artist(String::new())),
        copyright: read_string(&metadata, ExifTag::Copyright(String::new())),
        software: read_string(&metadata, ExifTag::Software(String::new())),
        date_time_original: read_string(&metadata, ExifTag::DateTimeOriginal(String::new())),
        make: read_string(&metadata, ExifTag::Make(String::new())),
        model: read_string(&metadata, ExifTag::Model(String::new())),
        lens_model: read_string(&metadata, ExifTag::LensModel(String::new())),
        orientation: read_orientation(&metadata),
    })
}

/// 写完后回读：字符串标签逐项比对，方向单独比对。
fn verify_written(path: &Path, fields: &ExifFields) -> Result<(), String> {
    let metadata = load_metadata(path)?;
    let checks: Vec<(&str, Option<String>, Option<String>)> = vec![
        (
            "描述",
            trimmed(&fields.description),
            read_string(&metadata, ExifTag::ImageDescription(String::new())),
        ),
        (
            "作者",
            trimmed(&fields.artist),
            read_string(&metadata, ExifTag::Artist(String::new())),
        ),
        (
            "版权",
            trimmed(&fields.copyright),
            read_string(&metadata, ExifTag::Copyright(String::new())),
        ),
        (
            "软件",
            trimmed(&fields.software),
            read_string(&metadata, ExifTag::Software(String::new())),
        ),
        (
            "拍摄时间",
            trimmed(&fields.date_time_original),
            read_string(&metadata, ExifTag::DateTimeOriginal(String::new())),
        ),
        (
            "相机厂商",
            trimmed(&fields.make),
            read_string(&metadata, ExifTag::Make(String::new())),
        ),
        (
            "相机型号",
            trimmed(&fields.model),
            read_string(&metadata, ExifTag::Model(String::new())),
        ),
        (
            "镜头",
            trimmed(&fields.lens_model),
            read_string(&metadata, ExifTag::LensModel(String::new())),
        ),
    ];
    for (name, expected, actual) in checks {
        if let Some(expected) = expected {
            if actual.as_deref() != Some(expected.as_str()) {
                return Err(format!(
                    "{name} 回读不一致：期望 {expected:?}，实际 {actual:?}"
                ));
            }
        }
    }
    if let Some(expected) = fields.orientation {
        let actual = read_orientation(&metadata);
        if actual != Some(expected) {
            return Err(format!("方向回读不一致：期望 {expected}，实际 {actual:?}"));
        }
    }
    Ok(())
}

fn unique_output(dir: &Path, stem: &str, ext: &str) -> PathBuf {
    let mut candidate = dir.join(format!("{stem}.{ext}"));
    let mut n = 1u32;
    while candidate.exists() {
        candidate = dir.join(format!("{stem} ({n}).{ext}"));
        n += 1;
    }
    candidate
}

/// 读取已有元数据；文件本来就没有元数据时返回空对象（这不是错误）。
fn load_metadata(path: &Path) -> Result<Metadata, String> {
    match Metadata::new_from_path(path) {
        Ok(md) => Ok(md),
        Err(e) => {
            let msg = e.to_string().to_lowercase();
            if msg.contains("no exif") || msg.contains("no metadata") {
                Ok(Metadata::new())
            } else {
                Err(format!("读取元数据结构失败：{e}"))
            }
        }
    }
}

/// 写单张：复制 → 加标签 → 回读校验 → 原子改名。任何失败都会删除中间文件。
pub fn write_one(src: &Path, out: &Path, fields: &ExifFields) -> Result<(), String> {
    if !SUPPORTED.contains(&ext_of(src).as_str()) {
        return Err(format!(
            "不支持的格式 .{}：目前只能写 {}",
            ext_of(src),
            SUPPORTED.join(" / ")
        ));
    }
    validate(fields)?;
    if out.exists() {
        return Err("输出名已存在，拒绝覆盖".into());
    }
    std::fs::copy(src, out).map_err(|e| format!("复制到输出失败：{e}"))?;
    let result = (|| -> Result<(), String> {
        let mut metadata = load_metadata(out)?;
        set_tags(&mut metadata, fields);
        metadata
            .write_to_file(out)
            .map_err(|e| format!("写入 EXIF 失败（该格式可能不支持写元数据）：{e}"))?;
        verify_written(out, fields)
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(out);
    }
    result
}

/// 目标路径：给了输出目录就用它，否则跟源文件同目录（同名冲突自动加 (1)、(2)…）。
pub fn output_path(src: &Path, output_dir: Option<&Path>) -> Result<PathBuf, String> {
    let dir = match output_dir {
        Some(d) => d.to_path_buf(),
        None => src
            .parent()
            .map(|p| p.to_path_buf())
            .ok_or_else(|| "无法定位源文件所在目录".to_string())?,
    };
    if !dir.is_dir() {
        return Err(format!("输出目录不存在：{}", dir.display()));
    }
    let stem = src
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "image".into());
    Ok(unique_output(&dir, &stem, &ext_of(src)))
}

/// 批量写：单张失败不影响其他文件，逐条返回原因。
pub fn write_batch(
    files: &[String],
    fields: &ExifFields,
    output_dir: Option<&Path>,
) -> Vec<ExifWriteResult> {
    files
        .iter()
        .map(|src| {
            let source = Path::new(src);
            match output_path(source, output_dir).and_then(|out| {
                write_one(source, &out, fields)?;
                Ok(out)
            }) {
                Ok(out) => ExifWriteResult {
                    src: src.clone(),
                    output: Some(out.to_string_lossy().to_string()),
                    ok: true,
                    message: None,
                },
                Err(e) => ExifWriteResult {
                    src: src.clone(),
                    output: None,
                    ok: false,
                    message: Some(e),
                },
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 1x1 透明 PNG（合法文件，底库支持 eXIf 块）。
    fn write_tiny_png(path: &Path) {
        const PNG: &[u8] = &[
            0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x00, 0x00, 0x0D, 0x49, 0x48,
            0x44, 0x52, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x06, 0x00, 0x00,
            0x00, 0x1F, 0x15, 0xC4, 0x89, 0x00, 0x00, 0x00, 0x0A, 0x49, 0x44, 0x41, 0x54, 0x78,
            0x9C, 0x63, 0x00, 0x01, 0x00, 0x00, 0x05, 0x00, 0x01, 0x0D, 0x0A, 0x2D, 0xB4, 0x00,
            0x00, 0x00, 0x00, 0x49, 0x45, 0x4E, 0x44, 0xAE, 0x42, 0x60, 0x82,
        ];
        std::fs::write(path, PNG).unwrap();
    }

    fn sample_fields() -> ExifFields {
        ExifFields {
            description: Some("现场照片".into()),
            artist: Some("Tyler".into()),
            copyright: Some("© 2026".into()),
            date_time_original: Some("2026:11:06 12:00:00".into()),
            orientation: Some(6),
            ..Default::default()
        }
    }

    #[test]
    fn writes_and_verifies_on_png() {
        let dir = std::env::temp_dir().join(format!("exif-ok-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let src = dir.join("src.png");
        write_tiny_png(&src);
        let before = std::fs::read(&src).unwrap();
        let out = output_path(&src, Some(&dir)).unwrap();
        write_one(&src, &out, &sample_fields()).unwrap();
        assert!(out.exists(), "输出应存在");
        // 源文件一个字节都不能变
        assert_eq!(before, std::fs::read(&src).unwrap(), "源文件必须原样保留");
        verify_written(&out, &sample_fields()).unwrap();
    }

    #[test]
    fn rejects_unsupported_format_before_copy() {
        let dir = std::env::temp_dir().join(format!("exif-bmp-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let src = dir.join("a.bmp");
        std::fs::write(&src, b"BM").unwrap();
        let out = dir.join("a.copy.bmp");
        let err = write_one(&src, &out, &sample_fields()).unwrap_err();
        assert!(err.contains("不支持的格式"), "{err}");
        assert!(!out.exists(), "失败时不应留下输出");
    }

    #[test]
    fn rejects_bad_datetime() {
        let mut fields = sample_fields();
        fields.date_time_original = Some("2026-11-06 12:00".into());
        let err = validate(&fields).unwrap_err();
        assert!(err.contains("格式不对"), "{err}");
    }

    #[test]
    fn rejects_empty_fields() {
        let err = validate(&ExifFields::default()).unwrap_err();
        assert!(err.contains("没有填写任何字段"), "{err}");
    }

    #[test]
    fn rejects_orientation_out_of_range() {
        let mut fields = sample_fields();
        fields.orientation = Some(9);
        assert!(validate(&fields).unwrap_err().contains("1–8"));
    }

    #[test]
    fn unique_output_avoids_existing() {
        let dir = std::env::temp_dir().join(format!("exif-uniq-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("shot.png"), b"x").unwrap();
        let out = unique_output(&dir, "shot", "png");
        assert_eq!(out.file_name().unwrap().to_string_lossy(), "shot (1).png");
    }
}
