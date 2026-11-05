//! 旧版 `FormatForge` → `Transmutio` 的数据迁移与手动导入。
//!
//! 迁移算法（两个目录各自独立执行，同变体对同变体）：
//! 1. 旧目录存在、新目录不存在（或为空）；
//! 2. 在**同卷**目标父目录创建临时目录；
//! 3. 逐文件复制，保留空目录，拒绝符号链接/重解析点；
//! 4. 重新枚举临时目录，逐项核对文件数、目录数、字节数与每个文件的 SHA-256；
//! 5. `rename(临时目录, 新目录)` 原子落位；
//! 6. 旧目录原样保留，新目录写 `migrated-from.txt`。
//!
//! 失败语义：只删除临时目录；旧目录不删不改；正式新目录绝不留半套。

use crate::flavor::Flavor;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

pub const MIGRATION_MARKER: &str = "migration.json";
pub const MIGRATION_LOG: &str = "migration.log";
pub const IMPORT_PENDING_MARKER: &str = "import-pending.json";
pub const LEGACY_WEBVIEW_STAGING: &str = "EBWebView.legacy-import";

/// 单个目录的迁移结果。`status`：migrated / target-present / source-missing / failed。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct DirOutcome {
    pub source: String,
    pub target: String,
    pub status: String,
    pub files: usize,
    pub dirs: usize,
    pub bytes: u64,
    pub detail: String,
}

/// 写进新应用数据目录 `migration.json` 的一次性提示数据。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct MigrationRecord {
    pub at: u64,
    pub migrated: bool,
    pub detail: String,
    pub sources: Vec<String>,
    pub app_data: DirOutcome,
    pub webview: DirOutcome,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ImportSummary {
    pub files: usize,
    pub bytes: u64,
    pub backup_app: String,
    pub backup_webview: Option<String>,
    pub detail: String,
}

/// 当前变体在真实机器上的四类目录（旧/新 × 应用数据/WebView2）。
#[derive(Debug, Clone)]
pub struct VariantPaths {
    pub old_app: PathBuf,
    pub new_app: PathBuf,
    pub old_local: PathBuf,
    pub new_local: PathBuf,
}

impl VariantPaths {
    pub fn for_variant(flavor: Flavor, appdata: &Path, local: &Path) -> Self {
        Self {
            old_app: appdata.join(flavor.legacy_app_identifier()),
            new_app: appdata.join(flavor.app_identifier()),
            old_local: local.join(flavor.legacy_app_identifier()),
            new_local: local.join(flavor.app_identifier()),
        }
    }

    pub fn available(&self) -> bool {
        self.old_app.exists() || self.old_local.exists()
    }
}

/// 从环境变量推导当前变体的目录；非 Windows 平台返回 None（迁移只针对 Windows 安装包）。
pub fn current_paths(flavor: Flavor) -> Option<VariantPaths> {
    #[cfg(windows)]
    {
        let appdata = PathBuf::from(std::env::var_os("APPDATA")?);
        let local = PathBuf::from(std::env::var_os("LOCALAPPDATA")?);
        Some(VariantPaths::for_variant(flavor, &appdata, &local))
    }
    #[cfg(not(windows))]
    {
        let _ = flavor;
        None
    }
}

#[derive(Debug, Default)]
struct Snapshot {
    files: usize,
    dirs: usize,
    bytes: u64,
    entries: BTreeMap<String, EntryMeta>,
}

#[derive(Debug)]
struct EntryMeta {
    dir: bool,
    size: u64,
    sha256: String,
}

fn now_stamp() -> String {
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    format!("{secs}-{}", std::process::id())
}

fn rel_key(path: &Path, root: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .components()
        .map(|c| c.as_os_str().to_string_lossy().to_string())
        .collect::<Vec<_>>()
        .join("/")
}

