# @formatforge/updater

自动更新适配层（前端包）。本包只保留 `UpdaterAdapter` 契约与 Null 实现；
**应用实际使用的更新器在 `src-tauri/src/updater.rs`**（检查 / 下载 / 校验 / 安装全部在 Rust 内完成），
不经过本包。

## 接口

- `UpdaterAdapter`：`check()` / `downloadAndInstall(info, onProgress)`
- `createNullUpdater(reason)`：明确失败原因
- `createUpdaterController(adapter, { enabled, disabledReason })` → 状态机 `idle / checking / up-to-date / available / downloading / error`

## 现状

- 设置页「诊断与更新」使用 Rust 更新器：多源按序回退、断点续传、大小 + SHA-256（配置公钥时另加
  minisign）校验、Windows 为「检查 + 安装」、macOS 与 Linux deb 为「检查 + 下载」。详见根目录
  `UPDATER.md`。
- 本包的 Null 适配器仍保留，作为契约测试与未来前端接入的占位；应用内未启用。
