# Transmutio（MVP）

跨平台桌面转码工具（Windows / macOS / Linux），Tauri 2 + React 19 + TypeScript，
FFmpeg 以外部子进程调用（不内嵌、不写入系统 PATH）。本仓库是独立工程，不与其他项目共享代码目录。

## 结论（先看这里）

| 项目 | 状态 |
| --- | --- |
| 可构建运行的应用工程 | ✅ Windows debug 构建通过，窗口可启动（构建日志 `build-debug.log`） |
| 视频转码（6 容器） | ✅ 已跑通：MP4 / MKV / WebM / MOV / TS / AVI；1080p→720p、4K→1080p、120s 长时长、硬解 d3d11va + `h264_amf` 硬编、硬编失败自动回退软编 |
| 音频转码（6 格式） | ✅ 已跑通：MP3 / AAC(m4a) / FLAC / WAV / OGG(Vorbis) / Opus；含从视频提取音轨；无损格式自动跳过码率参数 |
| 图片批量转换 / 缩放 | ✅ 已跑通：PNG→JPG、WebP、4K→最长边 1280（不拉伸） |
| 近期补齐的能力 | ✅ 音频合并 / 截取（`1.0.0`）；视频任意矩形裁剪（预览帧 + 框选）、设备预设（4 种）、音频混音（`1.1.0`）；均有 `verify:core` 真实链路实测（见下「`1.1.0` 实测摘要」与「`1.1.1` 实测摘要」） |
| 任务中心 | ✅ 4 槽并发、进度、取消杀子进程、失败重试、打开输出目录；`verify:core` 40/40 真实链路实测 |
| 用户文件安全 | ✅ 不覆盖（硬链接原子发布）、临时文件、取消清理；删除源文件未实现（默认关闭） |
| 首次启动引导 | ✅ 缺失或 GPL/nonfree 时展示；可选「按内置清单下载」（官方源 + 镜像回退 + 内置 SHA-256 校验 + 断点续传），也可手动选文件；UI 层 headless 39/39 + 真实 LGPL 落位 12/12（见文档 §4.2、§4.22） |
| 默认许可路线 | ✅ 默认走 LGPL 清单，不下载 GPL 构建；LGPL 发布版编译期强制拒绝 GPL 构建（仅 debug 构建可用设置页开关临时放行），GPL 变体则只接受 GPL 构建 |
| macOS / Apple Silicon | ⚠️ **CI 安装包构建通过，运行时待真机验证**（本机无 Apple 设备） |
| Linux | ⚠️ **CI 安装包构建通过，运行时待真机验证**（未在 Linux 上运行） |

MVP 完成度与逐项证据见 [MVP-完成度与验证.md](MVP-完成度与验证.md)；
许可待决策项见 [LICENSING.md](LICENSING.md)；
GPU 能力矩阵与实测/文档/待验证标注见 [GPU-能力矩阵.md](GPU-能力矩阵.md)。

## 功能范围（80/20）

已实现（入口可用，真实转换）：

- 视频：视频转码（容器 MP4 / MKV / WebM / MOV / TS / AVI，编码器按容器联动，H.264 / HEVC / AV1 / VP9 / VP8 / MPEG-4，分辨率、帧率、码控、音频、硬解、faststart，含 iPhone/iPad / Android / PSP / BlackBerry 设备预设模板）、压缩、旋转 / 比例裁剪 / 任意矩形裁剪（预览帧 + 框选）、多文件合并、音视频混流、转 GIF、变速（0.25–4.0，保持音高）、水印（图片 / 文字）、字幕（内嵌 / 烧录）、相对音量增益（−20…+20 dB，可静音）、按时间点分割（流复制 / 重编码）
- 图片：批量转换（JPG / PNG / WebP / BMP / TIFF）、批量缩放、批量裁剪（比例 / 自定义矩形）、网格拼图、加水印、目标大小 / 固定质量压缩、多图转 GIF（palettegen + paletteuse）、EXIF 元数据写入（描述 / 作者 / 版权 / 软件 / 拍摄时间 / 厂商 / 型号 / 镜头 / 方向，只写副本，写完回读校验）、转 AVIF（质量 / 速度可调，纯 Rust 编码）
- 序列：图片序列 → 视频（fps / 尺寸统一 / 编码器，起始编号按文件名自动推导）、视频 → 图片序列
- 音频：转码（MP3 / AAC→m4a / FLAC / WAV / OGG-Vorbis / Opus）、合并（多文件按顺序拼接，自动统一采样率/声道）、截取（按时间区间，支持批量同区间）、混音（多轨等权叠加，时长取最长一路，可选响度归一化）、音量标准化（EBU R128）、相对音量增益
- 配置：面板参数 + 输出策略的预设导入 / 导出（版本与字段严格校验，不含路径与凭据；导入会同步「批次完成后」动作），以及按方法记忆的命名预设（保存 / 重命名 / 删除，启动自动载入上次使用的预设）
- 界面：左侧栏分组导航（可折叠并记住折叠状态，折叠态为 64px 图标栏）、最近使用 4 条、分区速览卡（常用方法 / 常用格式 / 适用场景 / 上次参数摘要 / 继续入口）
- 任务：4 槽并发、暂停 / 继续、命名模板、冲突策略、自动重试（仅 hw-unavailable / 临时 IO）、批次完成后动作（不动作 / 打开输出目录 / 关机；关机需环境变量 `FORMATFORGE_ALLOW_SHUTDOWN=1` 才真正下发）
- 更新：检查 / 下载 / 校验（GitHub 官方 + 加速镜像按序回退、断点续传、启动清理）；
  Windows 为「检查 + 安装」，macOS 与 Linux deb 如实标为「检查 + 下载」

