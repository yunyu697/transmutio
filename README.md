# Transmutio（MVP）

跨平台桌面转码工具（Windows / macOS / Linux），Tauri 2 + React 19 + TypeScript，
FFmpeg 以 sidecar 子进程运行。本仓库是独立工程，与「菲菲更名宝贝」不共享代码目录。

## 结论（先看这里）

| 项目 | 状态 |
| --- | --- |
| 可构建运行的应用工程 | ✅ Windows debug 构建通过，窗口可启动（构建日志 `build-debug.log`） |
| 视频转码（6 容器） | ✅ 已跑通：MP4 / MKV / WebM / MOV / TS / AVI；1080p→720p、4K→1080p、120s 长时长、硬解 d3d11va + `h264_amf` 硬编、硬编失败自动回退软编 |
| 音频转码（6 格式） | ✅ 已跑通：MP3 / AAC(m4a) / FLAC / WAV / OGG(Vorbis) / Opus；含从视频提取音轨；无损格式自动跳过码率参数 |
| 图片批量转换 / 缩放 | ✅ 已跑通：PNG→JPG、WebP、4K→最长边 1280（不拉伸） |
| 任务中心 | ✅ 4 槽并发、进度、取消杀子进程、失败重试、打开输出目录；无窗口核心链路 7/7 实测 |
| 用户文件安全 | ✅ 不覆盖（硬链接原子发布）、临时文件、取消清理；删除源文件未实现（默认关闭） |
| 首次启动引导 | ✅ 缺失或 GPL/nonfree 时展示；可选「按内置清单下载」（官方源 + 镜像回退 + 内置 SHA-256 校验 + 断点续传），也可手动选文件；UI 层 headless 26/26 + 真实 LGPL 落位 10/10（见文档 §4.2、§4.22） |
| 默认许可路线 | ✅ 仅 LGPL 放行；GPL/nonfree 默认禁用，仅开发开关 |
| macOS / Apple Silicon | ⚠️ **按文档实现、待真机验证**（本机无 Apple 设备，未实测） |
| Linux | ⚠️ **按文档实现、待真机验证**（未在 Linux 上构建运行） |

MVP 完成度与逐项证据见 [MVP-完成度与验证.md](MVP-完成度与验证.md)；
许可待决策项见 [LICENSING.md](LICENSING.md)；
GPU 能力矩阵与实测/文档/待验证标注见 [GPU-能力矩阵.md](GPU-能力矩阵.md)。

## 功能范围（80/20）

已实现（入口可用，真实转换）：

- 视频：视频转码（容器 MP4 / MKV / WebM / MOV / TS / AVI，编码器按容器联动，H.264 / HEVC / AV1 / VP9 / VP8 / MPEG-4，分辨率、帧率、码控、音频、硬解、faststart）、压缩、旋转 / 比例裁剪、多文件合并、音视频混流、转 GIF、变速（0.25–4.0，保持音高）、水印（图片 / 文字）、字幕（内嵌 / 烧录）、相对音量增益（−20…+20 dB，可静音）、按时间点分割（流复制 / 重编码）
- 图片：批量转换（JPG / PNG / WebP / BMP / TIFF）、批量缩放、批量裁剪（比例 / 自定义矩形）、网格拼图、加水印、目标大小 / 固定质量压缩、多图转 GIF（palettegen + paletteuse）
- 序列：图片序列 → 视频（fps / 起始编号 / 尺寸统一 / 编码器）、视频 → 图片序列
- 音频：转码（MP3 / AAC→m4a / FLAC / WAV / OGG-Vorbis / Opus）、音量标准化（EBU R128）、相对音量增益
- 文档：PDF 合并、拆分（每页 / 页码范围，内置 lopdf，零外部依赖）、PDF 转图片（PNG / JPG + DPI，需外部 pdftoppm）
- 配置：面板参数 + 输出策略的预设导入 / 导出（版本与字段严格校验，不含路径与凭据）
- 任务：4 槽并发、暂停 / 继续、命名模板、冲突策略、自动重试（仅 hw-unavailable / 临时 IO）

保留入口但**明确标注「未实现」**，点击只提示、不执行任何假转换：