fn hash_file(path: &Path) -> Result<String, String> {
    let mut file =
        fs::File::open(path).map_err(|e| format!("打开 {} 失败：{e}", path.display()))?;
    let mut hasher = Sha256::new();
    let mut buf = vec![0u8; 1024 * 1024];
    loop {
        let n = file
            .read(&mut buf)
            .map_err(|e| format!("读取 {} 失败：{e}", path.display()))?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(format!("{:x}", hasher.finalize()))
}

fn snapshot(root: &Path) -> Result<Snapshot, String> {
    let mut snap = Snapshot::default();
    walk(root, root, &mut snap)?;
    Ok(snap)
}

fn walk(dir: &Path, root: &Path, snap: &mut Snapshot) -> Result<(), String> {
    let entries = fs::read_dir(dir).map_err(|e| format!("读取目录 {} 失败：{e}", dir.display()))?;
    for entry in entries {
        let entry = entry.map_err(|e| format!("遍历 {} 失败：{e}", dir.display()))?;
        let path = entry.path();
        let file_type = entry
            .file_type()
            .map_err(|e| format!("读取类型 {} 失败：{e}", path.display()))?;
        if file_type.is_symlink() {
            return Err(format!(
                "检测到符号链接/重解析点，拒绝迁移：{}",
                path.display()
            ));
        }
        let key = rel_key(&path, root);
        if file_type.is_dir() {
            snap.dirs += 1;
            snap.entries.insert(
                key,
                EntryMeta {
                    dir: true,
                    size: 0,
                    sha256: String::new(),
                },
            );
            walk(&path, root, snap)?;
        } else if file_type.is_file() {
            let size = entry
                .metadata()
                .map_err(|e| format!("读取大小 {} 失败：{e}", path.display()))?
                .len();
            let sha256 = hash_file(&path)?;
            snap.files += 1;
            snap.bytes += size;
            snap.entries.insert(
                key,
                EntryMeta {
                    dir: false,
                    size,
                    sha256,
                },
            );
        } else {
            return Err(format!("不是常规文件或目录，拒绝迁移：{}", path.display()));
        }
    }
    Ok(())
}

fn copy_tree(src: &Path, dst: &Path, fail_after: Option<usize>) -> Result<(), String> {
    let mut copied = 0usize;
    copy_recursive(src, dst, &mut copied, fail_after)
}

fn copy_recursive(
    src: &Path,
    dst: &Path,
    copied: &mut usize,
    fail_after: Option<usize>,
) -> Result<(), String> {
    fs::create_dir_all(dst).map_err(|e| format!("创建 {} 失败：{e}", dst.display()))?;
    let entries = fs::read_dir(src).map_err(|e| format!("读取目录 {} 失败：{e}", src.display()))?;
    for entry in entries {
        let entry = entry.map_err(|e| format!("遍历 {} 失败：{e}", src.display()))?;
        let path = entry.path();
        let file_type = entry
            .file_type()
            .map_err(|e| format!("读取类型 {} 失败：{e}", path.display()))?;
        if file_type.is_symlink() {
            return Err(format!(
                "检测到符号链接/重解析点，拒绝迁移：{}",
                path.display()
            ));
        }
        let target = dst.join(entry.file_name());
        if file_type.is_dir() {
            copy_recursive(&path, &target, copied, fail_after)?;
        } else if file_type.is_file() {
            if let Some(limit) = fail_after {
                if *copied >= limit {
                    return Err("注入的复制失败（测试用）".into());
                }
            }
            fs::copy(&path, &target)
                .map_err(|e| format!("复制 {} → {} 失败：{e}", path.display(), target.display()))?;
            *copied += 1;
        } else {
            return Err(format!("不是常规文件或目录，拒绝迁移：{}", path.display()));
        }
    }
    Ok(())
}

fn verify(source: &Snapshot, copy: &Snapshot) -> Result<(), String> {
    if source.files != copy.files {
        return Err(format!(
            "文件数不一致：源 {} / 副本 {}",
            source.files, copy.files
        ));
    }
    if source.dirs != copy.dirs {
        return Err(format!(
            "目录数不一致：源 {} / 副本 {}",
            source.dirs, copy.dirs
        ));
    }
    if source.bytes != copy.bytes {
        return Err(format!(
            "字节数不一致：源 {} / 副本 {}",
            source.bytes, copy.bytes
        ));
    }
    if source.entries.len() != copy.entries.len() {
        return Err("目录结构条目数不一致".into());
    }
    for (key, expected) in &source.entries {
        let actual = copy
            .entries
            .get(key)
            .ok_or_else(|| format!("副本缺少条目：{key}"))?;
        if expected.dir != actual.dir {
            return Err(format!("条目类型不一致：{key}"));
        }
        if !expected.dir {
            if expected.size != actual.size {
                return Err(format!(
                    "文件大小不一致：{key}（{} / {}）",
                    expected.size, actual.size
                ));
            }
            if expected.sha256 != actual.sha256 {
                return Err(format!("文件哈希不一致：{key}"));
            }
        }
    }
    Ok(())
}

#[derive(Debug, Clone, Copy, Default)]
struct MigrateOptions {
    /// 测试注入：复制到第 N 个文件后主动失败。
    fail_after: Option<usize>,
    /// 测试注入：复制完成后篡改副本，触发校验失败。
    corrupt_copy: bool,
}

fn corrupt_first_file(dir: &Path) -> Result<(), String> {
    let entries = fs::read_dir(dir).map_err(|e| format!("读取 {} 失败：{e}", dir.display()))?;
    for entry in entries {
        let entry = entry.map_err(|e| e.to_string())?;
        let path = entry.path();
        if path.is_dir() {
            corrupt_first_file(&path)?;
            return Ok(());
        }
        if path.is_file() {
            use std::io::Write;
            let mut file = fs::OpenOptions::new()
                .append(true)
                .open(&path)
                .map_err(|e| format!("打开 {} 失败：{e}", path.display()))?;
            file.write_all(b"x").map_err(|e| e.to_string())?;
            return Ok(());
        }
    }
    Ok(())
}

fn is_empty_dir(path: &Path) -> bool {
    fs::read_dir(path)
        .map(|mut it| it.next().is_none())
        .unwrap_or(false)
}

fn outcome(old: &Path, new: &Path, status: &str, detail: String) -> DirOutcome {
    DirOutcome {
        source: old.to_string_lossy().to_string(),
        target: new.to_string_lossy().to_string(),
        status: status.into(),
        files: 0,
        dirs: 0,
        bytes: 0,
        detail,
    }
}

/// 生产入口：迁移一个目录。
pub fn migrate_dir(old: &Path, new: &Path) -> DirOutcome {
    migrate_dir_with(old, new, MigrateOptions::default())
}

fn migrate_dir_with(old: &Path, new: &Path, options: MigrateOptions) -> DirOutcome {
    if !old.exists() {
        return outcome(old, new, "source-missing", "旧目录不存在，无需迁移".into());
    }
    if new.exists() {
        if !is_empty_dir(new) {
            return outcome(
                old,
                new,
                "target-present",
                "新目录已存在（非空），按已迁移处理，不覆盖".into(),
            );
        }
        // 空目录可以是上一次失败留下的；删掉后用原子 rename 落位。
        if let Err(e) = fs::remove_dir_all(new) {
            return outcome(old, new, "failed", format!("清理空的新目录失败：{e}"));
        }
    }
    let Some(parent) = new.parent() else {
        return outcome(old, new, "failed", "无法定位新目录的父目录".into());
    };
    if let Err(e) = fs::create_dir_all(parent) {
        return outcome(
            old,
            new,
            "failed",
            format!("创建父目录 {} 失败：{e}", parent.display()),
        );
    }
    let temp = parent.join(format!(".transmutio-migrate-{}", now_stamp()));

    let result = (|| -> Result<Snapshot, String> {
        let source = snapshot(old)?;
        fs::create_dir_all(&temp).map_err(|e| format!("创建临时目录失败：{e}"))?;
        copy_tree(old, &temp, options.fail_after)?;
        if options.corrupt_copy {
            corrupt_first_file(&temp)?;
        }
        let copy = snapshot(&temp)?;
        verify(&source, &copy)?;
        fs::rename(&temp, new)
            .map_err(|e| format!("原子落位 {} → {} 失败：{e}", temp.display(), new.display()))?;
        Ok(source)
    })();

    match result {
        Ok(source) => {
            let body = format!(
                "source={}\ntarget={}\nfiles={}\ndirs={}\nbytes={}\nmigratedAt={}\n",
                old.display(),
                new.display(),
                source.files,
                source.dirs,
                source.bytes,
                now_stamp()
            );
            let _ = fs::write(new.join("migrated-from.txt"), body);
            DirOutcome {
                source: old.to_string_lossy().to_string(),
                target: new.to_string_lossy().to_string(),
                status: "migrated".into(),
                files: source.files,
                dirs: source.dirs,
                bytes: source.bytes,
                detail: format!(
                    "复制并校验通过：{} 个文件 / {} 个目录 / {} 字节；旧目录保留",
                    source.files, source.dirs, source.bytes
                ),
            }
        }
        Err(error) => {
            // 失败只清临时目录；旧目录不动，正式新目录不出现。
            let _ = fs::remove_dir_all(&temp);
            outcome(old, new, "failed", error)
        }
    }
}

fn append_log(new_app: &Path, record: &MigrationRecord) {
    use std::io::Write;
    let _ = fs::create_dir_all(new_app);
    if let Ok(mut file) = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(new_app.join(MIGRATION_LOG))
    {
        let _ = writeln!(
            file,
            "[{}] appData={} ({}) webview={} ({}) {}",
            record.at,
            record.app_data.status,
            record.app_data.files,
            record.webview.status,
            record.webview.files,
            record.detail
        );
        let _ = file.sync_all();
    }
}

fn write_log_to_temp(message: &str) {
    use std::io::Write;
    let _ = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(std::env::temp_dir().join("transmutio-migration.log"))
        .and_then(|mut f| writeln!(f, "[{}] {message}", now_stamp()));
}

/// 启动早期（WebView2 创建之前）执行迁移。返回写进 `migration.json` 的记录（若有迁移发生）。
pub fn run_prelaunch(paths: &VariantPaths) -> Option<MigrationRecord> {
    apply_pending_import(paths);

    let app_data = migrate_dir(&paths.old_app, &paths.new_app);
    if app_data.status == "failed" {
        write_log_to_temp(&format!(
            "应用数据迁移失败：{}（{}）",
            app_data.source, app_data.detail
        ));
        return Some(MigrationRecord {
            at: epoch(),
            migrated: false,
            detail: format!("旧版数据迁移失败：{}", app_data.detail),
            sources: vec![app_data.source.clone()],
            app_data,
            webview: outcome(
                &paths.old_local,
                &paths.new_local,
                "skipped",
                "应用数据迁移失败，未迁移 WebView2".into(),
            ),
        });
    }

    let webview = migrate_dir(&paths.old_local, &paths.new_local);
    if app_data.status != "migrated" && webview.status != "migrated" {
        return None;
    }

    let mut sources = Vec::new();
    if app_data.status == "migrated" {
        sources.push(app_data.source.clone());
    }
    if webview.status == "migrated" {
        sources.push(webview.source.clone());
    }
    let detail = if webview.status == "migrated" {
        format!(
            "已从旧版 FormatForge 迁移数据：应用数据 {} 个文件，设置与面板参数已随 WebView2 数据迁移。旧目录仍保留。",
            app_data.files
        )
    } else {
        format!(
            "已从旧版 FormatForge 迁移应用数据（{} 个文件）；WebView2 数据未迁移（{}）。",
            app_data.files, webview.detail
        )
    };
    let record = MigrationRecord {
        at: epoch(),
        migrated: true,
        detail,
        sources,
        app_data,
        webview,
    };
    if let Err(e) = fs::create_dir_all(&paths.new_app) {
        write_log_to_temp(&format!("写入迁移记录失败：{e}"));
        return Some(record);
    }
    if let Err(e) = fs::write(
        paths.new_app.join(MIGRATION_MARKER),
        serde_json::to_string_pretty(&record).unwrap_or_default(),
    ) {
        write_log_to_temp(&format!("写入 migration.json 失败：{e}"));
    }
    append_log(&paths.new_app, &record);
    Some(record)
}

fn epoch() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// 下次启动时应用「手动导入」暂存的 WebView2 数据（必须在 WebView2 创建前调用）。
fn apply_pending_import(paths: &VariantPaths) -> Option<String> {
    let marker = paths.new_app.join(IMPORT_PENDING_MARKER);
    let staging = paths.new_local.join(LEGACY_WEBVIEW_STAGING);
    if !marker.is_file() || !staging.is_dir() {
        return None;
    }
    let current = paths.new_local.join("EBWebView");
    let stamp = now_stamp();
    if current.exists() {
        let backup = paths.new_local.join(format!("EBWebView.bak-{stamp}"));
        if fs::rename(&current, &backup).is_err() {
            return None;
        }
    }
    if let Err(e) = fs::rename(&staging, &current) {
        write_log_to_temp(&format!("应用旧版 WebView2 数据失败：{e}"));
        return None;
    }
    let _ = fs::remove_file(&marker);
    let _ = fs::create_dir_all(&paths.new_app);
    append_log(
        &paths.new_app,
        &MigrationRecord {
            at: epoch(),
            migrated: true,
            detail: "已应用手动导入的旧版 WebView2 数据（设置与面板参数）".into(),
            sources: vec![paths.old_local.to_string_lossy().to_string()],
            app_data: outcome(
                &paths.old_app,
                &paths.new_app,
                "imported",
                "手动导入已落位".into(),
            ),
            webview: outcome(
                &paths.old_local,
                &paths.new_local,
                "imported",
                "手动导入已落位".into(),
            ),
        },
    );
    Some("已应用旧版 WebView2 数据".into())
}

/// 手动导入：整份备份当前数据 → 合并旧应用数据 → 暂存旧 WebView2（下次启动生效）。
pub fn import_legacy_data(flavor: Flavor, paths: &VariantPaths) -> Result<ImportSummary, String> {
    if !paths.available() {
        return Err("未找到旧版 FormatForge 数据目录".into());
    }
    let stamp = now_stamp();
    let appdata = paths
        .new_app
        .parent()
        .ok_or_else(|| "无法定位应用数据父目录".to_string())?;

    // 1) 整份备份当前应用数据（备份目录与目标同级，避免递归复制）。
    let backup_app = appdata.join(format!(".transmutio-backup-{stamp}-{}", flavor.id()));
    let mut backup_bytes = 0u64;
    if paths.new_app.exists() {
        copy_tree(&paths.new_app, &backup_app, None)?;
        backup_bytes = snapshot(&paths.new_app).map(|s| s.bytes).unwrap_or(0);
    }

    // 2) 合并旧应用数据（覆盖同名文件；当前内容已在上面整份备份）。
    let mut files = 0usize;
    let mut bytes = 0u64;
    if paths.old_app.exists() {
        copy_tree(&paths.old_app, &paths.new_app, None)?;
        if let Ok(snap) = snapshot(&paths.old_app) {
            files = snap.files;
            bytes = snap.bytes;
        }
    }

    // 3) 暂存旧 WebView2 数据：不在运行中替换活动目录，下次启动前原子应用。
    let mut backup_webview = None;
    if paths.old_local.exists() {
        let staging = paths.new_local.join(LEGACY_WEBVIEW_STAGING);
        let _ = fs::remove_dir_all(&staging);
        // WebView2 的根是 `<identifier>\EBWebView`；暂存目录要直接对应 EBWebView 本体，
        // 这样下次启动才能把 staging 原子 rename 成 `EBWebView`（而不是套一层）。
        let source_webview = paths.old_local.join("EBWebView");
        let source_root = if source_webview.is_dir() {
            source_webview
        } else {
            paths.old_local.clone()
        };
        copy_tree(&source_root, &staging, None)?;
        let current = paths.new_local.join("EBWebView");
        if current.exists() {
            let backup = paths
                .new_local
                .join(format!("EBWebView.import-bak-{stamp}"));
            copy_tree(&current, &backup, None)?;
            backup_webview = Some(backup.to_string_lossy().to_string());
        }
        fs::create_dir_all(&paths.new_app).ok();
        let marker = serde_json::json!({
            "at": epoch(),
            "source": paths.old_local.to_string_lossy(),
            "staged": staging.to_string_lossy(),
        });
        fs::write(
            paths.new_app.join(IMPORT_PENDING_MARKER),
            serde_json::to_string_pretty(&marker).unwrap_or_default(),
        )
        .map_err(|e| format!("写入导入标记失败：{e}"))?;
    }

    let detail = "已整份备份当前数据并导入旧版数据；设置与面板参数将在下次启动生效。".to_string();
    let summary = ImportSummary {
        files,
        bytes,
        backup_app: backup_app.to_string_lossy().to_string(),
        backup_webview,
        detail,
    };
    append_log(
        &paths.new_app,
        &MigrationRecord {
            at: epoch(),
            migrated: true,
            detail: format!(
                "手动导入：{} 个文件 / {} 字节；备份 {}（{} 字节）",
                summary.files, summary.bytes, summary.backup_app, backup_bytes
            ),
            sources: vec![paths.old_app.to_string_lossy().to_string()],
            app_data: outcome(
                &paths.old_app,
                &paths.new_app,
                "imported",
                "手动导入合并完成".into(),
            ),
            webview: outcome(
                &paths.old_local,
                &paths.new_local,
                "imported",
                "旧 WebView2 数据已暂存".into(),
            ),
        },
    );
    Ok(summary)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_root(tag: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("transmutio-migration-test-{tag}-{}", now_stamp()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn write(path: &Path, text: &str) {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(path, text).unwrap();
    }

    #[test]
    fn migrate_tree_copies_and_verifies_while_keeping_source() {
        let root = temp_root("ok");
        let old = root.join("old");
        let new = root.join("new");
        write(
            &old.join("settings.json"),
            r#"{"theme":"dark","concurrency":4}"#,
        );
        write(
            &old.join("tools/ffmpeg/bin/ffmpeg.txt"),
            "fake-binary-content",
        );
        fs::create_dir_all(old.join("logs/empty")).unwrap();
        let before = snapshot(&old).unwrap();

        let result = migrate_dir(&old, &new);
        assert_eq!(result.status, "migrated", "{result:?}");
        assert_eq!(result.files, 2);
        assert_eq!(result.dirs, 5); // tools / tools/ffmpeg / tools/ffmpeg/bin / logs / logs/empty
        assert!(new.join("settings.json").exists());
        assert!(new.join("logs/empty").is_dir(), "空目录必须保留");
        assert!(new.join("migrated-from.txt").exists());
        assert!(old.join("settings.json").exists(), "旧目录必须保留");
        assert_eq!(snapshot(&old).unwrap().bytes, before.bytes);
        assert_eq!(snapshot(&new.join("tools")).unwrap().files, 1);

        // 新目录里的配置能被正常读取（模拟应用读取设置）。
        let text = fs::read_to_string(new.join("settings.json")).unwrap();
        assert!(text.contains("concurrency"));
    }

    #[test]
    fn second_run_is_idempotent_and_keeps_source() {
        let root = temp_root("idem");
        let old = root.join("old");
        let new = root.join("new");
        write(&old.join("a.txt"), "hello");
        assert_eq!(migrate_dir(&old, &new).status, "migrated");
        let again = migrate_dir(&old, &new);
        assert_eq!(again.status, "target-present", "{again:?}");
        assert!(old.join("a.txt").exists());
        assert_eq!(fs::read_to_string(new.join("a.txt")).unwrap(), "hello");
    }

    #[test]
    fn copy_failure_leaves_old_untouched_and_no_new_dir() {
        let root = temp_root("copyfail");
        let old = root.join("old");
        let new = root.join("new");
        write(&old.join("one.txt"), "1");
        write(&old.join("two.txt"), "2");
        write(&old.join("three.txt"), "3");
        let before = snapshot(&old).unwrap();
        let result = migrate_dir_with(
            &old,
            &new,
            MigrateOptions {
                fail_after: Some(1),
                corrupt_copy: false,
            },
        );
        assert_eq!(result.status, "failed");
        assert!(!new.exists(), "失败时不能留下正式新目录");
        assert_eq!(
            snapshot(&old).unwrap().bytes,
            before.bytes,
            "旧目录字节数不变"
        );
        assert_eq!(fs::read_to_string(old.join("three.txt")).unwrap(), "3");
        let leftovers: Vec<_> = fs::read_dir(&root)
            .unwrap()
            .flatten()
            .filter(|e| {
                e.file_name()
                    .to_string_lossy()
                    .starts_with(".transmutio-migrate-")
            })
            .collect();
        assert!(leftovers.is_empty(), "临时目录必须清理");
    }

    #[test]
    fn verification_failure_cleans_temp_and_leaves_no_new_dir() {
        let root = temp_root("verifyfail");
        let old = root.join("old");
        let new = root.join("new");
        write(&old.join("cfg.json"), "{\"x\":1}");
        let result = migrate_dir_with(
            &old,
            &new,
            MigrateOptions {
                fail_after: None,
                corrupt_copy: true,
            },
        );
        assert_eq!(result.status, "failed");
        assert!(
            result.detail.contains("哈希")
                || result.detail.contains("大小")
                || result.detail.contains("字节"),
            "{}",
            result.detail
        );
        assert!(!new.exists(), "校验失败时不能留下正式新目录");
        assert_eq!(
            fs::read_to_string(old.join("cfg.json")).unwrap(),
            "{\"x\":1}"
        );
        let leftovers: Vec<_> = fs::read_dir(&root)
            .unwrap()
            .flatten()
            .filter(|e| {
                e.file_name()
                    .to_string_lossy()
                    .starts_with(".transmutio-migrate-")
            })
            .collect();
        assert!(leftovers.is_empty(), "临时目录必须清理");
    }

    #[test]
    fn variants_use_scoped_legacy_paths() {
        let root = temp_root("scope");
        let appdata = root.join("Roaming");
        let local = root.join("Local");
        let lgpl = VariantPaths::for_variant(Flavor::Lgpl, &appdata, &local);
        let gpl = VariantPaths::for_variant(Flavor::Gpl, &appdata, &local);
        assert_eq!(lgpl.old_app, appdata.join("app.formatforge.desktop"));
        assert_eq!(lgpl.new_app, appdata.join("app.transmutio.desktop"));
        assert_eq!(gpl.old_app, appdata.join("app.formatforge.desktop.gpl"));
        assert_ne!(lgpl.old_app, gpl.old_app, "跨变体不能共用旧目录");
        assert_eq!(lgpl.old_local, local.join("app.formatforge.desktop"));
    }

    #[test]
    fn manual_import_backs_up_and_defers_webview_swap() {
        let root = temp_root("manual");
        let appdata = root.join("Roaming");
        let local = root.join("Local");
        let paths = VariantPaths::for_variant(Flavor::Lgpl, &appdata, &local);
        write(
            &paths.old_app.join("settings.json"),
            "{\"theme\":\"light\"}",
        );
        write(&paths.old_app.join("tools/ffmpeg.txt"), "old-tools");
        write(&paths.new_app.join("settings.json"), "{\"theme\":\"dark\"}");
        write(
            &paths
                .old_local
                .join("EBWebView/Default/Local Storage/leveldb/settings.ldb"),
            "legacy-localstorage",
        );
        write(
            &paths
                .new_local
                .join("EBWebView/Default/Local Storage/leveldb/settings.ldb"),
            "current",
        );

        let summary = import_legacy_data(Flavor::Lgpl, &paths).expect("导入应成功");
        assert!(
            Path::new(&summary.backup_app).exists(),
            "必须备份当前应用数据"
        );
        assert!(summary
            .backup_webview
            .as_deref()
            .map(|p| Path::new(p).exists())
            .unwrap_or(false));
        assert_eq!(
            fs::read_to_string(paths.new_app.join("settings.json")).unwrap(),
            "{\"theme\":\"light\"}"
        );
        assert!(paths.new_app.join("tools/ffmpeg.txt").exists());
        let staged = paths.new_local.join(LEGACY_WEBVIEW_STAGING);
        assert!(staged
            .join("Default/Local Storage/leveldb/settings.ldb")
            .exists());
        assert!(paths.new_app.join(IMPORT_PENDING_MARKER).is_file());

        // 下次启动早期：暂存目录原子换入，旧活动目录留作备份。
        let applied = apply_pending_import(&paths);
        assert!(applied.is_some());
        assert_eq!(
            fs::read_to_string(
                paths
                    .new_local
                    .join("EBWebView/Default/Local Storage/leveldb/settings.ldb")
            )
            .unwrap(),
            "legacy-localstorage"
        );
        assert!(!paths.new_app.join(IMPORT_PENDING_MARKER).exists());
        let backups: Vec<_> = fs::read_dir(&paths.new_local)
            .unwrap()
            .flatten()
            .filter(|e| {
                e.file_name()
                    .to_string_lossy()
                    .starts_with("EBWebView.bak-")
            })
            .collect();
        assert_eq!(backups.len(), 1, "被替换的 WebView2 目录必须留下备份");
    }
}