**PDF 功能已整体下线**：原有 PDF 合并 / 拆分、PDF 转图片、PDF 转文本四项已从界面、命令、依赖
（`pdf.rs` / `lopdf`）与下载清单中全部移除，不再检测或调用 `pdftoppm` / `pdftotext`，
也不再解析 `PDFTOPPM_PATH` / `PDFTOTEXT_PATH` / `TRANSMUTIO_PDF_TOOLS`。
此前自动落位的 `tools/poppler-*` 目录不会被自动删除，可自行清理；`licenses/Poppler.txt`
仅作历史声明保留。

**不再保留其它占位入口**：文档转 PDF / Office 互转、光驱 / DVD / ISO、工具集
（改名、校验、信息、截图等）已随导航分区一起移除，界面上没有指向未实现能力的控件。

`1.0.0` 自查发现的六项「看起来能做、实际没做」问题已全部处理，README 描述以处理后的行为为准：
文字水印改为真实 `drawtext`（`textfile=` 传文本，支持中文字体）；序列「起始编号」不再可手填、
按文件名自动推导 `-start_number`；界面语言下拉已移除（仅简体中文）；预设导入会真正应用并同步
「批次完成后」动作；命名模板校验与 LGPL 合规证据两项为测试/发布闸门修复，已回归通过。

### 格式清单与探测的关系

格式清单是**固定的完整矩阵**（视频 6 容器 × 兼容编码器、音频 6 格式），不随探测结果增减：

- 单一事实来源：后端 `src-tauri/src/containers.rs`，前端 `src/lib/formats.ts`（两者同构，由 `npm run verify:formats` 断言）；
- 探测（`ffmpeg -encoders` + 试编）只决定「哪些选项可用」：缺编码器的容器 / 格式**灰显并给出原因**，不会从列表消失；
- 不兼容组合（如 MP4 + VP9、WebM + AAC）在界面上不出现在可选项里，后端入队前再次拒绝；
- 探测未完成时显示「正在探测本机编码器…清单已完整渲染」；无 ffmpeg 或无可用编码器时显示
  「未检测到 ffmpeg 或编码器不可用」，**不会静默退化为只剩 mp4/mp3**。

### 本地数据与升级

- 设置与面板参数只使用 `transmutio:*` 键。旧版 `formatforge:*` 残留键**不读取、不迁移、也不删除**。
- `1.1.0` 起首次启动会**一次性**删除两个旧版遗留键：`transmutio:storage-migration:v1` 与
  `transmutio:migration-notice-dismissed`。是否已执行由独立标记位 `transmutio:cleanup:v2` 控制，
  只跑一次；删除失败静默忽略、不影响启动，也不触碰 `transmutio:settings`、面板参数或其它任何键
  （实测：注入两键 → 启动 → 两键消失、其它键原样保留；二次启动无新写入）。

## 下载与许可

许可分两层，**互不替代**，请分开看：

| 层 | 对象 | 许可 |
| --- | --- | --- |
| ① 应用代码 | 本仓库全部源码（含 `src/`、`src-tauri/`、`scripts/`、`packages/` 等所有子目录） | **MIT**，全文见根目录 [LICENSE](LICENSE)，覆盖整个仓库 |
| ② 第三方二进制 | FFmpeg（安装包**不包含**，用户自行获取） | LGPL 或 GPL，取决于构建与安装包变体，见下文与 [LICENSING.md](LICENSING.md) |

- **应用代码 = MIT**：任何人可自由使用、修改、再分发本仓库代码，只需保留版权与许可声明。
- **FFmpeg = 各自的 LGPL / GPL 路线**：FFmpeg 不属于本仓库代码，也不随安装包分发。
  LGPL 变体只接受 LGPL 构建的 FFmpeg；GPL 变体只接受 GPL 构建的 FFmpeg（可用 libx264 / libx265）。
  PDF 功能已下线，不再需要、也不再调用 Poppler；`licenses/Poppler.txt` 仅作历史声明保留。
  第三方许可证全文保持原样，位于 `licenses/`。

- 发布页：<https://github.com/yunyu697/transmutio/releases>（当前正式版 `1.1.1`，2026-11-07 发布；
  `1.1.0` 为上一正式版，`1.0.0` 为更早正式版，`0.1.0` 为历史版本）
