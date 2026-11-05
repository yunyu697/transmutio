//! PDF 处理：合并 / 拆分用纯 Rust 的 `lopdf` 在进程内完成（无外部依赖）；
//! PDF → 图片必须调用外部渲染器 `pdftoppm`（Poppler），由 `pdf_tools` 检测，
//! 缺失时入口明确不可用，绝不在点进去之后静默失败。

use lopdf::{Dictionary, Document, Object, ObjectId};
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

/// 读取 PDF 并返回页数；顺带把加密/损坏文件转成可读错误。
pub fn page_count(path: &str) -> Result<usize, String> {
    let doc = load(path)?;
    Ok(doc.get_pages().len())
}

fn load(path: &str) -> Result<Document, String> {
    let doc = Document::load(path).map_err(|e| format!("无法读取 PDF（{}）：{e}", path))?;
    if doc.is_encrypted() {
        return Err(format!(
            "{} 已加密，暂不支持处理加密 PDF（请在源程序中解除密码后重试）",
            Path::new(path)
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_else(|| path.to_string())
        ));
    }
    Ok(doc)
}

/// 合并多个 PDF，输出到 `output`（调用方负责临时文件与原子发布）。
/// 返回合并后的总页数。
pub fn merge(inputs: &[String], output: &Path) -> Result<usize, String> {
    if inputs.len() < 2 {
        return Err("PDF 合并至少需要 2 个文件".into());
    }
    let mut merged = Document::new();
    merged.version = "1.7".into();
    let mut next_id = 1u32;
    let mut page_ids: Vec<ObjectId> = Vec::new();

    for input in inputs {
        let mut doc = load(input)?;
        if doc.get_pages().is_empty() {
            return Err(format!("{} 没有页面，无法合并", input));
        }
        doc.renumber_objects_with(next_id);
        next_id = doc.max_id + 1;
        // 必须在 renumber 之后再取页面 ID，否则拿到的是重编号前的旧引用。
        page_ids.extend(doc.get_pages().values().copied());
        for (id, object) in std::mem::take(&mut doc.objects) {
            merged.objects.insert(id, object);
        }
    }

    let inherited_keys: [&[u8]; 4] = [b"Resources", b"MediaBox", b"CropBox", b"Rotate"];
    for page_id in &page_ids {
        let mut inherited: Vec<(Vec<u8>, Object)> = Vec::new();
        let mut parent = merged
            .objects
            .get(page_id)
            .and_then(|object| object.as_dict().ok())
            .and_then(|dict| dict.get(b"Parent").ok())
            .and_then(|object| object.as_reference().ok());
        while let Some(parent_id) = parent {
            let Some(dict) = merged.objects.get(&parent_id).and_then(|o| o.as_dict().ok()) else {
                break;
            };
            for key in inherited_keys {
                if let Ok(value) = dict.get(key) {
                    if !inherited.iter().any(|(existing, _)| existing.as_slice() == key) {
                        inherited.push((key.to_vec(), value.clone()));
                    }
                }
            }
            parent = dict.get(b"Parent").ok().and_then(|o| o.as_reference().ok());
        }
        if let Some(dict) = merged.objects.get_mut(page_id).and_then(|o| o.as_dict_mut().ok()) {
            for (key, value) in inherited {
                if dict.get(&key).is_err() {
                    dict.set(key, value);
                }
            }
        }
    }

    let mut pages_root = Dictionary::new();
    pages_root.set("Type", Object::Name(b"Pages".to_vec()));
    pages_root.set(
        "Kids",
        Object::Array(page_ids.iter().map(|id| Object::Reference(*id)).collect()),
    );
    pages_root.set("Count", Object::Integer(page_ids.len() as i64));
    let pages_root_id = merged.add_object(Object::Dictionary(pages_root));

    for page_id in &page_ids {
        if let Some(dict) = merged.objects.get_mut(page_id).and_then(|o| o.as_dict_mut().ok()) {
            dict.set("Parent", Object::Reference(pages_root_id));
        }
    }

    let mut catalog = Dictionary::new();
    catalog.set("Type", Object::Name(b"Catalog".to_vec()));
    catalog.set("Pages", Object::Reference(pages_root_id));
    let catalog_id = merged.add_object(Object::Dictionary(catalog));
    merged.trailer.set("Root", Object::Reference(catalog_id));

    // 旧文档的 Catalog/Pages 树已无人引用，清理掉避免合并结果膨胀。
    merged.prune_objects();
    merged.compress();
    merged
        .save(output)
        .map_err(|e| format!("写入合并结果失败（{}）：{e}", output.display()))?;
    Ok(page_ids.len())
}

