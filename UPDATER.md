# Transmutio 更新通道方案（本轮只设计，不启用）

> 状态：**方案文档，未接入代码、未生成密钥、未启用更新检查**。
> 现有 `packages/updater` 只有占位实现，发布流程仍以手动安装包为准。

## 1. 两类密钥，不要混为一谈

| 用途 | 格式 | 作用 | 是否本轮生成 |
| --- | --- | --- | --- |
| Tauri updater 签名密钥 | minisign（`.pub` / `.key`） | 给 `latest.json` 与更新包签名，客户端校验后才安装 | **否** |
| 代码签名证书 | Windows Authenticode（`.pfx`）/ macOS Developer ID | 消除 SmartScreen / Gatekeeper 警告，证明发布者身份 | **否** |

更新包签名**不等于**操作系统信任：没有 Authenticode 的包能通过 updater 校验，但仍会弹 SmartScreen。

生成 updater 密钥（将来执行，本轮不做）：

```bash
# 私钥写入本地安全位置（绝不入库；.gitignore 已排除 *.key / *.pem / .tauri-signing/）
npx tauri signer generate -- -w ~/.tauri/transmutio-updater.key

# 输出：公钥（写入 tauri.conf.json 的 plugins.updater.pubkey）
#       私钥密码短语（交给 CI secret，不进仓库）
```

CI 中应以 `TAURI_SIGNING_PRIVATE_KEY` + `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` 注入，
并把公钥提交到 `tauri.conf.json` / `tauri.gpl.conf.json` 的 `plugins.updater.pubkey`。

## 2. 更新清单 `latest.json`

```json
{
  "version": "0.2.0",
  "notes": "修复……",
  "pub_date": "2026-11-06T00:00:00Z",
  "platforms": {
    "windows-x86_64": { "signature": "<minisign 签名>", "url": "https://.../Transmutio_0.2.0_x64-setup.exe" },
    "windows-x86_64-gpl": { "signature": "...", "url": ".../Transmutio GPL_0.2.0_x64-setup.exe" },
    "darwin-aarch64": { "signature": "...", "url": ".../Transmutio_0.2.0_aarch64.app.tar.gz" },
    "darwin-x86_64": { "signature": "...", "url": "..." },
    "linux-x86_64": { "signature": "...", "url": ".../Transmutio_0.2.0_amd64.AppImage" }
  }
}
```

约定：

- **变体隔离**：LGPL 与 GPL 使用不同更新通道（`transmutio-lgpl-stable` / `transmutio-gpl-stable`），
  平台键带 `-gpl` 后缀或在独立清单中发布，**绝不允许 LGPL 客户端收到 GPL 包**。
- **最低支持版本**：`minimumVersion`（客户端低于该版本时强制全量更新，不再做增量）。
- **清单与安装包同源**：两者放同一 Release / 同一对象存储，URL 永久可回溯。

## 3. 灰度与回滚

1. 灰度：先向内部通道（`transmutio-internal`）发布，观察 24h；再按 10% → 50% → 100% 放量
   （Tauri updater 本身无百分比能力，需要由清单服务按客户端 ID 返回不同版本实现）。
2. 回滚：**保留上一版与上上版安装包与清单**，回滚只需把 `latest.json` 指回旧版本，
   不删除 Release 附件；已升级客户端会收到「降级更新」提示，需用户确认（`allowDowngrades` 默认关闭）。
3. 紧急撤回：清单服务返回 4xx/空清单时，客户端静默跳过本次检查，不阻塞启动。

## 4. 失败与降级

- 下载后**必须**校验 minisign 签名与哈希；校验失败 → 拒绝安装并清空临时文件，保留当前版本。
- 不提供「签名校验失败仍安装」的开关；日志记录失败原因（不静默降级）。
- 网络失败 / 服务器 5xx：提示可重试，不改变本地版本。
- 更新过程崩溃：安装包采用原子替换（Windows NSIS 由安装器处理），失败后旧版仍可启动。

## 5. 启用前置条件（清单）

- [ ] 生成并托管 updater 密钥对（公钥入配置，私钥入 CI secret）
- [ ] `tauri.conf.json` / `tauri.gpl.conf.json` 增加 `plugins.updater`（endpoints + pubkey）
- [ ] 每个变体各自的 `latest.json` 发布流水线（含签名）
- [ ] Windows Authenticode / macOS 公证证书（可选，但与 updater 签名分开采购）
- [ ] 回滚演练：用旧清单回退一版并验证客户端行为
