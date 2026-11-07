# 许可策略与发行变体

本文件讨论 FFmpeg 及其编解码组件的许可与两个发行变体（flavor）的边界。

## 0. 两层许可，分开看（先看这里）

| 层 | 对象 | 许可 |
| --- | --- | --- |
| 应用代码 | 本仓库全部源码与随源码分发的资源（所有子目录，含 `src/`、`src-tauri/`、`scripts/`、`packages/`） | **MIT**，全文见根目录 `LICENSE` |
| 第三方二进制 | FFmpeg（**不随安装包分发**，由用户自行获取） | FFmpeg：LGPL 或 GPL（取决于用户选的构建与安装包变体）。PDF 功能已下线，不再调用 Poppler |

- 应用代码以 MIT 授权：可自由使用、修改、再分发，保留版权与许可声明即可。
- 「LGPL 版 / GPL 版安装包」指的是**所接受的 FFmpeg 构建**的许可路线，
  不是把应用代码分别按 LGPL / GPL 授权，也不存在「应用代码双许可」。
- 第三方许可全文保持原样，不做改写：`licenses/FFmpeg-LGPL.txt`、`licenses/FFmpeg-GPL.txt`
  （另含 LGPL-2.1 / GPL-2.0 全文，用于安装包合规物）。`licenses/Poppler.txt` 仅作历史声明保留：
  PDF 功能已从当前版本移除，不再调用 `pdftoppm` / `pdftotext`，也不再检测或下载 Poppler。


## 变体矩阵（LGPL 默认 / GPL 变体）

| 维度 | `lgpl`（默认） | `gpl` |
| --- | --- | --- |
| 产物名（productName） | `Transmutio` | `Transmutio GPL` |
| 安装包文件名 | `Transmutio_0.1.0_x64_*.msi` / `Transmutio_0.1.0_x64-setup.exe` | `Transmutio GPL_0.1.0_x64_*.msi` / `Transmutio GPL_0.1.0_x64-setup.exe` |
| 应用标识 identifier | `app.Transmutio.desktop` | `app.Transmutio.desktop.gpl` |
| 应用数据目录 | `%APPDATA%\app.Transmutio.desktop\` | `%APPDATA%\app.Transmutio.desktop.gpl\` |
| 更新通道 | `lgpl-stable` | `gpl-stable` |
| 编译期开关 | `allowGpl=false`、`requireGplBuild=false` | `allowGpl=true`、`requireGplBuild=true`（不可被设置项/环境变量翻转） |
| 接受的 FFmpeg 构建 | 仅 LGPL（拒绝 GPL；release 下无开发开关） | 仅 GPL（含 libx264/libx265；拒绝 LGPL） |
| nonfree | 拒绝 | 拒绝 |
| 默认下载源 | `https://ffmpeg.org/download.html`（选 LGPL 资产） | `https://www.gyan.dev/ffmpeg/builds/`（GPL full/release） |
| 可用软件编码器 | 取决于 LGPL 构建：`libopenh264` 若有则可用；无 libx264/x265 | `libx264`、`libx265`（GPL 构建） |
| 随包 FFmpeg 二进制 | 不随包分发 | 不随包分发 |
| 安装包内合规物 | `LGPL-2.1.txt`、`LGPL-3.0.txt`、`THIRD-PARTY-NOTICES-LGPL.txt`、`SOURCE-ACQUISITION-LGPL.txt` | `GPL-2.0.txt`、`GPL-3.0.txt`、`THIRD-PARTY-NOTICES-GPL.txt`、`SOURCE-ACQUISITION-GPL.txt` |
| 分发约束 | 可对外分发；不得混入 GPL/nonfree 组件 | 可对外分发；FFmpeg 由用户自行获取；若再分发 FFmpeg 二进制需自行履行 GPL 源码义务 |

两个变体共用同一份源码：唯一差异是 Cargo feature `gpl` + Tauri 配置 overlay
（`src-tauri/tauri.gpl.conf.json`）。

### 构建与验证入口

```bash
npm run build:lgpl     # 默认：无 feature，Transmutio
npm run build:gpl      # --features gpl + tauri.gpl.conf.json，Transmutio GPL
npm run tauri build    # 等价于 lgpl（默认），避免误发 GPL 包
npm run verify:flavor:lgpl
npm run verify:flavor:gpl
npm run check:bundle   # 解包检查两个安装包的许可证/notices/源码说明
```