/// 页码范围：`1-3,5,8-10`。非法（0、倒序、超出总页数、重复）一律拒绝，不静默跳过。
pub fn parse_ranges(spec: &str, total: usize) -> Result<Vec<(u32, u32)>, String> {
    let spec = spec.trim();
    if spec.is_empty() {
        return Err("请填写页码范围，例如 1-3,5".into());
    }
    let mut ranges: Vec<(u32, u32)> = Vec::new();
    let mut seen: BTreeSet<u32> = BTreeSet::new();
    for part in spec.split(',') {
        let part = part.trim();
        if part.is_empty() {
            return Err(format!("页码范围「{spec}」含空段（多余逗号）"));
        }
        let (start, end) = match part.split_once('-') {
            Some((a, b)) => {
                let a = a.trim().parse::<u32>().map_err(|_| format!("无法解析页码「{a}」"))?;
                let b = b.trim().parse::<u32>().map_err(|_| format!("无法解析页码「{b}」"))?;
                (a, b)
            }
            None => {
                let n = part.parse::<u32>().map_err(|_| format!("无法解析页码「{part}」"))?;
                (n, n)
            }
        };
        if start == 0 || end == 0 {
            return Err("页码从 1 开始，不能为 0".into());
        }
        if start > end {
            return Err(format!("页码区间 {start}-{end} 的起始页大于结束页"));
        }
        if end as usize > total {
            return Err(format!("页码 {end} 超出总页数 {total}"));
        }
        for page in start..=end {
            if !seen.insert(page) {
                return Err(format!("页码 {page} 被重复选择"));
            }
            ranges_push(&mut ranges, page);
        }
    }
    // 连续页合并成区间，便于命名与统计。
    let mut merged: Vec<(u32, u32)> = Vec::new();
    for (page, _) in ranges.drain(..) {
        match merged.last_mut() {
            Some(last) if last.1 + 1 == page => last.1 = page,
            _ => merged.push((page, page)),
        }
    }
    Ok(merged)
}

fn ranges_push(ranges: &mut Vec<(u32, u32)>, page: u32) {
    ranges.push((page, page));
}

/// 拆分 PDF：每页一个文件，或按页码范围分段。
/// 返回实际生成的文件列表（调用方负责冲突改名与发布）。
pub fn split(
    input: &str,
    output_dir: &Path,
    stem: &str,
    per_page: bool,
    ranges: Option<&str>,
) -> Result<Vec<PathBuf>, String> {
    let doc = load(input)?;
    let total = doc.get_pages().len();
    if total == 0 {
        return Err("PDF 没有页面，无法拆分".into());
    }
    let groups: Vec<(u32, u32)> = if per_page {
        (1..=total as u32).map(|page| (page, page)).collect()
    } else {
        parse_ranges(ranges.unwrap_or(""), total)?
    };

    std::fs::create_dir_all(output_dir)
        .map_err(|e| format!("无法创建输出目录（{}）：{e}", output_dir.display()))?;
    let mut produced = Vec::new();
    for (start, end) in groups {
        let mut part = doc.clone();
        if start != 1 || end != total as u32 {
            let remove: Vec<u32> = (1..=total as u32)
                .filter(|page| *page < start || *page > end)
                .collect();
            if !remove.is_empty() {
                part.delete_pages(&remove);
            }
        }
        part.prune_objects();
        let name = if start == end {
            format!("{stem}-p{start:04}.pdf")
        } else {
            format!("{stem}-p{start:04}-{end:04}.pdf")
        };
        let path = output_dir.join(name);
        part.save(&path)
            .map_err(|e| format!("写入拆分结果失败（{}）：{e}", path.display()))?;
        produced.push(path);
    }
    Ok(produced)
}