- **安装包不包含 FFmpeg 二进制**。仓库与 Release 附件都不分发这个二进制：
  FFmpeg 由用户自行准备并在首次启动时经引导页校验（版本 / 构建配置 / SHA-256）后落位到
  应用数据目录。因此本项目不承担 FFmpeg 二进制再分发的源码提供义务；若你要再分发它，
  请自行履行对应许可证。更新包的 `.sig` 为 minisign 签名，与应用安装包的 Authenticode 签名
  不是一回事（见下文「未做」清单）。
- 两个安装包的区别与所选许可路线：

  | 包 | 接受的 FFmpeg 构建 | 含义 |
  | --- | --- | --- |
  | `Transmutio_<版本>_x64_*`（LGPL 变体，默认） | 仅 LGPL 构建（拒绝 `--enable-gpl` / `--enable-nonfree`） | 选择此包 = 只使用 LGPL 许可的 FFmpeg 与编码器（如 libopenh264 / libvpx / SVT-AV1 / aac / libmp3lame） |
  | `Transmutio GPL_<版本>_x64_*`（GPL 变体） | 仅 GPL 构建（含 libx264 / libx265） | 选择此包 = 接受 GPL 许可的 FFmpeg 构建，可用 x264 / x265 |

  两包共用同一份应用源码（同一 MIT 许可），差异仅为编译期 `gpl` feature + Tauri overlay
  （独立应用标识与数据目录）。上表的「LGPL / GPL」只描述**所接受的 FFmpeg 构建**，
  不是应用代码的双许可。
- 许可证全文：`licenses/FFmpeg-LGPL.txt`（LGPL-2.1）、`licenses/FFmpeg-GPL.txt`（GPL-2.0）；
  `licenses/Poppler.txt`（GPL-2.0+）仅作历史声明保留，当前版本不再调用。安装包内另附对应变体的
  许可证、第三方声明与源码获取说明（`src-tauri/resources/licenses/`）。
- 源码获取方式（本项目**未修改** FFmpeg 源码）：
  - FFmpeg：<https://ffmpeg.org/download.html>；源码 <https://git.ffmpeg.org/ffmpeg.git> 或
    <https://github.com/FFmpeg/FFmpeg>。内置下载清单钉死的 LGPL 构建：
    `N-128433-gdb8537d8cc`（BtbN/FFmpeg-Builds，清单内置官方 SHA-256），
    configure 参数全文见 `src-tauri/resources/licenses/SOURCE-ACQUISITION-LGPL.txt`；
    GPL 参考构建（gyan.dev）见 `SOURCE-ACQUISITION-GPL.txt`。
- **安装包未做 Authenticode / Apple 公证**（更新包另有 minisign 签名并已实测校验），
  Windows SmartScreen 可能提示未知发布者。

## 环境要求（本机实测版本）

- Node.js ≥ 20（实测 v26.13.0），npm ≥ 10（实测 11.20.0）
- Rust stable（实测 rustc/cargo 1.99.1）+ 平台工具链
  - Windows：Visual Studio Build Tools（C++/link.exe）+ WebView2 Runtime（实测 156.0.4314.58）
  - macOS：Xcode Command Line Tools
  - Linux：`webkit2gtk-4.1`、`libappindicator3`、`librsvg2`、`patchelf` 等 Tauri 依赖
- FFmpeg：**仓库默认不包含任何二进制**（许可与体积考虑），需自行准备 LGPL 构建

## FFmpeg 准备（必须）

默认包不放 ffmpeg。首次启动若找不到 FFmpeg、或找到的是 GPL/nonfree 构建，应用会进入
**首次启动引导页**，提供两条路径（都需你**手动点击**，不会后台静默下载）：

### A. 让应用按内置清单下载（可选）

- 清单（`src-tauri/resources/tools-manifest.json`）随二进制编译进应用，里面是**官方发布的
  版本号、文件名、字节数与 SHA-256**；镜像只替换传输通道。
- 来源顺序：官方 GitHub Release → 多个第三方加速镜像；失败会自动按顺序回退，界面实时显示
  当前来源、第几次尝试与已下载字节。
- **任何来源的文件落盘前都必须匹配内置 SHA-256**：镜像给的文件一律不采信；不匹配会删除
  临时文件并换下一个来源，全部失败则给出逐项可读原因（断网 / 超时 / HTTP 状态 / 哈希不符）。
- 许可变体严格对应：LGPL 包只下载 LGPL 构建，GPL 包只下载 GPL 构建；落位后再读一次
  `ffmpeg --version` 与构建配置，不匹配则回滚。
- 支持断点续传（`Range`，服务器不支持时自动从头）与每来源最多 3 次重试；下载前先检查
  磁盘剩余空间，不足直接拒绝并给出数字。
- 落位到 `<应用数据目录>/tools/ffmpeg-<版本>/`，路径写入设置页的 ffmpeg 路径，随后重新探测能力表。

当前内置清单的可自动下载包：**Windows x64** 的 FFmpeg LGPL / GPL（BtbN 固定构建）。
**macOS / Linux 没有可固定 SHA-256 的官方发布包**，因此不提供自动
下载，引导页会给出可读原因与手动/包管理器方案（不会伪造一个跑不通的下载项）。