- 文档：文档转 PDF、Office 互转
- 图片：EXIF 元数据写入、AVIF 编码
- 光驱 / DVD / ISO、工具集（改名、校验、信息、截图等）
- 更新器实装（接口与方案已就绪，未接真实发布通道）

### 格式清单与探测的关系

格式清单是**固定的完整矩阵**（视频 6 容器 × 兼容编码器、音频 6 格式），不随探测结果增减：

- 单一事实来源：后端 `src-tauri/src/containers.rs`，前端 `src/lib/formats.ts`（两者同构，由 `npm run verify:formats` 断言）；
- 探测（`ffmpeg -encoders` + 试编）只决定「哪些选项可用」：缺编码器的容器 / 格式**灰显并给出原因**，不会从列表消失；
- 不兼容组合（如 MP4 + VP9、WebM + AAC）在界面上不出现在可选项里，后端入队前再次拒绝；
- 探测未完成时显示「正在探测本机编码器…清单已完整渲染」；无 ffmpeg 或无可用编码器时显示
  「未检测到 ffmpeg 或编码器不可用」，**不会静默退化为只剩 mp4/mp3**。

### 本地数据与升级

- 设置与面板参数现使用 `transmutio:*` 键；启动时会把旧的 `formatforge:*`（设置 + 全部面板参数）
  一次性迁移过来：**写入新键 → 读回校验 → 通过后才删旧键**，任一键失败保留旧键；标记键
  `transmutio:storage-migration:v1` 保证只迁移一次。
- 为兼容旧版回滚，一个版本周期内写入会**双写**旧前缀（`TODO` 已标注到期删除）；读取优先新键，
  新键缺失时回退旧键。
- 「PDF 转图片」入口常驻可点：缺 `pdftoppm` 时进入面板会给出 Poppler 安装路径（设置页
  「PDF 工具目录」或 `PDFTOPPM_PATH`）与「重新检测」按钮，查找顺序仍是
  `PDFTOPPM_PATH → 设置目录 → TRANSMUTIO_PDF_TOOLS → PATH`。

## 下载与许可

许可分两层，**互不替代**，请分开看：

| 层 | 对象 | 许可 |
| --- | --- | --- |
| ① 应用代码 | 本仓库全部源码（含 `src/`、`src-tauri/`、`scripts/`、`packages/` 等所有子目录） | **MIT**，全文见根目录 [LICENSE](LICENSE)，覆盖整个仓库 |
| ② 第三方二进制 | FFmpeg / Poppler（安装包**不包含**，用户自行获取） | 各自的 LGPL / GPL 路线，见下文与 [LICENSING.md](LICENSING.md) |

- **应用代码 = MIT**：任何人可自由使用、修改、再分发本仓库代码，只需保留版权与许可声明。
- **FFmpeg / Poppler = 各自的 LGPL / GPL 路线**：这两个二进制不属于本仓库代码，
  也不随安装包分发。LGPL 变体只接受 LGPL 构建的 FFmpeg；GPL 变体只接受 GPL 构建的 FFmpeg
  （可用 libx264 / libx265）。Poppler 为 GPL-2.0+，仅在「PDF 转图片」功能中作为外部工具使用。
  第三方许可证全文保持原样，位于 `licenses/`。

- 发布页：<https://github.com/yunyu697/transmutio/releases>（`v0.1.0` 为历史版本；
  当前为 `1.0.0-alpha*` 预发布，不建议日常使用）
- **安装包不包含 FFmpeg / Poppler 二进制**。仓库与 Release 附件都不分发这两个二进制：
  FFmpeg 由用户自行准备并在首次启动时经引导页校验（版本 / 构建配置 / SHA-256）后落位到
  应用数据目录；「PDF 转图片」需用户自行安装 Poppler 的 `pdftoppm`。因此本项目不承担
  FFmpeg/Poppler 二进制再分发的源码提供义务；若你要再分发它们，请自行履行对应许可证。