/// 调用 pdftoppm 把 PDF 渲染成图片，返回生成的文件（按页码排序）。
/// `prefix` 是输出前缀（不含扩展名），pdftoppm 会自动追加 `-01.png` 之类。
pub fn rasterize(
    pdftoppm: &str,
    input: &str,
    out_dir: &Path,
    prefix: &str,
    format: &str,
    dpi: u32,
    first: u32,
    last: u32,
) -> Result<Vec<PathBuf>, String> {
    if dpi == 0 {
        return Err("DPI 必须大于 0".into());
    }
    if first == 0 || last < first {
        return Err(format!("页码范围非法：{first}-{last}"));
    }
    std::fs::create_dir_all(out_dir)
        .map_err(|e| format!("无法创建临时目录（{}）：{e}", out_dir.display()))?;
    let mut cmd = std::process::Command::new(pdftoppm);
    cmd.args(["-r", &dpi.to_string()]);
    if format == "jpg" {
        cmd.args(["-jpeg", "-jpegopt", "quality=90"]);
    } else {
        cmd.arg("-png");
    }
    cmd.args(["-f", &first.to_string(), "-l", &last.to_string()]);
    cmd.arg(input);
    cmd.arg(out_dir.join(prefix));
    cmd.stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());
    #[cfg(windows)]
    {
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    let output = cmd
        .output()
        .map_err(|e| format!("无法启动 pdftoppm（{pdftoppm}）：{e}"))?;
    if !output.status.success() {
        let tail = String::from_utf8_lossy(&output.stderr)
            .lines()
            .rev()
            .take(3)
            .collect::<Vec<_>>()
            .join(" · ");
        return Err(if tail.is_empty() {
            format!("pdftoppm 退出码 {:?}", output.status.code())
        } else {
            format!("pdftoppm 渲染失败：{tail}")
        });
    }
    let mut files: Vec<PathBuf> = std::fs::read_dir(out_dir)
        .map_err(|e| format!("无法读取输出目录：{e}"))?
        .filter_map(|entry| entry.ok().map(|e| e.path()))
        .filter(|path| {
            path.file_stem()
                .map(|name| name.to_string_lossy().starts_with(prefix))
                .unwrap_or(false)
                && path.is_file()
        })
        .collect();
    files.sort();
    if files.is_empty() {
        return Err("pdftoppm 没有生成任何图片".into());
    }
    Ok(files)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn temp_dir(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "transmutio-pdf-{tag}-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_millis())
                .unwrap_or(0)
        ));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// 手写一个最小 PDF（未压缩内容流），避免测试依赖外部 PDF 生成工具。
    fn minimal_pdf(pages: usize) -> Vec<u8> {
        let mut out = b"%PDF-1.4\n".to_vec();
        let mut offsets: Vec<usize> = Vec::new();
        offsets.push(out.len());
        out.extend(b"1 0 obj\n<< /Type /Catalog /Pages 2 0 R >>\nendobj\n");
        let kids: Vec<String> = (0..pages).map(|i| format!("{} 0 R", 3 + i)).collect();
        offsets.push(out.len());
        out.extend(
            format!(
                "2 0 obj\n<< /Type /Pages /Kids [{}] /Count {} >>\nendobj\n",
                kids.join(" "),
                pages
            )
            .as_bytes(),
        );
        for i in 0..pages {
            offsets.push(out.len());
            out.extend(
                format!(
                    "{} 0 obj\n<< /Type /Page /Parent 2 0 R /MediaBox [0 0 200 200] >>\nendobj\n",
                    3 + i
                )
                .as_bytes(),
            );
        }
        let xref = out.len();
        out.extend(format!("xref\n0 {}\n0000000000 65535 f \n", pages + 3).as_bytes());
        for offset in offsets {
            out.extend(format!("{offset:010} 00000 n \n").as_bytes());
        }
        out.extend(
            format!(
                "trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n",
                pages + 3
            )
            .as_bytes(),
        );
        out
    }

    #[test]
    fn merge_counts_pages_of_all_inputs() {
        let dir = temp_dir("merge");
        let a = dir.join("a.pdf");
        let b = dir.join("b.pdf");
        fs::write(&a, minimal_pdf(2)).unwrap();
        fs::write(&b, minimal_pdf(3)).unwrap();
        let out = dir.join("merged.pdf");
        let total = merge(&[a.to_string_lossy().to_string(), b.to_string_lossy().to_string()], &out).unwrap();
        assert_eq!(total, 5);
        assert_eq!(page_count(&out.to_string_lossy()).unwrap(), 5);
        let size = fs::metadata(&out).unwrap().len();
        assert!(size > 100, "合并结果应为非空 PDF，实际 {size} 字节");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn merge_rejects_single_input() {
        let err = merge(&[], Path::new("out.pdf")).unwrap_err();
        assert!(err.contains("至少需要 2 个文件"), "{err}");
        let one = vec!["only.pdf".to_string()];
        let err = merge(&one, Path::new("out.pdf")).unwrap_err();
        assert!(err.contains("至少需要 2 个文件"), "{err}");
    }

    #[test]
    fn split_per_page_and_ranges() {
        let dir = temp_dir("split");
        let input = dir.join("src.pdf");
        fs::write(&input, minimal_pdf(5)).unwrap();
        let input_s = input.to_string_lossy().to_string();

        let out_dir = dir.join("per");
        let files = split(&input_s, &out_dir, "doc", true, None).unwrap();
        assert_eq!(files.len(), 5);
        for file in &files {
            assert_eq!(page_count(&file.to_string_lossy()).unwrap(), 1);
        }

        let out_dir = dir.join("ranges");
        let files = split(&input_s, &out_dir, "doc", false, Some("1-2,4")).unwrap();
        assert_eq!(files.len(), 2);
        assert_eq!(page_count(&files[0].to_string_lossy()).unwrap(), 2);
        assert_eq!(page_count(&files[1].to_string_lossy()).unwrap(), 1);
        assert!(files[0].file_name().unwrap().to_string_lossy().contains("p0001-0002"));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn range_parser_rejects_invalid_specs() {
        assert!(parse_ranges("", 5).unwrap_err().contains("请填写"));
        assert!(parse_ranges("0-2", 5).unwrap_err().contains("从 1 开始"));
        assert!(parse_ranges("4-2", 5).unwrap_err().contains("起始页大于结束页"));
        assert!(parse_ranges("1-9", 5).unwrap_err().contains("超出总页数"));
        assert!(parse_ranges("1,1", 5).unwrap_err().contains("重复"));
        assert!(parse_ranges("1,,2", 5).unwrap_err().contains("空段"));
        assert!(parse_ranges("abc", 5).unwrap_err().contains("无法解析"));
        assert_eq!(parse_ranges("1-2,4", 5).unwrap(), vec![(1, 2), (4, 4)]);
        assert_eq!(parse_ranges("3,1-2", 5).unwrap(), vec![(3, 3), (1, 2)]);
    }
}