### B. 手动选择本地文件

1. 引导页给出可配置的 LGPL 下载源（默认 FFmpeg 官方下载页，链接用系统浏览器打开）
2. 选择已下载的 `ffmpeg` 可执行文件，应用读取版本、构建配置、编码器列表与 SHA-256
3. 可选填入发布页提供的期望哈希做比对；通过后复制到 `<应用数据目录>/tools/ffmpeg-<版本>/`
4. 落位后复核哈希并持久化路径，随后重新探测能力表

明确不做：不静默下载可执行文件、不代替用户接受许可、不接受 nonfree 组件；
GPL 构建不被默认包接受：LGPL 发布版在编译期强制拒绝，设置页只显示「发布版固定拒绝」说明，
不提供可切换的开关；只有 debug 构建才有「允许 GPL」开关（临时放行）。GPL 变体则只接受 GPL 构建。引导页在无 FFmpeg 时也能正常展示，
任何转换入口都不会伪造成功。

也可以手动放置（任选其一）：

1. 设置页填写 ffmpeg 可执行文件路径（会同时查找同目录的 ffprobe）
2. 环境变量 `FFMPEG_PATH` / `FFPROBE_PATH`
3. 放到应用目录 / 资源目录（推荐随包分发）

| 平台 | 放置路径（源码树内，构建时复制到资源目录） |
| --- | --- |
| Windows | `src-tauri/resources/ffmpeg/ffmpeg.exe`、`ffprobe.exe` |
| macOS | `src-tauri/resources/ffmpeg/ffmpeg`（**arm64 原生**）、`ffprobe` |
| Linux | `src-tauri/resources/ffmpeg/ffmpeg`、`ffprobe` |

解析顺序：设置中的路径 → `FFMPEG_PATH` → 应用目录（含 macOS `Contents/Resources`）→
资源目录（含 `resources/ffmpeg`）→ 系统 `PATH`。也兼容 Tauri sidecar 命名
`ffmpeg-<target-triple>[.exe]` 与 `binaries/` 目录。

**许可要求**：分发版必须使用 LGPL 构建（`--disable-gpl --disable-nonfree`，不含
libx264 / libx265 / fdk-aac）。本机开发用的 gyan.dev full 构建是 GPL，仅用于验证，
不得打包。分发前执行：

```powershell
npm run check:ffmpeg-license
```

## PDF 功能（已下线）

PDF 合并 / 拆分、PDF 转图片、PDF 转文本已从当前版本删除：

- 前端：PDF 方法卡、PDF 面板、设置页「PDF 工具目录」、`pdfToolsStatus` 调用一并移除；
  分区导航不再有「文档」入口。
- 后端：`pdf.rs` / `pdf_tools.rs`、4 条 PDF 命令、`lopdf` 依赖、`pdftoppm` / `pdftotext` 解析
  （`PDFTOPPM_PATH` / `PDFTOTEXT_PATH` / `TRANSMUTIO_PDF_TOOLS`）全部删除。
- 引导页与 `tools-manifest.json` 不再提供 Poppler 下载项。
- 旧版自动落位的 `tools/poppler-*` 目录**不会被自动删除**，可自行清理；
  `licenses/Poppler.txt` 保留并标注「历史声明，当前版本不再调用」。

如果你仍在用旧版本做过 PDF 任务，其输出文件不受影响；升级后界面不会再出现相关入口。

## 选哪个变体：LGPL 还是 GPL

应用有两个发行变体，**同一份源码、编译期注入**（Cargo feature `gpl` + Tauri 配置 overlay）：

| | LGPL 版（默认） | GPL 版 |
| --- | --- | --- |
| 产物 | `Transmutio` | `Transmutio GPL` |
| 许可 | 仅接受 LGPL 构建；无 libx264/libx265 | 只接受 GPL 构建；可用 libx264/libx265 |
| 软件 H.264 | 取决于构建（有 openh264 才有兜底） | 有（libx264） |
| 软件 HEVC | 无 | 有（libx265） |
| 下载源默认 | ffmpeg.org（选 LGPL 资产） | gyan.dev（GPL full/release） |
| 数据目录/通道 | `app.transmutio.desktop` / `transmutio-lgpl-stable` | `app.transmutio.desktop.gpl` / `transmutio-gpl-stable` |

怎么选：

- **普通用户 / 企业默认选 LGPL 版**：许可风险最低，不需要处理 GPL 源码义务；
  有 NVENC/AMF/QSV/VideoToolbox 硬编的机器体验一致，老机器无硬编时可能缺少软件 H.264。
- **需要软件 H.264/HEVC 兜底、愿意承担 GPL 义务的用户选 GPL 版**：含 x264/x265，
  兼容性最好。
- **企业再分发**：优先 LGPL 版，并完成专利与法务评估；若分发 GPL 版，
  FFmpeg 仍由用户自行获取（本应用不随包分发二进制），但你自己若再分发 FFmpeg
  二进制，需自行履行 GPL 源码提供义务。