- 两个安装包的区别与所选许可路线：

  | 包 | 接受的 FFmpeg 构建 | 含义 |
  | --- | --- | --- |
  | `Transmutio_<版本>_x64_*`（LGPL 变体，默认） | 仅 LGPL 构建（拒绝 `--enable-gpl` / `--enable-nonfree`） | 选择此包 = 只使用 LGPL 许可的 FFmpeg 与编码器（如 libopenh264 / libvpx / SVT-AV1 / aac / libmp3lame） |
  | `Transmutio GPL_<版本>_x64_*`（GPL 变体） | 仅 GPL 构建（含 libx264 / libx265） | 选择此包 = 接受 GPL 许可的 FFmpeg 构建，可用 x264 / x265 |

  两包共用同一份应用源码（同一 MIT 许可），差异仅为编译期 `gpl` feature + Tauri overlay
  （独立应用标识与数据目录）。上表的「LGPL / GPL」只描述**所接受的 FFmpeg 构建**，
  不是应用代码的双许可。
- 许可证全文：`licenses/FFmpeg-LGPL.txt`（LGPL-2.1）、`licenses/FFmpeg-GPL.txt`（GPL-2.0）、
  `licenses/Poppler.txt`（GPL-2.0+）。安装包内另附对应变体的许可证、第三方声明与源码获取说明
  （`src-tauri/resources/licenses/`）。
- 源码获取方式（本项目**未修改** FFmpeg 与 Poppler 源码）：
  - FFmpeg：<https://ffmpeg.org/download.html>；源码 <https://git.ffmpeg.org/ffmpeg.git> 或
    <https://github.com/FFmpeg/FFmpeg>。发布验证使用的参考构建：LGPL
    `N-128352-gec887b13a8-20261103`（BtbN/FFmpeg-Builds，git 提交 `ec887b13a8`），
    configure 参数全文见 `src-tauri/resources/licenses/SOURCE-ACQUISITION-LGPL.txt`；
    GPL 参考构建（gyan.dev）见 `SOURCE-ACQUISITION-GPL.txt`。
  - Poppler：<https://poppler.freedesktop.org/>；源码
    <https://gitlab.freedesktop.org/poppler/poppler>。本仓库不分发其二进制。
- **安装包未签名**（无 Authenticode / Apple 公证），Windows SmartScreen 可能提示未知发布者。

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
- 落位到 `<应用数据目录>/tools/ffmpeg-<版本>/`，随后重新探测能力表。

当前内置清单的可自动下载包：**Windows x64** 的 FFmpeg LGPL / GPL（BtbN 固定构建）与
Poppler（`pdftoppm`）。**macOS / Linux 没有可固定 SHA-256 的官方发布包**，因此不提供自动
下载，引导页会给出可读原因与手动/包管理器方案（不会伪造一个跑不通的下载项）。

### B. 手动选择本地文件

1. 引导页给出可配置的 LGPL 下载源（默认 FFmpeg 官方下载页，链接用系统浏览器打开）
2. 选择已下载的 `ffmpeg` 可执行文件，应用读取版本、构建配置、编码器列表与 SHA-256
3. 可选填入发布页提供的期望哈希做比对；通过后复制到 `<应用数据目录>/tools/ffmpeg-<版本>/`
4. 落位后复核哈希并持久化路径，随后重新探测能力表

明确不做：不静默下载可执行文件、不代替用户接受许可、不接受 nonfree 组件；
GPL 构建只有在设置中显式开启「允许 GPL」后才可用于本机开发。引导页在无 FFmpeg 时也能正常展示，
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

## PDF 与可选外部工具（pdftoppm）

| 功能 | 依赖 | 缺失时的行为 |
| --- | --- | --- |
| PDF 合并 / 拆分 | **无外部依赖**。内置 `lopdf`（纯 Rust，静态链接进主程序） | — |
| PDF 转图片 | 外部 **Poppler** 的 `pdftoppm`（需 `pdftoppm.exe` 及其 DLL 目录） | 方法列表里的「PDF 转图片」卡片置灰并标记「需外部工具」；点击给出完整原因与安装建议；后端入队再次拒绝。不会进入面板后静默失败 |

`pdftoppm` 查找顺序（任一命中即可）：

1. 环境变量 `PDFTOPPM_PATH`（直接指向 exe）
2. 设置页选择的 PDF 工具目录（同时查找 `<dir>\Library\bin` 与 `<dir>\bin`）
3. 环境变量 `TRANSMUTIO_PDF_TOOLS`（目录列表）
4. 系统 `PATH`

