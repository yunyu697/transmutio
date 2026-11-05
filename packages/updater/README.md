# @formatforge/updater

自动更新适配层。当前为 **契约 + 空实现**，未接入真实更新服务（如实标注，不假装可用）。

## 接口

- `UpdaterAdapter`：`check()` / `downloadAndInstall(info, onProgress)`
- `createNullUpdater(reason)`：明确失败原因，UI 显示"未启用"
- `createUpdaterController(adapter, { enabled, disabledReason })` → 状态机 `idle / checking / up-to-date / available / downloading / error`

## 后续接入步骤（未执行）

1. 在 `src-tauri` 增加 `tauri-plugin-updater`，配置 `plugins.updater.endpoints` + `pubkey`。
2. 在 `src/lib/updater.ts` 用 `@tauri-apps/plugin-updater` 实现 `UpdaterAdapter`。
3. 三个平台产出安装包后上传到更新端点，并在 `tauri.conf.json` 里填写公钥。
4. macOS 需签名 + 公证的 `.app`/`.dmg`；Windows 需签名的 `.msi`/`.exe`，否则更新校验会失败。

签名密钥与端点均未配置，因此设置页"检查更新"按钮当前给出明确提示而不是报成功。