构建脚本会把实际生效的 flavor 与编译期开关写入 `samples/build-<flavor>.json`，
并把安装包复制到 `dist-artifacts/<flavor>/`，避免两个变体互相覆盖。

### 合规物与「文案 = 实际行为」

- 两个安装包都携带对应的许可证全文、第三方声明、源码获取说明（见
  `src-tauri/resources/licenses/`，安装后位于 `licenses/`）。
- **两个变体都不随包分发 ffmpeg/ffprobe 二进制**，由用户在引导页自行下载、
  校验哈希并落位到各自的应用数据目录。因此：
  - 我们提供的是「获取方式 + 参考构建的源码版本/configure 参数/地址」，
  - **不提供** GPL §3 意义上的「源码书面要约」；
  - `npm run check:bundle` 会断言安装包里没有 ffmpeg 二进制，防止文案与行为不一致。
- 若未来改为随包分发 FFmpeg 二进制，必须同步补充源码提供方式（随附源码或有效要约，
  写清有效期与联系人），并更新上述两个文件与检查脚本。

## 默认路线：LGPL 安全包

- 默认安装包**不包含 FFmpeg 二进制**，由用户或构建方放入 LGPL 构建。
- 运行时能力表按 `license` 字段过滤：只有 `lgpl` 编码器默认可用；
  `gpl` / `nonfree` 即使编译进当前 FFmpeg，也会被标记为「许可策略已禁用」。
- 设置中的 `允许 GPL / nonfree 编码器` 默认关闭，仅供本地开发验证；
  开启后会把 GPL 编码器加入能力表，并在界面上显示高风险提示。
- 分发前执行 `npm run check:ffmpeg-license`，发现 `--enable-gpl`、`--enable-nonfree`、
  `libx264`、`libx265`、`libfdk_aac` 即失败。

### 默认包明确不引入

| 组件 | 许可 | 处理 |
| --- | --- | --- |
| libx264 | GPL | 不引入；非 GPL 包不可用 |
| libx265 | GPL | 不引入 |
| libfdk_aac | nonfree（且与 LGPL 不兼容） | 不引入 |
| libxvid / libxavs2 等 GPL 编码器 | GPL | 不引入 |
| libvpx / libaom / libwebp / libopenh264 | BSD 类（openh264 另有 Cisco 专利条款） | 允许，但当前构建未包含 openh264 |

### 默认允许（如果编译进 LGPL 构建）

- FFmpeg 内置 `aac`（LGPL）、`libmp3lame`（LAME LGPL，已标注 lgpl）、`libshine`（LGPL）
- 图片：`mjpeg`、`png`、`bmp`、`tiff`、`libwebp`
- 硬件编码：`h264_nvenc` / `hevc_nvenc` / `av1_nvenc`、`h264_amf` / `hevc_amf` /
  `av1_amf`、`h264_qsv` / `hevc_qsv`、`h264_vaapi` / `hevc_vaapi`、
  `h264_videotoolbox` / `hevc_videotoolbox`、`h264_d3d12va`
- 硬件解码：`-hwaccel cuda / d3d11va / dxva2 / qsv / vaapi / videotoolbox / d3d12va`
- 软件 H.264：`libopenh264`（如果构建包含）——**当前验证用的 gyan.dev 构建没有它**

## 许可相关待决策项

### 1. LGPL 包缺少软件 H.264 编码器（最重要）

现状：大多数 LGPL 构建不含 libx264，也不一定启用 libopenh264。结果是：

- 有可用硬编的机器：自动模式走硬编，正常使用。
- 没有硬编（或驱动失败）的机器：自动回退到软件时没有 H.264 编码器，
  任务会以 `encoder-missing` 失败，并提示改用其它编码器/安装组件。
- 本机验证用的 gyan.dev GPL 构建里有 libx264，但**不能分发**。

可选方案：

| 方案 | 做法 | 优点 | 代价 |
| --- | --- | --- | --- |
| A. OpenH264 | LGPL 构建加 `--enable-libopenh264` + Cisco 二进制 | 许可友好，有软件 H.264 兜底 | 需遵守 Cisco 专利条款；画质/码控弱于 x264；需按平台分发二进制 |
| B. 仅硬编 | 不提供软件 H.264，只依赖 NVENC/AMF/QSV/VAAPI/VT | 包最干净 | 老机器/驱动异常时不可用 |
| C. GPL 双包 | 另发 GPL 版（含 libx264/x265），默认包仍 LGPL | 兼容性最好 | 双构建、双安装包、双许可文本、GPL 源码义务 |