升级不能混装：两个变体使用不同 `identifier` 与数据目录，安装另一个变体会共存而不是升级；
切换变体前请先导出/清理设置。`npm run tauri build` 始终构建 LGPL 版，避免误发 GPL 包。

## 从 FormatForge（旧版）升级

**旧版不会原地升级。必须先卸载旧版，再安装 Transmutio。**

- 旧版 `FormatForge` 与新版 `Transmutio` 的 `identifier`、产品名、安装目录、卸载注册表项全部不同，
  Windows 视为两个产品；允许并存但不支持并存。安装器会检测旧版（显示名 + 旧 MSI UpgradeCode），
  命中时提示先卸载并中止安装，不会写入半套文件。
- 正确顺序：卸载旧版 → 安装 Transmutio。新版**不读取、不迁移旧版设置与数据**，
  首次启动即为默认设置；旧版数据目录保留在磁盘上，可自行确认后手动删除。

完整的变体矩阵与合规物说明见 [LICENSING.md](LICENSING.md)。

## 未签名构建的安装与首次运行

当前所有安装包**都没有代码签名**（无 Authenticode、无 Apple Developer ID、无公证）。系统会因此
给出警告，这是**真实存在、尚未解决**的限制，下面的步骤只是让系统放行，不等于问题已修复。

| 平台 | 现象 | 放行方式（自担风险） |
| --- | --- | --- |
| Windows | SmartScreen「Windows 已保护你的电脑 / 未知发布者」 | 「更多信息」→「仍要运行」；MSI 可先在文件属性里勾选「解除锁定」 |
| macOS | Gatekeeper「无法打开，因为 Apple 无法验证」「已损坏，无法打开」 | 在 Finder 里**右键 → 打开 → 再点打开**；或 系统设置 → 隐私与安全性 → 「仍要打开」；必要时 `xattr -dr com.apple.quarantine /Applications/Transmutio.app` |
| Linux（deb） | `apt install ./Transmutio_*.deb` 提示包未签名 | 用 `sudo dpkg -i Transmutio_*.deb` 安装；缺少运行时库时按提示补 `libwebkit2gtk-4.1-0`、`libgtk-3-0` |
| Linux（AppImage） | 双击无反应 / 提示 FUSE | `chmod +x Transmutio_*.AppImage` 后运行；无 FUSE 时用 `--appimage-extract-and-run` |

macOS 上没有「设置一次就永久不警告」的官方做法；每次更新安装包后都可能重新触发。如果
安装后应用能启动但**所有转换都失败**，先确认 FFmpeg 是否可用（见下文引导页），再检查
是否被 Gatekeeper 拦住了 sidecar 二进制（签名链路不完整时的典型表现）。

## 开发与构建

按项目约定，**构建与验证一律后台执行并写日志**（见 [CONTRIBUTING.md](CONTRIBUTING.md)）：

```powershell
npm install
npm run build 2>&1 | Tee-Object -FilePath build-frontend.log
npm run test:rust 2>&1 | Tee-Object -FilePath cargo-test.log
npx tauri build --debug --no-bundle 2>&1 | Tee-Object -FilePath build-debug.log
```

产物：`src-tauri/target/debug/formatforge.exe`（Windows，cargo 调试产物沿用内部包名）。
安装包内的主程序按产品名命名：`Transmutio.exe` / `Transmutio GPL.exe`（`mainBinaryName`）。

发布构建（安装包）：

```powershell
npm run build:lgpl 2>&1 | Tee-Object -FilePath samples\build-lgpl.log   # 默认变体
npm run build:gpl  2>&1 | Tee-Object -FilePath samples\build-gpl.log    # GPL 变体
npm run check:bundle                                                    # 解包检查合规物
```

`npm run build:lgpl|build:gpl` 会把安装包收集到 `dist-artifacts/<flavor>/`，
并把实际生效的 flavor、编译期开关、产物名/大小/SHA256 写入 `samples/build-<flavor>.json`。

`src/bin` 下的检查程序（`flavor-check` / `flavor-report` / `headless-check` / `setup-check`）
受 `dev-tools` feature 约束，**默认不构建、不进安装包**；验证脚本会显式加 `--features dev-tools`。

> `build.rs` 必须调用 `tauri_build::build()`：它负责生成并链接 Windows 资源
> （manifest / 图标 / 版本信息）。少了它，release exe 没有 `.rsrc` → 没有 comctl32 6.0
> 的 activation context，`TaskDialogIndirect` 解析不到，启动即报「无法找到入口」
> （`0xC0000139`）。`npm run check:bundle` 会对包内 exe 断言 manifest 存在。
> 排查脚本：`samples/probe/desktop-run.ps1`（在独立桌面启动并回报退出码，不弹可见窗口）。

> 运行 GUI（`npm run app:dev` 或双击 exe）属于前台操作，需先取得确认；
> 自动化验证请在后台执行 `npm run verify` / `npm run test:rust`。
>
> `jobs::start_scheduler` 是 `async fn`，**必须在 Tokio 运行时里 spawn**
> （应用侧 `tauri::async_runtime::spawn`，检查程序 `tokio::spawn`）。在 Tauri 的
> `setup` 同步闭包里直接调用会 panic（`there is no reactor running`），
> release 又是 `panic = "abort"`，表现就是启动即闪退（`0xC0000409`）。
> 崩溃信息会追加到 `<appData>/crash.log`（panic 文本 + backtrace，abort 前 fsync）。