仓库与安装包**不分发** Poppler。Windows 上可在「PDF 转图片」面板点
**自动下载 Poppler**：走同一份内置清单（固定版本 + 内置 SHA-256 + 镜像回退），校验通过后
解压到 `<应用数据目录>/tools/poppler-<版本>/Library/bin/`，并自动写入设置页的 PDF 工具目录；
校验失败会拒绝落位并给出原因。macOS / Linux 没有可固定校验和的包，面板会给出
`brew install poppler` / `apt install poppler-utils` 的说明（不会伪造下载项）。
也可以手动安装 Poppler for Windows（便携包解压后把 `Library\bin` 配进 `PDFTOPPM_PATH` 或设置页），
或用 `scoop install poppler` / `choco install poppler`。合并与拆分不受影响。

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

**旧版不会原地升级。必须先卸载旧版，再安装 Transmutio；卸载时不要删除应用数据。**

- 旧版 `FormatForge` 与新版 `Transmutio` 的 `identifier`、产品名、安装目录、卸载注册表项全部不同，
  Windows 视为两个产品；允许并存但不支持并存。安装器会检测旧版（显示名 + 旧 MSI UpgradeCode），
  命中时提示先卸载并中止安装，不会写入半套文件。
- 正确顺序：卸载旧版（保留应用数据）→ 安装 Transmutio → 首次启动自动迁移
  `%APPDATA%\app.formatforge.desktop*` 与 `%LOCALAPPDATA%\app.formatforge.desktop*`（含 WebView2
  的设置/面板参数）。迁移采用「同卷临时目录 → 校验文件数与字节数 + SHA-256 → 原子 rename → 旧目录保留」；
  失败只清临时目录，旧目录不动、新目录不留半套。
- 若两版目录已经并存（或迁移被跳过），可在新版「设置 → 诊断与更新 → 从旧版导入数据」手动导入；
  导入前会对当前数据整份备份，WebView2 设置在下一次启动生效。
- 迁移完成后旧目录仍保留，可确认无误后手动删除。

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
| `release.yml` | 推送 `v*` 标签 | **唯一发布路径**：先跑版本闸门（tag / 版本文件 / alpha 序号），再由 `windows-latest` / `macos-latest` / `ubuntu-22.04` 三平台各构建 LGPL + GPL 变体，产物（MSI/NSIS、DMG、deb/rpm/AppImage）自动汇总 `SHA256SUMS` 并创建 Release。**产物不含 FFmpeg/Poppler**，正文写明未签名与 alpha 不建议日常使用。不再手工上传附件 |

- `verify:core` / `verify:setup` / `verify:flavor:*` / `p2-check` 依赖真实 FFmpeg（部分还需 Poppler），
  **只在本地或自托管 runner 执行**，GitHub 默认 runner 不跑。
- 本地等价：`cargo fmt --check`、`cargo clippy`、`cargo test`、`npm run build`、`verify:formats`、`panel-persist`。

更新通道方案见 [UPDATER.md](UPDATER.md)（本轮**只出方案，未生成密钥、未启用**）。

## 测试与验证

