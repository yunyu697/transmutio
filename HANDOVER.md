# Transmutio 开发交接文档

> 生成于 2026-11-13，由 AI 开发过程的完整工作记忆汇总而成。
> 配套材料：`transmutio-handover-memory.zip`（Trae 会话记忆原文件，含逐日任务记录与硬约束来源，不入库）。

## 1. 项目概览

- **产品**：Transmutio —— 跨平台多媒体转换桌面应用（视频/音频/图片批量转换，硬件编码优先，失败自动回退）
- **仓库**：`github.com/yunyu697/transmutio`，本地目录名 `formatforge/`（历史遗留名，内部仍有 `transmutio:*` 存储键约定）
- **技术栈**：Tauri 2.12.5 + React 19 + TypeScript + Vite；Rust 后端在 `src-tauri/`
- **双变体**：LGPL 版（`tauri.conf.json`，产品名 Transmutio）与 GPL 版（`tauri.gpl.conf.json`，产品名 Transmutio GPL，允许 libx264/libx265）。FFmpeg 不随包分发，用户自行获取
- **前端产物**：Vite 构建到 `dist/`，Tauri 打包三平台（MSI/NSIS/dmg/AppImage/deb）

## 2. 当前状态（截至 2026-11-13）

- 最新版本 **v1.3.2** 已发布（Release published，23 个资产），main HEAD = `cb86263 chore(release): 1.3.2`，工作区干净
- 无遗留待办。下一版发版时在 release notes 里带一句「侧栏折叠按钮移位与对齐（已在 1.3.2 落地）」即可
- 1.0.0 曾长期暂停发布（走 alpha 序号），最终已正式发出；**1.0.0 暂停期确立的规矩：完成修复后先回报核对，不自行发布**

### 版本历史速览

| 版本 | 内容要点 |
| --- | --- |
| 0.1.x → 1.0.0-alpha* | 安装后自动重启（NSIS `/S /R`）、删「默认」码控选项、删 formatforge 旧数据迁移 |
| 1.0.0 | 六项假功能修复（drawtext 水印等）、音频合并/截取、FEATURE-GAP.md 对账 |
| 1.1.0 / 1.1.1 | PDF→TXT（后又在 1.2.x 彻底移除）、README 全面对账、引导页 GPL 提示 |
| 1.2.1 / 1.2.2 | 首帧闪烁修复（bootDone 门闩 + 内联底色）、过渡动画 + 减少动态效果适配、间距标尺、v4 设计稿（删顶栏/PDF 全移除/预设系统） |
| 1.3.0 | 方案 A 侧栏重组、真机验证脚本 `verify:real` 入库、首帧跳色根因修复 |
| 1.3.1 | crystal-indigo 新图标全套（ICO/ICNS/PNG/安装器）、BrandMark 双 SVG 跟明暗 |
| 1.3.2 | 侧栏折叠按钮移到左下角靠右并与底部图标行对齐 |

## 3. 架构与关键设计决策

- **主题正交**：`data-theme`（light/dark）× `data-palette`（graphite/ocean/amber/forest）两个独立轴。品牌相关资源**只跟明暗、绝不跟配色**——不能做 8 套图标
- **首帧**：`index.html` 内联脚本只写 `data-theme`/`data-palette` 和底色，主界面用 `bootDone` 门闩控制显隐。根因教训：引导页异步切换会闪一帧；WebView2 初始空画布还有 1 帧残留闪烁（已接受，未用延时窗口掩盖）
- **BrandMark**（`src/components/BrandMark.tsx`）：浅色/深色两版 SVG 常驻 DOM，CSS 按 `[data-theme]` 切 display。1.3.1 起取代旧的「CSS 取景裁切 Tauri 图标」方案
- **侧栏**：展开 236px / 折叠 64px，同一个 nav 节点宽度过渡。折叠按钮是**同一个 `.collapse` 节点**，绝对定位 right/bottom 各 10px，两态只换箭头方向与 `aria-label`，禁止退回两个按钮。折叠态图标列底部让位 42px（10 底距 + 32 按钮高）
- **间距标尺**：`--sp-1`(4) ~ `--sp-5`(20)，新增样式先对照档位；动效时长集中在 `--motion-*`，必须尊重系统「减少动态效果」
- **预设**：按方法粒度存 `transmutio:presets:v1`，不含绝对路径与凭据，启动载入上次使用的
- **旧数据**：禁止任何 formatforge 迁移逻辑。只读写 `transmutio:*`，`formatforge:*` 残留键不读取、不迁移、不删除，代码注释明确禁止重新添加
- **PDF**：已彻底移除，不得残留控件/文案/断言/代码引用。`tools/poppler-*` 目录保留不自动删，`licenses/Poppler.txt` 标注「历史声明」

## 4. 硬约束（必须遵守）