## 持续集成（CI）

`.github/workflows/`：

| 工作流 | 触发 | 内容 |
| --- | --- | --- |
| `ci.yml` | `push` 到 `main` + `pull_request` | 前端 `npm ci` → 类型检查/构建 → `verify:formats`；后端 `cargo fmt --check`（阻断）→ `cargo clippy`（仅暴露警告）→ `cargo test`；无头 UI 在 `windows-latest` 跑 `panel-persist`（需浏览器，不需要 FFmpeg） |
| `release.yml` | 推送 `v*` 标签 | **唯一发布路径**：先跑版本闸门（tag / 版本文件 / alpha 序号），再由 `windows-latest` / `macos-latest` / `ubuntu-22.04` 三平台各构建 LGPL + GPL 变体（实际产物为 MSI/NSIS、DMG、deb/AppImage，`1.1.0` 共 10 个安装包），自动汇总 `SHA256SUMS` 并创建 Release。**产物不含 FFmpeg**，正文写明未做 Authenticode / 公证；版本闸门同时校验 alpha 序号规则。不再手工上传附件 |

- `verify:core` / `verify:setup` / `verify:flavor:*` / `p2-check` 依赖真实 FFmpeg，
  **只在本地或自托管 runner 执行**，GitHub 默认 runner 不跑。
- 本地等价：`cargo fmt --check`、`cargo clippy`、`cargo test`、`npm run build`、`verify:formats`、`panel-persist`。

更新通道方案见 [UPDATER.md](UPDATER.md)（已实装；`src-tauri/updater.pub` 已配置公钥，签名缺失或校验失败时安装 fail-closed）。

## 测试与验证

```powershell
npm run samples    # 生成真实样本库（中文/emoji/长路径/只读/0字节/损坏/120s/4K）
npm run verify     # 参数配方层 + 真实 ffmpeg 转换（无 GUI，报告 24/24）
npm run verify:core # 真实任务核心 + 真实 ffmpeg + 真实文件系统（无窗口，报告 40/40）
npm run verify:formats # 静态断言：容器 ≥4 / 音频 ≥5 / 不兼容组合 / 探测提示 / 卡片与存储前缀（30/30）
node scripts/panel-persist-check.mjs # 面板参数持久化 + 旧前缀零访问 + v4 界面 DOM 断言（headless，22/22）
npm run verify:ui   # 引导页 UI 层：headless Edge + mock 后端（LGPL / nonfree / GPL 三场景，39/39）
npm run verify:setup # 真实 LGPL ffmpeg 的 inspect → 落位 → 两变体判定 → 拒绝分支 → 闭环转码（12/12）
npm run verify:flavor:lgpl # LGPL 变体：编译期策略、隔离、拒绝 GPL 编码器（7/7）
npm run verify:flavor:gpl  # GPL 变体：接受 GPL 构建 + 真实 libx264 转 MP4（7/7）
npm run test:rust  # 变体策略 + 文件安全 / 命名 / 容器与音频格式矩阵单元测试（170/170，无 GUI）
```

验证层级不要混称：

| 命令 | 驱动对象 | 证据层级 |
| --- | --- | --- |
| `npm run verify` | 独立配方脚本（与 `args.rs` 手工对齐）+ 系统 ffmpeg | 参数配方 + 真实转码 |
| `npm run verify:core` | **应用同一份任务核心**（队列/调度/取消/发布），真实 ffmpeg、真实文件系统 | 真实核心链路（无窗口） |
| `npm run verify:ui` | `dist/` 前端产物，headless Edge + 注入的 Tauri IPC mock | **UI 层（headless）**，不驱动真实后端 |
| `npm run verify:setup` | **应用同一份 `install_blocking`** + 真实 LGPL/GPL ffmpeg、真实文件系统 | 真实落位链路（无窗口） |
| `npm run verify:flavor:*` | **应用同一份策略与任务核心**，分别以默认 / `--features gpl` 编译 | 变体策略 + 真实 x264 转码（无窗口） |
| `npm run verify:formats` | 读取 `formats.ts` / 两个面板源文件做静态断言 | 清单完整性（不是真实转码，真实转码见 `p2-check`） |
| `npm run check:bundle` | 真实安装包解包（NSIS 用 7z，MSI 用 WiX `dark.exe` + File 表还原文件名） | 合规物层（不是源码目录检查） |
| `npm run verify:app` | 真实应用进程 + WebView2 CDP | 应用级 E2E，但会启动窗口，默认禁止 |

- 报告：`samples/verify-report.json`、`samples/core-e2e-report.json`、
  `samples/setup-ui-report.json`、`samples/setup-install-report.json`、
  `samples/flavor-check-lgpl.json`、`samples/flavor-check-gpl.json`、
  `samples/build-<flavor>.json`、`samples/compliance-check.json`、
  `samples/app-e2e-report.json`（旧轮次，含 2 个已修复 bug，见文档）
