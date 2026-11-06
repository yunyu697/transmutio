//! AVIF 编码（纯 Rust，无外部二进制）。
//!
//! 为什么不走 FFmpeg：内置的 BtbN LGPL/GPL 构建确实带 AV1 编码器（libaom-av1 / librav1e /
//! libsvtav1），但 **FFmpeg 没有 AVIF 封装器（muxer）**，`-f avif` 不存在，无法直接产出 `.avif`
//! 静图。因此这里用 `ravif`（rav1e 之上的 AVIF 封装）直接编码，代价是编译体积与时间增加。
//!
//! 约束与 EXIF 写入一致：只写副本、目标已存在则拒绝、失败删除半成品、写完校验容器签名。

use image::DynamicImage;
use ravif::{Encoder, RGBA8};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

const SUPPORTED: &[&str] = &["jpg", "jpeg", "png", "webp", "bmp", "tif", "tiff", "gif"];

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AvifParams {
    /// 1–100，数值越高越接近原图
    pub quality: u8,
    /// 1–10，越大越快、压缩率越低
    pub speed: u8,
}

impl Default for AvifParams {
    fn default() -> Self {
        Self {
            quality: 70,
            speed: 6,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AvifResult {
    pub src: String,
    pub output: Option<String>,
    pub width: u32,
    pub height: u32,
    pub bytes: u64,
    pub ok: bool,
    pub message: Option<String>,
}

fn ext_of(path: &Path) -> String {
    path.extension()
        .map(|e| e.to_string_lossy().to_lowercase())
        .unwrap_or_default()
}

pub fn validate(params: &AvifParams) -> Result<(), String> {
    if !(1..=100).contains(&params.quality) {
        return Err(format!("质量必须在 1–100，收到 {}", params.quality));
    }
    if !(1..=10).contains(&params.speed) {
        return Err(format!("速度必须在 1–10，收到 {}", params.speed));
    }
    Ok(())
}

/// AVIF 文件头必须是以 `ftyp` 开头的 box，且 major/compatible brand 含 `avif`。
pub fn looks_like_avif(bytes: &[u8]) -> bool {
    if bytes.len() < 16 || &bytes[4..8] != b"ftyp" {
        return false;
    }
    bytes[8..16].windows(4).any(|w| w == b"avif")
}

fn unique_output(dir: &Path, stem: &str) -> PathBuf {
    let mut candidate = dir.join(format!("{stem}.avif"));
    let mut n = 1u32;
    while candidate.exists() {
        candidate = dir.join(format!("{stem} ({n}).avif"));
        n += 1;
    }
    candidate
}

fn convert(image: DynamicImage) -> (Vec<RGBA8>, u32, u32) {
    let rgba = image.to_rgba8();
    let (w, h) = (rgba.width(), rgba.height());
    let pixels = rgba
        .pixels()
        .map(|p| RGBA8::new(p[0], p[1], p[2], p[3]))
        .collect::<Vec<_>>();
    (pixels, w, h)
}

/// 单张：解码 → AVIF 编码 → 写临时文件 → 校验容器头 → 原子改名。
pub fn encode_one(
    src: &Path,
    dir: &Path,
    params: &AvifParams,
) -> Result<(PathBuf, u32, u32, u64), String> {
    if !SUPPORTED.contains(&ext_of(src).as_str()) {
        return Err(format!(
            "不支持的输入格式 .{}：目前支持 {}",
            ext_of(src),
            SUPPORTED.join(" / ")
        ));
    }
    validate(params)?;
    let image = image::open(src).map_err(|e| format!("解码失败：{e}"))?;
    let (pixels, width, height) = convert(image);
    let encoded = Encoder::new()
        .with_quality(params.quality as f32)
        .with_speed(params.speed)
        .encode_rgba(ravif::Img::new(&pixels[..], width as usize, height as usize))
        .map_err(|e| format!("AVIF 编码失败：{e}"))?;
    if !looks_like_avif(&encoded.avif_file) {
        return Err("编码产物不是合法 AVIF 容器（缺少 ftyp/avif 签名）".into());
    }
    let stem = src
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "image".into());
    let out = unique_output(dir, &stem);
    let tmp = dir.join(format!(
        ".avif-tmp-{}.part",
        out.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default()
    ));
    std::fs::write(&tmp, &encoded.avif_file).map_err(|e| format!("写临时文件失败：{e}"))?;
    if out.exists() {
        let _ = std::fs::remove_file(&tmp);
        return Err("输出名已存在，拒绝覆盖".into());
    }
    std::fs::rename(&tmp, &out).map_err(|e| {
        let _ = std::fs::remove_file(&tmp);
        format!("改名失败：{e}")
    })?;
    let bytes = std::fs::metadata(&out).map(|m| m.len()).unwrap_or(0);
    Ok((out, width, height, bytes))
}

pub fn encode_batch(files: &[String], params: &AvifParams, output_dir: Option<&Path>) -> Vec<AvifResult> {
    files
        .iter()
        .map(|src| {
            let source = Path::new(src);
            let dir = match output_dir {
                Some(d) => d.to_path_buf(),
                None => source.parent().map(|p| p.to_path_buf()).unwrap_or_default(),
            };
            if !dir.is_dir() {
                return AvifResult {
                    src: src.clone(),
                    output: None,
                    width: 0,
                    height: 0,
                    bytes: 0,
                    ok: false,
                    message: Some(format!("输出目录不存在：{}", dir.display())),
                };
            }
            match encode_one(source, &dir, params) {
                Ok((out, width, height, bytes)) => AvifResult {
                    src: src.clone(),
                    output: Some(out.to_string_lossy().to_string()),
                    width,
                    height,
                    bytes,
                    ok: true,
                    message: None,
                },
                Err(e) => AvifResult {
                    src: src.clone(),
                    output: None,
                    width: 0,
                    height: 0,
                    bytes: 0,
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

    fn temp_dir(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("avif-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn write_png(path: &Path) {
        let img = image::RgbaImage::from_pixel(8, 8, image::Rgba([200, 60, 30, 255]));
        img.save(path).unwrap();
    }

    #[test]
    fn encodes_real_png_to_avif() {
        let dir = temp_dir("ok");
        let src = dir.join("src.png");
        write_png(&src);
        let (out, w, h, bytes) = encode_one(
            &src,
            &dir,
            &AvifParams {
                quality: 70,
                speed: 8,
            },
        )
        .unwrap();
        assert_eq!((w, h), (8, 8));
        assert!(bytes > 16, "产物太小：{bytes}");
        let data = std::fs::read(&out).unwrap();
        assert!(looks_like_avif(&data), "缺少 ftyp/avif 签名");
        assert!(src.exists(), "源文件必须保留");
    }

    #[test]
    fn rejects_unknown_input_before_work() {
        let dir = temp_dir("ext");
        let src = dir.join("a.mp4");
        std::fs::write(&src, b"x").unwrap();
        let err = encode_one(&src, &dir, &AvifParams::default()).unwrap_err();
        assert!(err.contains("不支持的输入格式"), "{err}");
    }

    #[test]
    fn rejects_quality_out_of_range() {
        let err = validate(&AvifParams {
            quality: 0,
            speed: 6,
        })
        .unwrap_err();
        assert!(err.contains("质量"));
    }

    #[test]
    fn rejects_speed_out_of_range() {
        let err = validate(&AvifParams {
            quality: 70,
            speed: 11,
        })
        .unwrap_err();
        assert!(err.contains("速度"));
    }

    #[test]
    fn unique_name_avoids_existing() {
        let dir = temp_dir("uniq");
        std::fs::write(dir.join("a.avif"), b"x").unwrap();
        assert_eq!(
            unique_output(&dir, "a").file_name().unwrap().to_string_lossy(),
            "a (1).avif"
        );
    }

    #[test]
    fn signature_check_rejects_non_avif() {
        assert!(!looks_like_avif(b"not an avif file at all"));
        assert!(!looks_like_avif(&[0, 0, 0, 0, b'f', b't', b'y', b'p', 1, 2, 3, 4, 5, 6, 7, 8]));
    }
}