当前代码已按 A 的接口预留：`probe.rs` 会识别 `libopenh264`，`args.rs` 有对应码控分支，
只需换入包含 openh264 的 LGPL 构建即可启用。

### 2. HEVC 软件编码

libx265 是 GPL，LGPL 包没有软件 HEVC。当前策略：HEVC 只走
`hevc_nvenc / hevc_amf / hevc_qsv / hevc_vaapi / hevc_videotoolbox`，
能力表里不可用的会置灰并说明原因。若后续要软件 HEVC，只能走 GPL 双包方案。

### 3. 专利许可（与版权许可不同）

H.264 / HEVC / AAC 涉及专利池（MPEG LA / Access Advance 等）。FFmpeg 的 LGPL
只解决**版权**许可，不解决**专利**许可。分发带编码能力的二进制前需做专利评估；
硬件编码由驱动/操作系统实现，风险与责任划分需法务确认。这是待决策项，不是本工程能解决的。

### 4. LGPL 合规形式（sidecar 分发）

FFmpeg 作为独立可执行文件（sidecar）分发时，需满足 LGPL 的可替换/重链接要求：

- 随包提供 FFmpeg 的源码或获取方式（含修改说明与构建脚本）；
- 不得阻止用户替换 sidecar 二进制（本工程的解析顺序允许用户通过设置/env 指向自己的
  FFmpeg，满足这一点）；
- 在「关于 / 许可」中给出 FFmpeg 版本、构建配置与源码链接。

## GPL 双包：已落地的改动点（对照原计划）

原计划的 9 条已全部实现，落点如下：

1. **构建期开关** → `src-tauri/Cargo.toml` 的 `[features] gpl`；`src-tauri/build.rs`
   注入 `FF_FLAVOR`；`src-tauri/src/flavor.rs` 的 `Flavor` 集中所有策略。
2. **能力表** → `probe::detect` 接收由 `flavor::allow_gpl_effective` 计算的值；
   GPL 版放行 `gpl`+`lgpl`，nonfree 仍在两包都拒绝。
3. **安装包身份** → `src-tauri/tauri.gpl.conf.json`（productName / identifier /
   窗口标题 / 资源清单）。不同 identifier 天然隔离 app_data 目录与更新通道。
4. **构建脚本** → `scripts/build-flavor.mjs`（`npm run build:lgpl|build:gpl`），
   产物复制到 `dist-artifacts/<flavor>/`；`npm run tauri build` 仍是 LGPL。
5. **许可检查** → `scripts/check-ffmpeg-license.mjs`（LGPL 严格失败）+
   `scripts/check-bundle-compliance.mjs`（安装包解包断言）。
6. **界面与文档** → 关于面板/状态栏显示变体、构建 gpl/nonfree 标志与可用编码器；
   引导页按变体切换文案、默认下载源与许可提示。
7. **发布流水线** → 两套产物、两套 SHA256 记录（`samples/build-*.json`）；
   两个变体的产物互不覆盖由构建脚本复核。
8. **nonfree** → 任何变体都不接受（`Flavor::accepts_build` 首条规则）。
9. **首次启动引导** → `install_blocking` 接收 `Flavor`，LGPL 拒绝文案明确指向
   「请改用 GPL 版（Transmutio GPL）」；GPL 版只接受 GPL 构建。

## 首次启动引导的许可约束（当前实现）

- 不静默下载：应用只打开用户配置的下载页，下载动作由用户完成。
- 不代替用户接受许可：引导页只展示许可事实与校验结果，不提供「同意」勾选代替许可文本。
- nonfree 一律拒绝。
- LGPL 版：GPL 构建默认拒绝；debug 构建可用「仅本机开发」显式开关，release 构建无效。
- GPL 版：`allowGpl` 编译期固定为 true，只接受 GPL 构建；无开发开关。
- 落位前校验 SHA-256（可选，与发布页比对）、版本可读、构建配置；落位后复核哈希，
  不一致立即回滚。

## 待法务/产品确认

- 是否引入 OpenH264（方案 A），还是接受「无硬编即不可用」（方案 B）。
- GPL 双包已决定并落地（方案 C）；**源码获取说明与「不随包分发二进制」的措辞需法律确认**。
- H.264 / HEVC / AAC 专利许可与分发地区。
- 品牌素材授权（自绘图标可用于本项目，但品牌名需另评估）；应用代码许可已定为 MIT（见第 0 节）。