- `verify:core` 覆盖：同名冲突不覆盖、4 路并发（running 峰值=4）、取消杀进程且无输出/无临时文件、
  损坏输入分类为 `input`、失败重试保留参数与输出路径
- 应用级链路 `npm run verify:app` 会启动可见窗口，默认拒绝执行，需显式确认：
  `FORMATFORGE_ALLOW_GUI_E2E=1`

历史实测摘要（Windows 11 / AMD RX 5700 XT / gyan.dev GPL 开发构建）：

- 视频：`h264_amf` 硬编 + `d3d11va` 硬解；4K→1080p 成功；120s 样本 6.7s 完成（17.9x）
- 硬编失败回退：用不可用的 `h264_nvenc` 复现，分类为 `hw-unavailable`，回退 libx264 成功
- 音频：MP3 705KB/44.1kHz、AAC 544KB/48kHz、视频提取音轨成功；六格式真实转码（LGPL FFmpeg：
  mp3 12.9KB、m4a/aac 13.1KB、flac 65.2KB、wav 88.3KB、ogg/vorbis 6.4KB、opus 15.3KB）
- 视频容器：mp4 / mkv / webm(vp9) / mov / ts / avi(mpeg4) 均用真实 ffmpeg 转码并由 ffprobe 复核
- 图片：PNG→JPG（mjpeg 1920x1080）、4K→1280x720 等比缩放、WebP 可用
- 并发：4 个进程同时转码全部成功（6.6s）；取消杀进程与临时文件清理在**真实任务核心**上通过
- 引导：真实 LGPL 构建（当时清单钉的是 BtbN `N-128352-gec887b13a8-20261103`；当前清单为 `N-128433-gdb8537d8cc`）落位 10/10，含错误哈希、nonfree、
  无版本、GPL 拒绝分支；用落位副本转 AAC 成功
- 未验证：被占用文件（占用模拟未成功）、macOS/Linux 真机、Apple Silicon 硬编

`1.1.0` 实测摘要（`verify:core` 41/41、`panel-persist` 16/16）：

- 矩形裁剪：1280×720 → x10%/y20%/50%×50% 输出 **640×360**，与独立参考
  `crop=640:360:128:144` 首帧 **PSNR 58.51 dB**；预览帧 PNG **480×270 / 33,573 字节**
- 设备预设：720p 源套用 apple-1080p → **1920×1080 / 30.00 fps / libx264**
- PDF→TXT：2 页真实 PDF → **52 字节 txt，两页文字均命中**（该能力已在当前版本下线，
  记录保留仅作 `1.1.0` 历史证据）
- 音频混音：两路 → **3.000s（取最长一路）/ 44.1 kHz / 立体声 / peak −21.5 dB**
- 升级：真实 `1.0.0 → 1.1.0` 静默覆盖安装 **0.92s**，启动到窗口 **0.19s**；
  匿名更新检查 **1.956s**、下载 3,090,747 B **6.182s**、minisign 校验通过

`1.1.1` 实测摘要（补丁版，Windows 11 真机）：

- 本地检查全绿：`verify` 24/24、`verify:core` 41/41、`panel-persist` 16/16、`verify:ui` 39/39、
  `verify:setup` 12/12、`cargo test` 170/170、`verify:formats` 31/31、`version-rules` 18/18
- CI 发布：run `42337017451` 三平台（Windows / macOS / Linux × LGPL / GPL）全部成功，
  Release `v1.1.1` 于 2026-11-07T04:24:05Z 发布，共 24 个资产（10 安装包 + 10 `.sig` +
  `latest.json` / `latest-gpl.json` / `SHA256SUMS`）
- 匿名核对（不带 token 直接下载）：`Transmutio_1.1.1_x64-setup.exe` 3,091,124 B、
  `Transmutio.GPL_1.1.1_x64-setup.exe` 3,087,516 B、`Transmutio_1.1.1_aarch64.dmg`
  4,004,794 B，三者 SHA-256 与 `SHA256SUMS`、`latest.json` 的 `assetSha256` 完全一致
- 更新检查与下载（基线 `1.1.0`）：检查 **2.040s**（首选 GitHub 官方命中）、下载 3,091,124 B
  **14.668s**（合计 16.895s）；大小、SHA-256 与 minisign 签名全部通过（独立复核 `Ok(())`）
- 真实升级 `1.1.0 → 1.1.1`：静默覆盖安装 **0.88s**，注册表 `DisplayVersion` 变为 **1.1.1**，
  `Transmutio.exe` 更新为 8,359,424 B；启动到窗口 **0.451s**（首次）/ **0.311s**（再次）
- 说明：代码中不存在 `last_install` 字段，故以「注册表版本 + 安装耗时 + 启动耗时」作为升级
  落地的等价证据，不额外编造指标
- 未验证：macOS / Linux 真机运行

## 签名与分发（第一版流程已就位）

Windows（先 sidecar 后主程序/安装包）：