- 界面文案必须与实际行为完全一致，禁止「点了没反应」的空壳选项——要么彻底实现，要么移除
- 禁止引入非必要新依赖、修改无关模块
- 发布补丁版本必须完整执行指定范围，不得自行裁剪
- 视频码控/音频码率**不得出现「默认」选项**；历史 `mode=default` 读回映射到最接近的合法选项
- CRF 档位固定五档（近乎无损/高质量/标准/省空间/极小体积），具体数值只在高级选项展示
- 分辨率、帧率、图片质量滑杆、最长边像素值保持现状，不加档位名
- 修复界面问题先定位真实根因并给依据，禁止延时显示窗口等掩盖手段
- 真机验证脚本不进 CI（需要真实窗口和本机 LGPL 构建）
- LGPL 发布版隐藏「允许 GPL」开关，仅 debug 构建保留
- 界面语言 en-US 选项隐藏并标注「暂未提供」（未接 i18n）
- 已发布版本除非用户明确要求，不删 Release/tag、不回退版本号（孤儿版本教训）

## 5. 发布流程（照此执行，不要即兴）

1. `npm pkg set version=X.Y.Z` → `npm run version:sync`（同步 package.json/lock、两个 tauri*.conf.json、Cargo.toml/lock）→ `npm run version:check`
2. 写 `release-notes/vX.Y.Z.md`（历史都在 `release-notes/`，照风格写）
3. 本地构建 `npm run build:lgpl`，跑全量 `verify:real`
4. 提交 `chore(release): X.Y.Z` → 打 tag `vX.Y.Z` → push main + tag
5. GitHub Actions `release.yml` 三平台构建（guard → build → release），自动建 Release + 传 23 个资产（LGPL/GPL × MSI/NSIS/dmg/AppImage/deb + `.sig` + `latest*.json` + `SHA256SUMS`）
6. 发布后核对：资产数量 23、关键资产 SHA-256 与 `SHA256SUMS` 逐条比对、静默安装读回 `--version`
7. 等 CI 不要用 `gh run watch --exit-status`（成功也返回 1），用 `gh run list --json status,conclusion` 轮询

## 6. 验证体系

| 命令 | 内容 | 环境 |
| --- | --- | --- |
| `npm run verify:real` | 真实 WebView2 全量 28 项（首帧浅/深采样 + 功能断言 + 折叠按钮两态） | 真机，CDP 端口 9455 |
| `npm run verify:ui` | 无头 UI 39 项 | CI 可跑 |
| `node scripts/panel-persist-check.mjs` | 面板持久化 35 项 | 无头 |
| `npm run verify:theme` / `verify:formats` | 主题 72 项 / 格式 30 项 | 无头 |
| `npm run test:rust` | Rust 164 项 | 本地/CI |
| `scripts/startup-frame-check.ps1` | PrintWindow 逐帧首帧采样 | 真机 |

`verify:real` 注意：需要本机 LGPL ffmpeg（环境变量 `VERIFY_FFMPEG` 或 `samples/_dl` 下自动查找）；启动前必须 `taskkill /IM Transmutio.exe /F` 清场——**单实例机制会让第二个进程转发退出、CDP 端口不开**。

## 7. 图标与品牌资源

- 源图：`src/assets/brand-light.svg` / `brand-dark.svg`（512 viewBox），小尺寸光学版 `brand-light-small.svg`（16/24/32：去投影、加白描边分离三卡、波形加粗）
- 生成：`npm run icons`（`scripts/make-icons.mjs`）→ Chromium 渲染 PNG → 手写 ICO（16~256 七档）/ ICNS → 调 `@tauri-apps/cli icon` 铺全平台 → 覆盖桌面图标；预览输出 `samples/icon-preview/`
- NSIS 安装器图标必须配置 `installerIcon: "icons/icon.ico"`（两个 conf 都已配；漏配会显示默认 NSIS 图标）
- 16px 单独看过：三色块可辨，字形退化为白点——已接受

## 8. 踩坑清单（都付过学费）

- NSIS 静默安装必须 `/S /R`，只给 `/S` 不自动重启
- 0.1.0 不支持 `--version`，直接调会弹 GUI；**版本探测前先判断支持性**
- GUI 子系统 exe 的 stdout 在 PowerShell 里捕不到，用 `node spawnSync` 捕获
- PowerShell 不支持 heredoc，多行 commit 用多个 `-m`
- `spawnSync("npx.cmd", ...)` 在 Node 下 EINVAL，改 `spawnSync(process.execPath, [tauri.js路径, ...])`
- 命名模板验证 FAIL 的常见根因是本地残留文件，先清理再测
- legacy-guard 运行时探针在部分环境会假 FAIL，属已知现象

## 9. 需要线下移交的资产（不在 git 里）

1. **GitHub 仓库权限**（yunyu697/transmutio）与 CI secrets（签名密钥等）
2. **Tauri updater 签名私钥**（`.sig` 文件就是它签的——拿到仓库却签不出新 `.sig` 就发不了版）
3. 本机 LGPL ffmpeg 构建（`verify:real` 与引导页注入依赖它）
4. Trae 会话记忆包：`transmutio-handover-memory.zip`（见配套文件，含 2026-11-06 ~ 11-13 逐日任务记录）

## 10. 仓库内已有文档索引

- `README.md` —— 已与实际能力全面对账（1.1.0 起保持同步更新）
- `FEATURE-GAP.md` —— 对标格式工厂的功能对账（A41/B6/C2/D9/E15/N16）
- `release-notes/v*` —— 全部历史版本的发布说明
- `.github/workflows/release.yml` / `ci.yml` —— 发布与校验流水线