```powershell
npm run samples    # 生成真实样本库（中文/emoji/长路径/只读/0字节/损坏/120s/4K）
npm run verify     # 参数配方层 + 真实 ffmpeg 转换（无 GUI，报告 23 PASS / 1 SKIP）
npm run verify:core # 真实任务核心 + 真实 ffmpeg + 真实文件系统（无窗口，报告 31/31）
npm run verify:formats # 静态断言：容器 ≥4 / 音频 ≥5 / 不兼容组合 / 探测提示 / 卡片与迁移（10/10）
node scripts/panel-persist-check.mjs # 面板参数持久化 + localStorage 前缀迁移（headless，11/11）
npm run verify:ui   # 引导页 UI 层：headless Edge + mock 后端（LGPL / nonfree / GPL 三场景，39/39）
npm run verify:setup # 真实 LGPL ffmpeg 的 inspect → 落位 → 两变体判定 → 拒绝分支 → 闭环转码（12/12）
npm run verify:flavor:lgpl # LGPL 变体：编译期策略、隔离、拒绝 GPL 编码器（7/7）
npm run verify:flavor:gpl  # GPL 变体：接受 GPL 构建 + 真实 libx264 转 MP4（7/7）
npm run test:rust  # 变体策略 + 文件安全 / 命名 / 容器与音频格式矩阵单元测试（63/63，无 GUI）
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

本次实测摘要（Windows 11 / AMD RX 5700 XT / gyan.dev GPL 开发构建）：

- 视频：`h264_amf` 硬编 + `d3d11va` 硬解；4K→1080p 成功；120s 样本 6.7s 完成（17.9x）
- 硬编失败回退：用不可用的 `h264_nvenc` 复现，分类为 `hw-unavailable`，回退 libx264 成功
- 音频：MP3 705KB/44.1kHz、AAC 544KB/48kHz、视频提取音轨成功；六格式真实转码（LGPL FFmpeg：
  mp3 12.9KB、m4a/aac 13.1KB、flac 65.2KB、wav 88.3KB、ogg/vorbis 6.4KB、opus 15.3KB）
- 视频容器：mp4 / mkv / webm(vp9) / mov / ts / avi(mpeg4) 均用真实 ffmpeg 转码并由 ffprobe 复核
- 图片：PNG→JPG（mjpeg 1920x1080）、4K→1280x720 等比缩放、WebP 可用
- 并发：4 个进程同时转码全部成功（6.6s）；取消杀进程与临时文件清理在**真实任务核心**上通过
- 引导：真实 LGPL 构建（BtbN `N-128352-gec887b13a8-20261103`）落位 10/10，含错误哈希、nonfree、
  无版本、GPL 拒绝分支；用落位副本转 AAC 成功
- 未验证：被占用文件（占用模拟未成功）、macOS/Linux 真机、Apple Silicon 硬编

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
│  └─ updater/               # 自动更新接口（当前 Null 实现）
├─ src/                      # React 界面（6 分区、方法网格、Inspector、任务中心）
├─ src-tauri/                # Rust 后端（工具解析、能力探测、参数、任务调度）
├─ scripts/                  # 样本生成、无 GUI 验证、签名脚本
├─ samples/                  # 生成的样本与验证报告（不入包）
└─ README / LICENSING / GPU-能力矩阵 / MVP-完成度与验证 / CONTRIBUTING
```

## 已知限制与未验证项

- 默认包**不含 FFmpeg**；未放置 LGPL 二进制时无法转换（这是有意设计，不伪造成功）。
- LGPL 构建通常没有软件 H.264 编码器（无 libx264、未必有 libopenh264），
  因此 LGPL 发布版要么依赖硬件编码，要么引入 OpenH264（见 LICENSING.md 待决策项）。
- 图片编码器没有逐项试编，缺少 libwebp 时任务会失败并给出原因。
- 被占用文件的错误提示未完成自动化验证。
- 引导页：UI 层（headless + mock）与真实 LGPL 落位已验证；**未在真实窗口内点击**（禁止 GUI），
  「复制损坏 → 复核回滚」分支与 macOS/Linux 落位未验证。
- macOS / Apple Silicon 的 VideoToolbox 能力、并发上限、B 帧/10bit 行为：按文档实现，待真机验证。
- 自动更新仅有接口与占位，没有真实更新端点与签名密钥。
- Windows/macOS 签名脚本已写好但本机无证书，未实际签名。

### 当前未实现项（如实列出，入口置灰）

- 水印、字幕、变速、视频音量、视频分割
- 图片压缩、加水印、图片转 GIF、EXIF 元数据写入
- 文档转 PDF、Office 互转
- 光盘 / DVD / ISO 抓取
- 工具集（批量改名、校验、信息、截图）

## 文档索引

- [MVP-完成度与验证.md](MVP-完成度与验证.md)
- [GPU-能力矩阵.md](GPU-能力矩阵.md)
- [LICENSING.md](LICENSING.md)
- [CONTRIBUTING.md](CONTRIBUTING.md)
- [UPDATER.md](UPDATER.md)（更新通道方案，未启用）
- 上一轮方案与设计稿：`../FormatFactory-跨平台重构方案.md`、`../FormatForge-UI-Design.html`