```powershell
$env:WINDOWS_CERT_THUMBPRINT="..."
./scripts/sign-windows.ps1 -SidecarsOnly
npx tauri build 2>&1 | Tee-Object -FilePath build-release.log
./scripts/sign-windows.ps1 -Bundle
```

macOS（sidecar 先签 → 构建 → app 签名 → 公证 → staple）：

```bash
APPLE_SIGN_IDENTITY="Developer ID Application: ..." ./scripts/sign-macos.sh
npx tauri build
APPLE_SIGN_IDENTITY="..." NOTARY_PROFILE="..." ./scripts/sign-macos.sh
```

- entitlements：`src-tauri/entitlements/app.entitlements.plist`、`ffmpeg.entitlements.plist`
- 无证书时脚本会打印占位流程并退出 0，不会伪造签名
- 签名链不完整会导致 sidecar 被 Gatekeeper 拒绝执行，表现是所有转换失败（含硬编）

## 目录结构

```
formatforge/
├─ packages/                 # 可复用基础层（独立接口 + README）
│  ├─ design-tokens/         # 设计令牌、主题、格式化
│  ├─ i18n/                  # 多语言骨架（zh-CN 完整、en-US 骨架）
│  ├─ settings/              # 设置存储（StorageAdapter 可替换）
│  ├─ task-queue/            # 任务队列抽象（TaskTransport 可替换）
│  ├─ telemetry/             # 日志缓冲 + 崩溃上报
│  └─ updater/               # 自动更新接口（包内仅 Null 适配器；实际更新见 src-tauri/src/updater.rs）
├─ src/                      # React 界面（6 分区、方法网格、Inspector、任务中心）
├─ src-tauri/                # Rust 后端（工具解析、能力探测、参数、任务调度）
├─ scripts/                  # 样本生成、无 GUI 验证、签名脚本
├─ samples/                  # 生成的样本与验证报告（不入包）
└─ README / LICENSING / GPU-能力矩阵 / MVP-完成度与验证 / FEATURE-GAP / UPDATER / CONTRIBUTING
```

## 已知限制与未验证项

- 默认包**不含 FFmpeg**；未放置 LGPL 二进制时无法转换（这是有意设计，不伪造成功）。
- LGPL 构建**没有软件 HEVC 编码器**（无 libx265）；软件 H.264 取决于构建是否带 libopenh264
  （当前清单钉死的 BtbN LGPL 构建带 `--enable-libopenh264`，见
  `src-tauri/resources/licenses/SOURCE-ACQUISITION-LGPL.txt`）。既无硬件编码器、构建又不带 openh264 时，H.264 编码不可用。
- 图片编码器没有逐项试编，缺少 libwebp 时任务会失败并给出原因。
- 被占用文件的错误提示未完成自动化验证。
- 界面仅提供简体中文；语言切换入口已移除（en-US 词典仍是未完成骨架），不再保留点了没反应的选项。
- 引导页：UI 层（headless + mock）与真实 LGPL 落位已验证；**未在真实窗口内点击**（禁止 GUI），
  「复制损坏 → 复核回滚」分支与 macOS/Linux 落位未验证。
- macOS / Apple Silicon 的 VideoToolbox 能力、并发上限、B 帧/10bit 行为：按文档实现，待真机验证。
- 自动更新已实装（多镜像回退、断点续传、minisign 校验、安装后重启）；`src-tauri/updater.pub`
  已配置公钥，1.1.0 更新包的 `.sig` 已实测通过 minisign 校验；签名缺失或校验失败时安装
  按 fail-closed 拒绝，不会跳过校验。
- Windows/macOS **Authenticode / 公证未做**（本机无证书），安装包与 macOS `.app` 可能触发
  SmartScreen / Gatekeeper 提示；这与更新包的 minisign 签名是两套机制。

### 当前未实现项（如实列出，入口置灰或明确写进已知限制）

- Delogo（去水印区域）
- 音频混音的逐轨音量推子与试听波形——当前为等权混合（时长取最长一路），不提供拖拽音量配比
- PDF 相关能力（合并 / 拆分 / 转图片 / 转文本 / PDF→DOC、XLS、HTM）已整体下线，不再作为待实现项
- 外挂字幕渲染与多轨切换——支持把外挂字幕内嵌（soft）或烧录（hard），但不支持播放器式外挂加载/切换
- 文档 Office 互转、光盘 / DVD / ISO 抓取、工具集（批量改名、校验、信息、截图）——
  这些入口已随导航分区一并移除，界面不再展示
- macOS / Linux：以上仅为 Windows 实测结论，其他平台未验证

## 文档索引

- [MVP-完成度与验证.md](MVP-完成度与验证.md)
- [GPU-能力矩阵.md](GPU-能力矩阵.md)
- [FEATURE-GAP.md](FEATURE-GAP.md)（对标产品功能对账，产品名已中性化、不保留链接与版本号）
- [LICENSING.md](LICENSING.md)
- [CONTRIBUTING.md](CONTRIBUTING.md)
- [UPDATER.md](UPDATER.md)（更新通道方案，已启用）
- 上一轮方案与设计稿：工作区上级目录（未纳入本仓库）
