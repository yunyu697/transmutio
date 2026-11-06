# Transmutio 更新通道方案

> 状态：**已实装（代码在 `src-tauri/src/updater.rs`，UI 在设置页「诊断与更新」）**。
> 密钥已生成：公钥在 `src-tauri/updater.pub`，私钥只在仓库外目录与本机 CI secret；
> 发布流水线会为每个安装包产出 `.sig`，安装前强制校验大小 + SHA-256 + 签名。
> 公钥缺失时仍按 fail-closed 拒绝安装，没有"跳过校验"开关。
> 交互以 Renomio 为参照模板，不另起一套。

## 1. 两类密钥，不要混为一谈

| 用途 | 格式 | 作用 | 是否本轮生成 |
| --- | --- | --- | --- |
| Tauri updater 签名密钥 | minisign（`.pub` / `.key`） | 给更新包签名，客户端校验后才安装 | **是**（公钥入库，私钥入 secret） |
| 代码签名证书 | Windows Authenticode（`.pfx`）/ macOS Developer ID | 消除 SmartScreen / Gatekeeper 警告，证明发布者身份 | **否** |

更新包签名**不等于**操作系统信任：没有 Authenticode 的包能通过 updater 校验，但仍会弹 SmartScreen。

密钥已生成（Tauri 兼容的 minisign 格式，私钥绝不入库）：

```bash
npx @tauri-apps/cli@2 signer generate -w <仓库外目录>/transmutio-updater.key
# 公钥写入 src-tauri/updater.pub（只提交公钥）
# 私钥与密码短语写入 CI secret：TAURI_SIGNING_PRIVATE_KEY / TAURI_SIGNING_PRIVATE_KEY_PASSWORD
```

`.gitignore` 已排除 `*.key` / `*.pem` / `*.sig` / `transmutio-updater.key*` / `password.txt` / `.tauri-signing/`。

## 2. 更新清单与来源

实装采用 **GitHub Releases API** 作为客户端清单（不依赖 `latest.json`）；发布流水线另外产出
`latest.json` / `latest-gpl.json` 供外部 Tauri updater 兼容使用：

- 官方源：`https://api.github.com/repos/yunyu697/transmutio/releases?per_page=30`
- 加速镜像：`前缀 + 官方 URL`（gh-proxy / ghfast / ghproxy.net / mirror.ghproxy）
- 回退顺序：**用户所选镜像 → GitHub 官方 → 其余镜像**
- 新版本判定：`tag_name` 去 `v` 后按语义化版本比较（支持 `alpha9 < alpha10`，正式版高于预发布）
- 变体隔离：LGPL 只匹配 `Transmutio_*`，GPL 只匹配 `Transmutio.GPL_*` / `Transmutio GPL_*`；
  平台匹配 Windows `_x64-setup.exe`、macOS `_<arch>.dmg`、Linux `.AppImage`（优先）/ `.deb`
- 包信息：`size` + `digest`（`sha256:`）来自 API；缺失时回退到同一 Release 的 `SHA256SUMS`
- 签名：Release 里存在 `<asset>.sig` 时使用；发布流水线为每个安装包强制生成 `.sig`
  （缺失即构建失败），`SHA256SUMS` 也包含 `.sig`

## 3. 交互口径（与设置页一致）

- 只放一个「检查更新」入口；未检查时只显示「检查更新」与「打开主页」。
- 检查中按钮显示「检查中…」且不可重复点击。
- **只有确实发现新版本**才出现「更新」「仅下载」，并显示版本号、包大小、来源；已是最新显示
  「当前已是最新版本」。
- 「更新」= 下载 → 校验 → 启动安装 → 应用退出 → 安装完成自动重启。
- 「仅下载」= 落默认下载目录（`下载目录/资产名`），完成后按钮变「安装」；点「安装」再做
  大小 + SHA-256 + 签名二次校验后安装。
- 已下载且校验通过的包**持久化在** `<appData>/updates/pending.json`：即使随后检查显示已是最新，
  「安装」仍保留。
- 每个来源的失败原因（HTTP 状态、限流、超时、哈希/签名失败）汇总展示，失败自动换源；
  **签名/哈希失败也换源，绝不降级为不校验**。

## 4. 下载与清理

- 下载写 `<下载目录>/.transmutio-update-<资产名>.part`，支持 `Range` 断点续传与每源 2 次重试。
- 任一来源失败或换源：立即删除临时文件与其 `.sig`，不留下半成品。
- 启动时清理所有 `.part` 残留；`pending.json` 指向的包若不存在或版本已不高于当前版本，
  连同包一起清掉。
- 落盘前校验：字节数 + SHA-256（+ 配置公钥时的 minisign 签名）；不通过即拒绝落盘。
- 安装前二次校验：再次核对大小 + SHA-256 + 签名。

## 5. 平台能力边界（如实标注）

| 平台 / 形态 | 能力 | 说明 |
| --- | --- | --- |
| Windows（NSIS `-setup.exe`） | **检查 + 安装** | 启动安装器 `/S`，应用退出；安装器完成后重启 |
| macOS（`.dmg`） | **检查 + 下载** | 打开 DMG 让用户拖入「应用程序」，应用不会自行替换 |
| Linux AppImage | **检查 + 安装** | 替换当前 AppImage 后重启 |
| Linux deb / 发行包 | **检查 + 下载** | 安装由包管理器完成（需要 root），应用不自行替换 |

## 6. 失败与降级

- 校验失败 → 拒绝安装并清空临时文件，保留当前版本；没有「跳过校验」开关。
- 网络失败 / 服务器 5xx / GitHub API 限流（403/429）：自动换源，提示可重试，不改本地版本。
- 更新过程崩溃：Windows 由 NSIS 安装器处理；AppImage 先写 `.new` 再原子改名。
- 回滚：保留上一版与上上版安装包；客户端默认不接受降级（版本比较只升不降）。

## 7. 启用前置条件（清单）

- [x] 生成并托管 updater 密钥对（公钥写入 `src-tauri/updater.pub`，私钥入 CI secret）
- [x] 更新器代码 + 设置页交互 + 多源回退 + 断点续传 + 启动清理
- [x] Windows / macOS / Linux 能力边界在 UI 与文档中标注
- [x] 发布流水线配置签名 secret，Release 资产出现 `.sig` 与 `latest.json` / `latest-gpl.json`
- [ ] Windows Authenticode / macOS 公证证书（与 updater 签名分开采购）
- [ ] 回滚演练：用旧版本包验证客户端行为
