# MVP 完成度与验证说明

对应要求：三条主线跑通（探测 → 参数 → 入队 → 并发调度 → 进度/取消 → 输出与失败重试），
其余分区保留入口但标未实现；所有验证**后台 CLI 执行，无 GUI**。

---

## 1. 总体结论

| 项目 | 完成度 | 证据等级 |
| --- | --- | --- |
| 视频转码（6 容器） | 可用（MP4 / MKV / WebM / MOV / TS / AVI；硬解/硬编/回退、恒定质量或目标码率、90°/180°/270°、16:9/1:1/9:16 居中裁剪、缩放、帧率、音频、faststart） | **后台实测**（真实 ffmpeg + ffprobe 产物断言，§4.1 / §4.12） |
| 音频转码（6 格式） | 可用（MP3 / AAC→m4a / FLAC / WAV / OGG-Vorbis / Opus；含从视频提取音轨；采样率/声道；EBU R128 响度归一化） | **后台实测**（§4.12 六格式真实转码） |
| 图片批量转换 / 缩放 / 质量 | 可用（JPG/PNG/WebP/BMP/TIFF + 最长边缩放 + 质量） | **后台实测** |
| 任务中心（4 槽并发、进度、取消、重试、打开目录、任务日志） | 可用 | **真实核心链路 11/11**（§4.1）；UI 交互未在窗口验收 |
| 任务诊断日志（每任务完整 stderr 落盘 + 任务中心打开） | 可用 | **真实核心链路**（失败任务日志断言，§4.1） |
| 文件安全（不覆盖、临时文件、原子发布、默认不删源） | 可用 | **单元测试 + 真实核心验证** |
| 首次启动 FFmpeg 引导 | 可用（校验 → 落位 → 持久化 → 用落位后的二进制转码） | **UI 层（headless）39/39** + **真实落位 12/12**（§4.2） |
| GPL 双变体（LGPL 默认 + GPL 变体） | 可用（编译期注入、独立身份/数据目录/通道、真实 x264 转码、合规物进包） | **构建层 2 套产物 + 逻辑层 7/7 + 真实转码 + 解包检查**（§4.3） |
| GPU 能力探测与自动降级 | 可用 | Windows **实测**；macOS/Linux **按文档、待真机** |
| macOS / Linux 运行 | 代码就绪、未构建运行 | **未验证** |
| 签名与分发流程 | 脚本与占位就绪、未实际签名 | **未执行** |

工程可构建：`npm run build:lgpl` / `npm run build:gpl` 各产出一套安装包（见 §4.3）；
`npm run tauri build` 仍为 LGPL 默认。前端 `npx tsc --noEmit && vite build` 通过；
Rust 单元测试 16/16 通过（`cargo-test.log`；新增 5 条滤镜/归一化参数单测）。

---

## 2. 已实现（按链路）

| 环节 | 实现 |
| --- | --- |
| 探测 | `ffprobe` JSON 取时长/分辨率/编码；0 字节在入队前拒绝；损坏文件报 `input` 错误 |
| 参数 | 视频：H.264/HEVC、编码器（自动/仅软件/手动）、恒定质量或目标码率、分辨率、帧率、**旋转（0/90/180/270）**、**居中裁剪（16:9 / 1:1 / 9:16）**、音频、硬解开关、faststart；音频：MP3/AAC、码率/采样率/声道、**EBU R128 响度归一化**；图片：格式、质量、最长边 |
| 入队 | 同一把锁内选「不冲突输出名 + 入队」，并发入队不会抢同一文件名 |
| 调度 | 自研调度器：`Notify` + 队列 + running 计数，默认 4 槽，运行时可调 1–8 |
| 进度 | 解析 `-progress pipe:1` 的 `out_time_us` / `speed` / `progress=end`，前端实时刷新 |
| 取消 | 中止等待任务 → `kill_on_drop` 终止子进程，Windows 追加 `taskkill /T /F`，Linux `kill -TERM`；清理临时文件（12×100ms 重试应对句柄延迟） |
| 输出 | 写 `.ff-tmp-<id>.part.<ext>`，成功后 `hard_link` 原子发布；目标已存在则返回 `AlreadyExists`，**绝不覆盖**，改名为 `name (1).ext` |
| 重试 | `retry_job` 重排队，`attempts` 递增；失败保留错误分类与 stderr 摘要 |
| 结果 | 任务中心可打开输出目录；输出大小/耗时/实际编码器/回退说明可见 |
| 日志 | 每任务写 `<appData>/logs/job-<id>.log`（完整参数 + 最多 400 行 ffmpeg stderr）；任务卡片「日志」按钮直接打开 |

### 未实现（入口保留、点击只提示，不做假转换）

视频：暂无（音量增益、按时间点分割已在第九轮落地，见 §4.19）。
图片：EXIF 元数据写入、AVIF 编码。
文档：文档转 PDF、Office 互转。
光驱 / DVD / ISO、工具集（改名、校验、信息、截图）全部仅入口占位。
（变速、水印、字幕、图片压缩、音量、分割、图片 GIF 已在第五～九轮落地，见 §4.14–§4.19；总账见 §4.18。）

> 本节曾把合并 / 混流 / GIF / 裁剪 / 拼图 / PDF / 预设 / 持久化列为未实现，
> 这些已由 P0/P1/P2 三轮补齐，逐项现状见 §4.6–§4.12 的入口对账表。

### 共享基础层（可复用包）

`packages/design-tokens`、`i18n`、`settings`（`StorageAdapter`）、`task-queue`（`TaskTransport`）、
`telemetry`（`LogSink` + 崩溃上报）、`updater`（`UpdaterAdapter`，当前 Null 实现）。
各包独立目录与 README，业务层只依赖接口；未从「菲菲」工程读取或复制任何代码。

---

## 3. 真实样本库（`scripts/make-samples.mjs` 已执行）

清单：`samples/manifest.json`。样本覆盖：中文名、emoji、空格与引号、超长路径（219 字符）、
只读源、被占用（模拟）、0 字节、损坏、120s 长时长、4K 3840x2160、WAV、中文 PNG。

---

## 4. 配方层验证（`npm run verify`）

> 层级说明：`verify-mvp.mjs` 是**独立脚本**，参数配方与 `args.rs` 手工对齐，
> 用系统 ffmpeg 做真实转码。它能证明「命令配方 + 真实 ffmpeg 可行」，但**不驱动应用任务核心**。
> 驱动真实核心的是 §4.1。

报告：`samples/verify-report.json`（最近一次 `2026-11-04T12:28Z`，在 VAAPI 分支与
「每次清空输出目录」改动后重跑）。汇总 **23 PASS / 0 FAIL / 1 SKIP / 24**。

| 用例 | 实测结果 |
| --- | --- |
| 运行时编码器探测 | 硬编 `h264_amf`；`h264_nvenc`/`h264_qsv` 试编失败；软件 `libx264`（GPL，仅开发） |
| 自动编码器选择 | 选中 `h264_amf`（硬件） |
| 视频转 MP4（1080p→720p） | `base-720p.mp4` h264 1280x720 3.00s 1345KB；`d3d11va` 硬解 + AMF 硬编 |
| 硬编失败分类与回退 | 分类 `hw-unavailable` → 回退 `libx264` 成功 |
| 中文文件名 | `中文 视频 样本-转换.mp4` 1345KB，源文件未修改 |
| emoji 文件名 | `🎬 emoji 视频-转换.mp4` 1345KB，源文件未修改 |
| 空格与引号 | `空格 与 ' 引号' 视频-转换.mp4`，未因 shell 引号出错（全程参数数组，无 shell 拼接） |
| 超长路径（219 字符） | 转码成功 |
| 只读源文件 | 转码成功，源文件未被修改 |
| 120s 长时长 | 耗时 6.7s，约 17.9x |
| 4K → 1080p | 输出 1920x1080 |
| 进度解析 | 末条 `out_time_us=120000000`，speed 正常，`progress=end=true` |
| 冲突自动改名 | 已存在 `冲突测试.mp4` → 输出 `冲突测试 (1).mp4`，原文件 mtime 未变 |
| 取消 | 子进程已杀死；取消时临时文件 2048KB，清理后不存在 |
| 4 路并发 | 4 个进程同时运行，4/4 成功，总耗时 6.6s |
| 音频 MP3 | 705KB / 44100Hz |
| 音频 AAC | 544KB / 48000Hz |
| 视频提取音轨转 MP3 | 3.0s 成功 |
| 图片 PNG→JPG | `mjpeg` 1920x1080 101KB |
| 图片批量缩放 | 4K → 1280x720，等比未拉伸 |
| 图片 WebP | `libwebp` 可用，转换成功 |
| 0 字节 | 入队前拒绝（应用侧） |
| 损坏文件 | `moov atom not found`，被识别为输入错误 |
| **被占用文件** | **SKIP：未能可靠模拟 Windows 文件占用**（**未验证**） |

### 单元测试（`npm run test:rust` → `cargo-test.log`）

4/4 通过：预留名避让、无已存在文件时编号、绝不覆盖、`rename_atomic` 拒绝已存在目标。

---

## 4.1 真实核心链路（无窗口，`npm run verify:core`）

任务核心已从 Tauri `AppHandle` 解耦为 `jobs::Ctx`（`ARC<AppState>` + 事件回调），
因此可以在**不启动任何窗口**的情况下，用与应用命令**同一份代码**驱动队列、调度、取消与发布。

- 入口：`src-tauri/src/bin/headless-check.rs` → `headless::run_checks`
- 报告：`samples/core-e2e-report.json`（本轮功能补齐后重跑）：**11 PASS / 0 FAIL**
- 证据层级：真实任务核心 + 真实 ffmpeg 子进程 + 真实文件系统；无 UI、无 mock
- 已知注入项：事件回调为静默实现（不验证消息通道），能力表为手工注入（真实探测由 §4 覆盖）

| 用例 | 实测结果 |
| --- | --- |
| 生成素材 | 60s / 720p 与 8s 素材，均由真实 ffmpeg 生成 |
| 同名输出冲突 | 得到 `同名输出.mp4` 与 `同名输出 (1).mp4`，两任务都 `done`、文件均 >1KB、无临时残留 |
| 4 槽并发 | `running` 峰值 **4**，4/4 完成，临时残留 0 |
| 取消（运行中） | 状态 `cancelled`，输出未创建，4s 后无 `.ff-tmp-*` 残留 |
| 失败分类 | 损坏输入 → `errorKind=input`，保留 stderr 摘要 |
| 失败重试 | `attempts=1`，`params` 与 `output` 与原任务完全一致 |
| **旋转 90°** | 1280x720 源 → 输出 **720x1280**（ffprobe 实测宽高互换，不是只写元数据） |
| **居中裁剪 9:16** | 1280x720 源 → 输出 **404x720**（宽高比 0.5611，目标 0.5625；等比裁剪不拉伸） |
| **音量归一化** | 低音量正弦 `mean_volume -47.1 dB` → MP3 输出 `-15.5 dB`（提升 31.6 dB），`loudnorm` 生效 |
| **任务日志** | 损坏输入失败后 `logs/job-<id>.log` 存在（1113 B），含任务参数与完整 `ffmpeg stderr` |

> 这组结果补上了此前只有单元测试的两个 bug 证据：**同名不覆盖**与**取消后临时文件清理**，
> 都是在真实核心链路上验证的，不是 mock。

---

## 4.2 首次启动引导（两层证据，分开标注）

### （a）UI 层（headless 浏览器 + mock 后端，`npm run verify:ui`）

- 入口：`scripts/setup-ui-check.mjs`（Playwright Core 驱动本机 Edge headless 加载 `dist/`，
  在页面脚本前注入 `window.__TAURI_INTERNALS__.invoke` mock）
- 报告：`samples/setup-ui-report.json`，**39 PASS / 0 FAIL**（改名后重跑）
- 截图：`samples/setup-ui-missing-before-install.png`（缺失时引导页）、
  `setup-ui-missing-after-install.png`（安装后主界面）、`setup-ui-nonfree.png`（nonfree 拒绝）
- 覆盖：
  - 缺失 FFmpeg 时引导页接管内容区（侧栏、方法网格、任务中心均不渲染），标题与原因文案正确
  - 下载源默认 `https://ffmpeg.org/download.html`，可修改并持久化到设置；「打开下载页」走
    `plugin:opener|open_url`
  - 选文件后展示 LGPL/编码器/SHA-256，安装按钮由禁用转为可用；安装调用带路径且 `allowGpl=false`
  - 安装完成后离开引导页进入主界面，状态栏显示 FFmpeg；全程 `enqueue_jobs` 调用次数为 0（不伪造转换）
  - 已存在 nonfree 构建时标题为「当前 FFmpeg 不符合默认许可路线」，选 nonfree 文件后出现拒绝原因、
    安装按钮禁用，并提供「仅本机开发：允许 GPL」显式开关
- **层级说明**：后端全部是 mock，只能证明引导页的渲染与交互逻辑，**不是应用级 E2E**。

### （b）真实落位链路（真实 LGPL ffmpeg + 真实文件系统，`npm run verify:setup`）

- 真实构建：BtbN `ffmpeg-master-latest-win64-lgpl`
  （`N-128406-g1fe6854fd9-20261104`，`gpl=false`、`nonfree=false`，含 libopenh264/libmp3lame/aac）
- 入口：`src-tauri/src/bin/setup-check.rs` → `setup_check::run_checks`，复用应用命令的
  `setup::install_blocking`（非替身实现）
- 报告：`samples/setup-install-report.json`，**12 PASS / 0 FAIL / 0 SKIP**

| 用例 | 实测结果 |
| --- | --- |
| inspect 真实 LGPL 构建 | `version=N-128406-g1fe6854fd9-20261104`，`gpl=false`、`nonfree=false`、`lgplOk=true`、`h264=true`、`aac=true`，`sha256=1bca0bb0…d4f2` |
| 正常落位 | `<appData>/tools/ffmpeg-N-128406-…/` 内 `ffmpeg.exe` + `ffprobe.exe`；落位后哈希与源一致；`.tmp-*` 残留 0 |
| 重复落位 | 新建 `…-1` 独立目录，既有目录未被覆盖 |
| 错误哈希 | 拒绝（「哈希不匹配…未落位任何文件」），目录数 2→2，临时文件 0 |
| nonfree 构建 | 拒绝（「包含 nonfree 组件…默认 LGPL 路线不接受」），即使 `allowGpl=true`；目录数不变 |
| 不存在 / 无版本 | 分别拒绝「文件不存在」「无法读取版本」 |
| GPL 构建 | 系统 gyan.dev GPL 构建在 `allowGpl=false` 时拒绝 |
| 闭环转码 | 临时移走源 LGPL 的 `bin` 目录后，用**落位副本**生成素材并转 AAC：`status=done`、46532 B、`encoder=aac`、临时残留 0 |

- 已知注入：nonfree 场景用真实 LGPL 二进制，把版本横幅中的 `--enable-libvmaf` **等长替换**为
  `--enable-nonfree`（只改字符串，不改执行逻辑），用于覆盖拒绝分支；未获取真实 nonfree 分发包
  （该类构建本就不应分发）。
- 测试 appData 为 `samples/setup-check/appdata`，不是真实安装目录。
- **未触发**：「落位后复核哈希不一致 → 删目录回滚」分支（需要复制过程损坏，本轮未注入）。

## 4.3 GPL 双变体（同一份源码，编译期注入）

实现方式：

- `src-tauri/Cargo.toml` 增加 `[features] gpl`；`build.rs` 注入 `FF_FLAVOR`；
  `src-tauri/src/flavor.rs` 用 `Flavor` 集中所有策略（ID/产物名/数据目录/通道/默认下载源/许可）。
- `[features] dev-tools`：`src/bin` 下的 4 个检查程序带 `required-features = ["dev-tools"]`，
  默认不构建、不进安装包（此前 `cargo build --release` 会把它们一起打进 MSI/NSIS，已修正）。
- `src-tauri/tauri.gpl.conf.json` 是 GPL 变体的 Tauri overlay（productName、identifier、
  窗口标题、资源清单）；`npm run build:lgpl` / `build:gpl` → `scripts/build-flavor.mjs`。
- 构建脚本在构建前清空共享的 `target/release/bundle`，只收集本次变体的安装包（避免把另一变体
  残留的产物复制进来）。
- 默认 `npm run tauri build` 不带 feature，仍是 LGPL 变体。

验证分层（全部后台、无窗口）：

| 层 | 命令 | 结论 |
| --- | --- | --- |
| 构建层 | `npm run build:lgpl` / `build:gpl` | 两套安装包，产物名含/不含 `GPL`，SHA256 与大小写入 `samples/build-*.json`，互不覆盖 |
| 逻辑层 | `npm run test:rust` | 11/11：`gpl` 接受 GPL、`lgpl` 拒绝；设置项/环境变量都无法翻转 `allowGpl` |
| 逻辑层（文件系统） | `npm run verify:flavor:lgpl` | 7/7：策略矩阵、两个 identifier 的 tools 目录隔离、删除一端不影响另一端 |
| 真实转码（GPL） | `npm run verify:flavor:gpl` | 7/7：落位真实 gyan.dev GPL 构建，`libx264` 转 MP4；`encoder=libx264`、`ffprobe=h264`、临时残留 0 |
| 真实拒绝（LGPL） | `npm run verify:flavor:lgpl` | 7/7：同一任务在 LGPL 变体下 `status=failed`，无输出、无临时残留（`software` 与显式 `libx264` 两条都拒绝） |
| 合规物层 | `npm run check:bundle` | 20/20：解包真实安装包（NSIS 用 7z，MSI 用 WiX `dark.exe` + File 表还原文件名），断言各自许可证全文/notices/源码说明存在、**未**随包塞 ffmpeg 二进制、**未**混入 `*-check.exe` 验证二进制、**应用 exe 内嵌 Windows manifest（comctl32 6.0 依赖）** |

关键证据：

- GPL 变体真实转码：`samples/flavor-check-gpl.json`（`encoderUsed=libx264`，输出 MP4 经 ffprobe 确认为 `h264`）。
- LGPL 变体拒绝：`samples/flavor-check-lgpl.json`（任务层拒绝 GPL 编码器，文案指向「请改用 GPL 版」）。
- 隔离：`samples/flavor-check-*/` 下 `app.formatforge.desktop` 与 `app.formatforge.desktop.gpl`
  两个目录，落位路径互不重合。
- 合规注入口径：nonfree 场景沿用「等长替换横幅字符串」的测试桩，只用于覆盖拒绝分支，
  与真实分发包无关（报告中已标注）。
- 产物哈希（本轮修复 `build.rs` 后重建，见 `samples/build-*.json`）：
  GPL MSI `78bba359…96069`（2 125 824 B）、GPL NSIS `1199f94a…89dd0`（1 456 513 B）、
  LGPL MSI `c08e5d37…8f6ff`（2 134 016 B）、LGPL NSIS `59a4e5f1…79e0c9`（1 459 604 B）。

### 4.4 启动期「无法找到入口」：A/B 复现与修复

现象：安装包安装后启动即报「无法找到入口」（Windows 上即
「无法定位程序输入点 xxx 于动态链接库 yyy.dll」）。

证据链（全部无窗口，脚本 `samples/probe/desktop-run.ps1` + `dumpbin`）：

| 项 | 结论 |
| --- | --- |
| 导入表差异 | GPL 与 LGPL 包内 `formatforge.exe`：**25 个依赖 DLL 完全相同**，`dumpbin /imports` 归一化后 **475 行 0 差异**（无 feature 独有符号） |
| 编译期注入是否生效 | GPL exe 含 `app.formatforge.desktop.gpl` / `gpl-stable` / `https://www.gyan.dev/ffmpeg/builds/`；LGPL exe 含 `app.formatforge.desktop` / `lgpl-stable` / `https://ffmpeg.org/download.html` → feature 与 overlay 都进了 release |
| 安装身份是否冲突 | MSI：ProductCode/UpgradeCode/INSTALLDIR（`FormatForge` vs `FormatForge GPL`）三处均不同 → 不覆盖（该假设排除） |
| comctl32 导出 | `C:\Windows\System32\comctl32.dll` 为 5.82，导出 `SetWindowSubclass`/`RemoveWindowSubclass`/`DefSubclassProc`，**但不导出 `TaskDialogIndirect`**；exe 却按名导入 `comctl32.dll!TaskDialogIndirect` |
| 加载期是否失败 | `build.rs` 漏调 `tauri_build::build()` 时 release exe 无 `.rsrc`/无 manifest（3 545 088 B）；在独立桌面启动 **exitCode=0xC0000139（STATUS_ENTRYPOINT_NOT_FOUND）**；补回后同源构建（3 767 296 B、`.rsrc=True`）不再出现该退出码 |

根因：`build.rs` 曾只负责注入 `FF_FLAVOR`、**丢失了 `tauri_build::build()`**，
导致 release 二进制没有 `.rsrc`（manifest/图标/版本信息全缺）。没有 manifest 就没有
comctl32 6.0 的 activation context，加载器改用 System32 的 5.82，`TaskDialogIndirect`
解析不到 → 启动即入口点错误。

修复：`build.rs` 恢复 `tauri_build::build()`；`check-bundle-compliance.mjs` 增加产物级断言
「应用 exe 内嵌 manifest 且声明 `Microsoft.Windows.Common-Controls`」。

> 边界（如实标注）：本机无法在**可见窗口**下验收（禁止前台 GUI）；独立桌面探针能判定
> 「加载期是否报入口点错误」。当时同一探针看到的 `0xC0000409` 被记为「环境限制」，
> 后来证明那是**真 bug**（见 §4.5，已修复）。

### 4.5 启动即闪退（`0xC0000409`）：定位与修复

现象：装了修复后的新包，GPL 与 LGPL **都**在启动后立刻退出（进程起来又消失）。

事实（无窗口，`samples/probe/desktop-run.ps1` + 事件日志 + stderr 重定向）：

| 项 | 结果 |
| --- | --- |
| 退出码 | **`0xC0000409`**（3221226505，fast-fail，Windows 记为 BEX64） |
| 出错模块 / 偏移 | 两变体、以及用户机上 `C:\Program Files\FormatForge GPL\formatforge.exe`，**同一偏移 `0x22d415`** |
| 启动到退出 | 立即（探针 `WaitForSingleObject` 返回 0ms） |
| 窗口 | 0（错误发生在本窗口创建之前） |
| WER | `C:\ProgramData\Microsoft\Windows\WER\ReportArchive\formatforge.exe.<pid>.dmp` 有 dump |
| **panic 文本**（关键） | `thread 'main' panicked at src\jobs.rs:461:5: there is no reactor running, must be called from the context of a Tokio 1.x runtime` |

根因：`jobs::start_scheduler` 内部直接 `tokio::spawn`，而它被 Tauri 的 `setup` 闭包
（**同步上下文，没有 Tokio 运行时**）调用。`panic = "abort"` 把这条 panic 变成
`0xC0000409` 立即退出。无窗口检查程序都在 `#[tokio::main]` 里调用，所以这条路径
**只在真实应用启动时**炸；上一轮所有验证都是无窗口跑的，因此漏检。
两变体共用这份代码 → 同时中招；与 `gpl` feature、安装目录无关。

修复：

1. `jobs::start_scheduler` 改为 `pub async fn`（只含循环，不再自己 spawn 外层任务）；
2. 应用侧 `tauri::async_runtime::spawn(jobs::start_scheduler(...))`；
   三个检查程序在自己运行时里 `tokio::spawn(jobs::start_scheduler(...))`；
3. **崩溃可观测性**：`run()` 开头装 `std::panic::set_hook`，把 panic 文本 +
   `Backtrace::force_capture()` 追加到 `<appData>/crash.log` 并 `sync_all()`
   （abort 前落盘），同时保留默认 hook 的 stderr 输出。不吞 panic。

复验（修复后重打的包，解包出的真实载荷）：

| 变体 | 存活 | 退出码 | 窗口 |
| --- | --- | --- | --- |
| GPL | 12s 后仍在运行（`wait=258` 超时） | `259`（STILL_ACTIVE） | `Tauri Window`，标题 `FormatForge GPL · 格式工坊（GPL 版）`，子窗口 `Chrome_WidgetWin_1`（WebView2 已渲染） |
| LGPL | 12s 后仍在运行 | `259` | `Tauri Window`，标题 `FormatForge 格式工坊`，子窗口同上 |

同时确认：修复后无新的 Application Error / WER BEX64 事件；两个数据目录都**没有**
`crash.log`（说明没有 panic）。

> 边界：隐藏桌面「存活 12 秒 + 有窗口 + WebView2 子窗口」只能证明**启动期不再崩**，
> 不等于真实桌面能正常进主界面并完成操作；仍需用户在真机确认一次。

**未验证**：

- 代码签名与 macOS 公证（产物未签名）；
- 真实用户机器上「LGPL ↔ GPL」的升级/共存路径（两个 identifier 会共存，未实测卸载与迁移）；
- GPL 源码要约与「不随包分发二进制」措辞的**法律审查**（需要法律确认）；
- Linux/macOS 的安装包与合规物。


---

## 4.6 功能补齐第一轮（P0，本轮）

### 盘点结论（完整缺口表见本轮交付说明）

- **P0（本轮做）**：视频压缩入口「标未实现但底层已具备」的误导、视频旋转、比例裁剪、
  音频响度归一化、每任务诊断日志。
- **P1（下一轮）**：视频合并/混流/字幕/GIF、图片 EXIF、暂停/继续、命名模板、冲突策略选择、
  完成后自动打开目录/关机、面板参数持久化与导入导出。
- **P2**：PDF 合并/拆分/转图片（范围大）、图片序列、配置导入导出、自动重试。

### 本轮实现

| 项 | 实现 | 关键文件 |
| --- | --- | --- |
| 视频压缩入口接通 | `video-compress` 不再只是占位，进入同一视频面板并默认「目标码率 4 Mbps」 | `src/catalog.ts`、`src/panels/VideoPanel.tsx` |
| 视频旋转 | 参数 `rotation: 0/90/180/270` → `transpose=1` / `transpose=2` / `hflip,vflip`；顺序为裁剪 → 旋转 → 缩放 | `src-tauri/src/args.rs` |
| 视频比例裁剪 | `crop: source/16:9/1:1/9:16` → `crop=w=…:h=…`（居中取最大区域，宽高取偶），不拉伸 | `src-tauri/src/args.rs` |
| 音频响度归一化 | `normalize=true` → `-af loudnorm=I=-16:TP=-1.5:LRA=11`；失败会明确报错，不静默成功 | `src-tauri/src/args.rs`、`src/panels/AudioPanel.tsx` |
| 每任务诊断日志 | 任务结束（done/failed/cancelled）写 `<appData>/logs/job-<id>.log`（参数 + 最多 400 行 stderr）；任务卡片「日志」按钮打开 | `src-tauri/src/jobs.rs`、`src-tauri/src/lib.rs`、`src/components/TaskCenter.tsx` |
| 画面裁剪入口接通 | `video-crop` 进入同一面板并默认 16:9 | `src/catalog.ts`、`src/panels/VideoPanel.tsx` |
| 音量标准化入口接通 | `audio-volume` 进入音频面板并默认开启归一化 | `src/catalog.ts`、`src/panels/AudioPanel.tsx` |

> 「视频压缩」与「画面裁剪」不是新造的第二套转码路径，而是把视频面板已有的码控/滤镜能力
> 暴露给对应入口；避免「入口点进去只提示未实现」的误导。

### 验证

| 验证 | 结果 |
| --- | --- |
| `npm run test:rust` | **16/16**（新增 5 条：滤镜顺序、180° 翻转对、无调整不插 `-vf`、16:9/1:1 表达式、`loudnorm` 参数） |
| `npm run verify:core` | **11/11**（新增旋转 720x1280、裁剪 404x720、响度 −47.1→−15.5 dB、失败任务日志 1113 B；均为真实 ffmpeg/ffprobe） |
| `npm run verify:flavor:lgpl` / `:gpl` | 各 **7/7** |
| `npm run verify:setup` | **12/12** |
| `npm run verify:ui` | **39/39**（引导页 UI 层，后端 mock） |
| `npm run check:bundle` | **20/20**（两变体 MSI+NSIS 解包合规） |
| 隐藏桌面 CDP UI 度量（7 组视口/缩放：1280×800、1440×900、1400×880、最小 1080×680，DSF 100/125/150，GPL release exe） | 横向溢出 **0**、被父容器裁切 **0**、不可滚出 **0**；纵向滚动仅 Inspector 1 处（设计允许）。视口外元素 84–313 条（主界面 84–120、设置弹层 313，均可滚动到达），**不可滚出=0**（含新增「画面调整」组） |
| 隐藏桌面 50 ms 窗口采样（GPL release exe，10 s） | **console 类窗口 = 0**；可见窗口仅 `Tauri Window` 与系统输入指示器（隐藏桌面上，不进入用户前台） |

> UI 度量使用 GPL release exe：LGPL 变体在本机只找到系统 GPL ffmpeg，会停在首次引导页，
> 无法量到主界面。两变体前端是**同一份构建产物**，因此 UI 结论对 LGPL 同样成立；
> 该步骤未在真机可见窗口内复验。

### 本轮产物（两变体，同一套功能代码，仅 FFmpeg 许可策略不同）

| 变体 | 产物 | 大小 (B) | SHA-256 |
| --- | --- | --- | --- |
| GPL | `FormatForge GPL_0.1.0_x64_en-US.msi` | 2 134 016 | `8CC2115EBBD1903B4DF838E9990D92869075A049BA711CF18E0BCCEFDC5F2622` |
| GPL | `FormatForge GPL_0.1.0_x64-setup.exe` | 1 463 407 | `AD6D8A2537A65FBABA7562D5DB187764F92D459A8C86EC291B1AEC39907FD3E1` |
| LGPL | `FormatForge_0.1.0_x64_en-US.msi` | 2 138 112 | `E3F7813C742199F2BFDE5AA71AA87CE1F24286D6C2B228452CA07B92EB1BC3A0` |
| LGPL | `FormatForge_0.1.0_x64-setup.exe` | 1 466 650 | `D5397C4D4AA0943379CD296560F6144BDE6561C71C4FC8275698203D1F40172D` |

---

## 4.7 功能补齐第二轮（P1，本轮）

本轮把上一轮列出的 P1 全部做完；两变体仍是**同一套源码**，仅 FFmpeg 许可策略不同。
产品名**已定为 Transmutio**，改名排下一轮执行（方案见 `RENAME-PLAN.md`），**本轮安装包仍叫 FormatForge**。

### 本轮实现

| 项 | 实现 | 关键文件 |
| --- | --- | --- |
| 视频合并（多输入） | 三种模式：`auto`（同参直接 concat copy，不同参自动统一转码）、`copy`（要求编码/分辨率/帧率/音轨完全一致，不一致明确拒绝）、`transcode`（逐段统一到目标分辨率/fps 后 concat copy）；中间段与清单落到输出目录，结束清理 | `src-tauri/src/args.rs`、`src-tauri/src/jobs.rs`、`src/panels/CompositePanels.tsx` |
| 音视频混流 / 替换音轨 | `replace` 用 `-map 0:v:0 -map 1:a:0`，`add` 追加为第二条音轨；视频 `-c:v copy`；音频 AAC 或直接 copy；copy 模式下 PCM 等不被 MP4 接受的编码**明确拒绝**；输出时长以视频为准 | 同上 |
| GIF | fps / 宽度 / 开始时间 / 时长可配；默认「调色板两遍」（`palettegen` + `paletteuse`），可关闭为直接转换；两者都产出合法 GIF | `src-tauri/src/args.rs`、`src/panels/CompositePanels.tsx` |
| 图片 EXIF 方向 | 自带 JPEG/TIFF Orientation 解析（只读文件头 256 KiB）；**无条件 `-noautorotate`** 关闭 ffmpeg 隐式旋转，由显式 `transpose/hflip/vflip` 唯一负责，再叠加手动旋转，**不会转两次** | `src-tauri/src/args.rs`、`src/panels/ImagePanel.tsx` |
| 暂停 / 继续 | **队列级暂停**：暂停后不再调度新任务，正在运行的任务继续跑完；恢复后继续调度。理由：Windows 无 SIGSTOP，`NtSuspendProcess` 属未文档化 API，挂起 ffmpeg 子进程有句柄/管道死锁风险，收益不值得 | `src-tauri/src/jobs.rs`、`src-tauri/src/lib.rs`、`src/components/TaskCenter.tsx` |
| 命名模板 + 冲突策略 | `{name}` / `{date}` / `{index}` / `{ext}` 占位符；非法字符清理；冲突策略 `suffix`（默认，生成「文件名 (1).ext」）/ `skip`（不入队、不产生文件）/ `overwrite`（仅显式选择；旧文件先备份再落位，成功删备份、失败回滚，失败即报错不静默改名） | `src-tauri/src/jobs.rs`、`src/panels/shared.tsx` |
| 完成后动作 | `open-folder` 走 Tauri opener（测试注入回调记录，不开窗）；`shutdown` **只构造命令**（Windows `shutdown /s /t 60`），且必须同时满足设置项与 `FORMATFORGE_ALLOW_SHUTDOWN=1` 才执行，测试绝不真关机 | `src-tauri/src/after.rs`、`src-tauri/src/lib.rs` |
| 面板参数持久化 | 视频/音频/图片面板参数与命名策略写入 `localStorage`（`formatforge:panel:*`）；缺省合并、损坏回落、隐私模式静默降级 | `src/lib/panelPrefs.ts`、`src/panels/*.tsx` |

> 进度条：多步任务（合并/转码链）把每一步进度映射到总进度区间，避免回跳。

### 验证（真实转码 / 单测 / headless UI 分开标注）

| 验证 | 结果 | 说明 |
| --- | --- | --- |
| `cargo test` | **31/31** | 新增 EXIF 解析与滤镜映射、`-noautorotate` 去重、concat 清单反斜杠归一化、GIF 两遍选择、混流 map、命名模板、冲突策略、overwrite 备份、日期格式、关机命令构造等 |
| `npm run verify:core` | **31/31**（真实 ffmpeg/ffprobe） | 见下方逐项产物断言 |
| `npm run verify:flavor:lgpl` / `:gpl` | 各 **7/7** | 编译期变体与许可拦截未回归 |
| `npm run verify:setup` | **12/12** | 首次引导 + 真实落位 + 落位后转码 |
| `npm run check:bundle` | **20/20** | 两变体 MSI+NSIS 解包合规（新包） |
| `node scripts/panel-persist-check.mjs` | **9/9** | UI 层（headless + mock 后端），非应用级 E2E |
| `npm run build` | 通过 | `tsc --noEmit` + `vite build`，退出码 0 |

真实产物断言（`samples/core-e2e-report.json`）：

| 用例 | 关键字段 |
| --- | --- |
| 合并：同参 concat copy | `duration=8.000s`、`video=1 audio=0`、`1280x720`、`encoder=copy` |
| 合并：不同参统一转码 | `duration=8.000s`、`1280x720`、`encoder=libx264` |
| 合并：copy 拒绝不同参 | `status=failed`、`error_kind=invalid-params`、文案含「分辨率不同（1280x720 vs 640x360）」 |
| 混流：替换音轨（AAC） | `duration=8.000s`、`video=1 audio=1` |
| 混流：新增第二条音轨 | `duration=8.000s`、`video=1 audio=2` |
| 混流：copy 拒绝 PCM | `status=failed`、文案含「pcm_s16le 不能直接复制进 MP4」 |
| GIF：调色板优化 | `frames=20`、`width=320`、`size=164994` |
| GIF：直接转换 | `frames=20`、`width=320`、`size=92843` |
| EXIF：自动校正 | `720x1280`（只转一次 90°；合成样本，非相机原图） |
| EXIF：关闭自动校正 | `1280x720`（保留原始像素方向） |
| EXIF：自动 + 手动 90° | `1280x720`（合计 180°，证明没有第三次隐式旋转） |
| 命名模板 `{name}-{index}.{ext}` | `模板-1.mp4` / `模板-2.mp4` |
| 冲突 suffix | 生成 `同名 (1).mp4`，原文件内容不变 |
| 冲突 skip | `status=skipped`，目标存在且未生成新文件 |
| 冲突 overwrite | `替换.mp4` 被显式替换（1957229 B），无 `.ffbak` 残留 |
| 暂停 / 继续 | concurrency=1 时长任务 running、短任务 queued；取消长任务后短任务仍 queued；恢复后短任务 done |
| 完成后 open-folder | 注入回调收到 `action=open-folder`、`dir=samples/headless/out-after` |
| 关机（仅构造） | `["/s", "/t", "60"]`；`shutdown_enabled()=false`，未执行 |

面板持久化（`samples/ui-overflow/panel-persist-report.json`）：

- 视频（分辨率 720 / 帧率 30 / 旋转 90° / 模板 / 冲突 skip）、音频（320 kbps / 48 kHz / 立体声）、图片（WebP / 1920 / 关自动校正 / 180°）均先写入 `localStorage`，页面重载后逐项恢复；
- 另用「应用脚本执行前注入历史 localStorage」的冷启动场景验证视频面板恢复（HEVC / 1080 / 60 / 270° / `backup_{date}` / overwrite）；
- 页面无未捕获异常。

### 本轮产物（两变体，同一套功能代码）

| 变体 | 产物 | 大小 (B) | SHA-256 |
| --- | --- | --- | --- |
| GPL | `FormatForge GPL_0.1.0_x64_en-US.msi` | 2 179 072 | `BC3C82652DEC41B7CB8C7C3CE9FF4FA73762BFA7529911CE9257BD07D0482B91` |
| GPL | `FormatForge GPL_0.1.0_x64-setup.exe` | 1 496 349 | `391A05FCA49906CC0C3053575091DA01F4D7A5B2F8EBB1931957C62483EE1427` |
| LGPL | `FormatForge_0.1.0_x64_en-US.msi` | 2 183 168 | `66415AEA9112D3AAB5101ACC8E6067787E3F07DE058BA74E559D9C988D355AA5` |
| LGPL | `FormatForge_0.1.0_x64-setup.exe` | 1 499 005 | `A9CE89A2CD59D80E0D43A93DCC0C04866411771E74BF98EF044699F9A1B32C2F` |

> 打包串行执行（WiX 并发会报 os error 32）；`check:bundle` 在**新包**上重跑。
> 打包后只改了 dev-tools 的 `headless-check` 断言代码，未改任何进安装包的业务代码。

### 本轮未验证

- **真实窗口内**的暂停/继续按钮、完成后动作与面板恢复（本轮全部为 headless/注入回调）；设置页文案未在真机复验；
- 关机动作从未真实执行（按纪律只做命令构造）；
- 硬件编码结论仍**只有 AMF 一项**实测，本轮不扩大；
- macOS / Linux 未构建运行；签名流程未执行。

---

## 4.8 改名轮：FormatForge → Transmutio（已执行）

方案与执行记录见 [RENAME-PLAN.md](RENAME-PLAN.md)。本节只记**实际结果**。

### 身份切换（静态核验 + 产物级实测）

| 项目 | LGPL | GPL |
| --- | --- | --- |
| 产品名（ARP / 窗口 / 快捷方式 / 安装目录） | `Transmutio` | `Transmutio GPL` |
| identifier / 数据目录 | `app.transmutio.desktop` | `app.transmutio.desktop.gpl` |
| 更新通道 | `transmutio-lgpl-stable` | `transmutio-gpl-stable` |
| MSI UpgradeCode（新） | `{5C1F7A2E-8D34-5B96-A1E7-C4D2F908B365}` | `{B7E4D190-3C62-5A8F-9D2B-E6F1047C8A53}` |
| 旧 identifier（仅迁移/拦截用） | `app.formatforge.desktop` | `app.formatforge.desktop.gpl` |

证据：`check:bundle` 从**成品安装包**读回 ProductName / UpgradeCode（MSI Property 表）、
identifier（dark 反编译 `ShortcutProperty System.AppUserModel.ID`）、NSIS 版本资源 ProductName、
归档 `installer.generated.nsi` 的 `PRODUCTNAME` / `BUNDLEID`。

### 四个安装包（本轮重建，实测）

| 变体 | 产物 | 字节 | SHA-256 |
| --- | --- | --- | --- |
| LGPL | `Transmutio_0.1.0_x64_en-US.msi` | 2 207 744 | `6212688a1019046ba9bd68c4a782b827db9147ce9e2f7c68c55cdc2762dccf5e` |
| LGPL | `Transmutio_0.1.0_x64-setup.exe` | 1 518 737 | `23ee0fc838b70bf013f793ffb884e4626135df4e5dd508af9df9440566bc7251` |
| GPL | `Transmutio GPL_0.1.0_x64_en-US.msi` | 2 203 648 | `4b570db7bd3869563320d3150285dee165db37e162aa643a51f6ae50c2c2db64` |
| GPL | `Transmutio GPL_0.1.0_x64-setup.exe` | 1 516 330 | `9998e07cb1b49d88bfa67c3ce35252ed543294360fe3ddf6074d428b683d1853` |

安装后主程序文件名 = 产品名：`Transmutio.exe` / `Transmutio GPL.exe`（`mainBinaryName`，构建日志
`Built application at: …\Transmutio.exe`）。Cargo 包名仍是内部的 `formatforge`，`cargo build`
的调试产物因此仍叫 `formatforge.exe`（白名单，见下）。

报告：`samples/build-lgpl.json` / `samples/build-gpl.json`（含另一变体哈希未变的复核）。
`check:bundle` 由 20 条断言扩到 **36/36 PASS**：

- 新增 8 条（产品身份、identifier、NSIS 版本资源、生成脚本身份，×2 变体 ×2 种安装包）；
- 新增 4 条旧版并存拦截断言（MSI `Upgrade`+`LaunchCondition`、NSIS `NSIS_HOOK_PREINSTALL`，×2 变体 ×2 种安装包）；
- 新增 4 条主程序文件名断言（包内必须是 `<产品名>.exe` 且无 `formatforge.exe` 残留，×4 个安装包）。

### 残留白名单（`formatforge` 仍出现在代码里的位置，逐条理由）

| 位置 | 保留理由 |
| --- | --- |
| localStorage 前缀 `formatforge:`（`src/lib/settings.ts`） | 旧版设置键名；改名后继续读同一前缀，等价于迁移面板参数，清空会丢失用户设置 |
| `FORMATFORGE_ALLOW_SHUTDOWN` / `FORMATFORGE_ALLOW_GUI_E2E` | 已有自动化脚本与既有文档在用；改变量名会静默失效（安全闸门失效比名字一致更危险） |
| npm scope `@formatforge/*`（`packages/*`） | 工作区内部包名，不进安装包、不出现在用户可见处；改 scope 需全仓引用同步，无收益 |
| Cargo package / crate / lib 名 `formatforge`、`formatforge_lib` | 编译期标识：改包名会牵动 `main.rs`、4 个 dev-tools bin、`build.rs`、验证脚本与增量缓存；用户不可见。**安装包内的可执行文件名已改**为 `Transmutio.exe` / `Transmutio GPL.exe`（`mainBinaryName`），`formatforge.exe` 现在只出现在 `cargo build` 的调试产物里 |
| `FF_FLAVOR` 环境变量、`ff-` 临时文件前缀 | 构建期注入 / 文件系统临时名，用户不可见 |
| 旧 identifier `app.formatforge.desktop*` | 迁移与旧版拦截的**唯一锚点**，删掉就无法找到旧数据/旧安装 |
| 旧 UpgradeCode `{68CDE348…}` / `{D0BEF367…}` | MSI `Upgrade(OnlyDetect)` 的检测目标 |
| 历史文档与验证报告中的 FormatForge | 记录既有事实，不构成用户可见文案 |

用户可见处（窗口标题、页面标题、设置页、引导页、任务日志头、安装包 ProductName、
快捷方式、ARP、安装目录、更新通道）已全部为 Transmutio：由前端 build + headless UI 脚本
（`setup-ui-check.mjs` / `panel-persist-check.mjs`）与 `check:bundle` 双向核对。

## 4.9 数据迁移与旧版并存拦截

### 迁移实现（`src-tauri/src/migration.rs`，启动期 `main.rs` 先于 WebView2 初始化调用）

流程：新目录不存在且旧目录存在 → 同卷临时目录 `.transmutio-migrate-<stamp>` 复制
（保留空目录、拒绝 symlink）→ 比对文件数/目录数/字节数/目录结构/逐文件 SHA-256 →
原子 `rename` 落位 → 写 `migration.json` + `migration.log` + `migrated-from.txt`。
失败路径（复制中途失败 / 校验不一致）只删临时目录，旧目录不动、新目录不留半套。
两目录并存时**不合并**；设置页「从旧版导入数据」= 整份备份当前 appdata →
合并旧 appdata → 旧 WebView2 复制到 `EBWebView.legacy-import` 暂存 → 下次启动原子换入
（当前 `EBWebView` 备份为 `EBWebView.import-bak-<stamp>`）。

单测（`cargo test` 37/37 中的 6 条，全部通过）：

| 单测 | 断言要点 |
| --- | --- |
| `migrate_tree_copies_and_verifies_while_keeping_source` | 真实写入 `settings.json`（主题/并发）+ tools 文件 + 空目录 → 迁移 → 新目录读回、旧目录字节数不变。本产品无撤销历史（转换任务不提供 undo）；localStorage 设置/面板参数的等价覆盖见最后一条 |
| `second_run_is_idempotent_and_keeps_source` | 再跑一次不覆盖、不重复迁移 |
| `copy_failure_leaves_old_untouched_and_no_new_dir` | 注入复制失败 → 旧目录完整、新目录不存在 |
| `verification_failure_cleans_temp_and_leaves_no_new_dir` | 注入字节/哈希不一致 → 清临时目录、不落新目录 |
| `variants_use_scoped_legacy_paths` | LGPL/GPL 各自映射到对应旧 identifier 目录 |
| `manual_import_backs_up_and_defers_webview_swap` | 手动导入先备份、WebView2 只暂存不就地改 |

### 旧版并存拦截（NSIS 实测 / MSI 静态核验）

- **NSIS（实测）**：`installer-hooks.nsh` 的 `NSIS_HOOK_PREINSTALL` 检查
  ARP `DisplayName` 前缀 `FormatForge`（HKLM/HKCU × 64/32 视图）与旧 UpgradeCode 打包键
  （`Installer\UpgradeCodes\<packed>` 下的**值**，用 `EnumRegValue`）。命中即 `MessageBox` + `Quit`。
  证据：`npm run verify:legacy-guard` 用**同一份 hooks 源文件**编译成无 UI 探针，
  在本机真实注册表上执行，输出 `hit=1`（LGPL 侧产品码 `BD70F43A…`、GPL 侧 `9E1E8DE9…`）；
  同时静态核对归档 `installer.generated.nsi` 确有 `!insertmacro NSIS_HOOK_PREINSTALL`。
- **MSI（数据库级静态核验）**：`wix/legacy-block.wxs` 用 `Upgrade(OnlyDetect="yes")` 检测两个旧
  UpgradeCode，`LaunchCondition` 为 `NOT OLD_FORMATFORGE_LGPL AND NOT OLD_FORMATFORGE_GPL`。
  证据：从成品 MSI 的 `Upgrade` / `LaunchCondition` 表读回上述两行与条件文本（`verify:legacy-guard` 与
  `check:bundle` 双重复核）。**未做运行时安装探针**：per-machine MSI 会触发 UAC 提权并写宿主状态，
  违反「不装到宿主」的纪律（实测 `/a` 管理员安装会挂起在提权对话框，已终止且未产生任何文件）。

### 本轮踩坑（已写进 RENAME-PLAN.md，避免复发）

1. WiX 3.14 `light` 静默丢弃未被引用的 Fragment → 必须 `componentGroupRefs` 引用一个空 ComponentGroup；
2. 旧 UpgradeCode 在注册表下是**值**不是子键（`EnumRegKey` 恒为空）；
3. Tauri v2 NSIS 钩子宏名是 `NSIS_HOOK_PREINSTALL`（`customInit` 不会被调用）；
4. MSI 代码页 1252 → `LaunchCondition` 描述只能 ASCII（NSIS 侧仍为中文提示）。

## 4.10 改名后回归（全部后台，实数）

| 命令 | 结果 | 报告 / 证据 |
| --- | --- | --- |
| `cargo test` | **37 passed / 0 failed**（另有 2 个 0 测试的 test target） | `samples/cargo-test.log` |
| `npm run verify:core` | **31 PASS / 0 FAIL** | `samples/core-e2e-report.json` |
| `npm run verify:flavor:lgpl` | **7 / 0 / 0（7）** | `samples/flavor-check-lgpl.json` |
| `npm run verify:flavor:gpl` | **7 / 0 / 0（7）** | `samples/flavor-check-gpl.json` |
| `npm run verify:setup` | **12 PASS / 0 FAIL / 0 SKIP** | `samples/setup-install-report.json` |
| `npm run check:bundle` | **36 / 36 PASS** | `samples/compliance-check.json` |
| `npm run verify:legacy-guard` | **6 / 6 PASS**（MSI 静态 ×2、NSIS 静态 ×2、NSIS 注册表探针 ×2） | `samples/legacy-guard-report.json` |
| `npm run build`（前端 `tsc --noEmit && vite build`） | 通过，64 modules / 约 1s | 构建日志 |
| `node scripts/setup-ui-check.mjs` | **39 PASS / 0 FAIL** | `samples/setup-ui-report.json` |
| `node scripts/panel-persist-check.mjs` | **9 PASS / 0 FAIL** | `samples/ui-overflow/panel-persist-report.json` |

说明：`verify:setup` 在本轮重跑时换用了 BtbN 2026-11-04 的 LGPL 构建（`N-128406-g1fe6854fd9-20261104`），
断言数由旧记录的 10 条变为 12 条（新增 GPL 变体侧的「接受 GPL / 拒绝 LGPL」两条），全部通过。

---

## 4.11 功能补齐第三轮（P2，本轮）

产品名保持 **Transmutio / Transmutio GPL**，本轮只做功能，不改身份。

### 逐项判定

| 子项 | 判定 | 实现 / 真实断言摘要 |
| --- | --- | --- |
| PDF 合并 | **已完成** | 内置 `lopdf` 0.47 进程内合并；2 页 + 3 页 → 输出 **5 页 / 629 B** |
| PDF 拆分 | **已完成** | 每页一个文件（2 页 → 2 个各 1 页）与指定页码范围（`1-2` → 1 个 2 页）均实测；0 / 倒序 / 越界 / 重复明确拒绝 |
| PDF 转图片 | **已完成（需外部 pdftoppm）** | Poppler 正路径：200 pt 页面 @96 DPI → **2 张 PNG，首张 267×267，合计 1234 B**；工具缺失时卡片置灰 + 后端拒绝 |
| 图片序列 → 视频 | **已完成** | 10 张 320×240 @10 fps → **320×240 / 10 帧 / 1.00 s / 1254 B**；缺号、前缀不一致、编号位宽不一致入队即拒绝 |
| 视频 → 图片序列 | **已完成** | 2 s / 30 fps 源，fps=10、宽 160 → **20 帧 160×120**；先写临时目录，全成功才移入输出目录 |
| 图片裁剪 | **已完成** | 640×480 按 1:1 → **480×480**；自定义 300×200 @(50,25) + 最长边 150 → **150×100**；越界明确拒绝（`640x480 @ (10,0) 超出源图 640x480`） |
| 图片拼图 | **已完成** | 4 张 200×150、2×2、间距 10 → **410×310**；超出网格容量入队拒绝 |
| 配置导入 / 导出 | **已完成** | 导出 **479 B** 并可回读；版本 2、错误 kind、未知字段、未知占位符、路径模板、字段类型错误逐条明确拒绝；导出文本不含 `outputDir` / 本机路径 |
| 自动重试 | **已完成** | 仅 `hw-unavailable` 与新增 `io-temporary` 两类可重试；参数错误（invalid-params 等）不重试；每任务最多 1 次，重试排在 `after_action` 之前 |

### 依赖策略（讲清哪些是外部工具）

| 能力 | 依赖 | 缺失时的用户可见行为 |
| --- | --- | --- |
| PDF 合并 / 拆分 | **无外部依赖**：`lopdf` 纯 Rust，静态链接进主程序 | — |
| PDF 转图片 | **外部 Poppler `pdftoppm`** | ① 启动时调用 `pdf_tools_status`；② 方法列表卡片置灰、badge「需外部工具」，点击 toast 完整安装建议；③ 即使绕过 UI，后端 `precheck_spec` 再次拒绝入队；④ 转换失败归类 `external-tool` |
| 不采用 | qpdf / Ghostscript | 合并/拆分无需外部进程，避免新增安装步骤与许可面 |

`pdftoppm` 查找顺序：`PDFTOPPM_PATH` → 设置目录（兼容 `Library/bin`、`bin`）→ `TRANSMUTIO_PDF_TOOLS` → `PATH`。
仓库与安装包都不分发 Poppler。

### 实现要点

| 模块 | 内容 |
| --- | --- |
| `src-tauri/src/pdf.rs`（新） | 合并（renumber + object 合并 + 重建 Pages 根 + 继承 Resources/MediaBox/CropBox/Rotate）、页码范围解析、每页/按范围拆分、`pdftoppm` 栅格化（Windows `CREATE_NO_WINDOW`） |
| `src-tauri/src/pdf_tools.rs`（新） | 外部渲染器解析与缺失原因（含安装建议），纯函数 `resolve_with` 便于测试 |
| `src-tauri/src/preset.rs`（新） | 版本化预设（`transmutio-preset` / version 1），根、面板、output 三层字段白名单 + 按类型反序列化 + 路径/占位符校验 |
| `src-tauri/src/args.rs` | `ImageParams` 裁剪字段与 `crop→rotate→scale` 顺序；`SeqParams` / `FramesParams` / `CollageParams` 及命令构造、序列连续性校验、拼图画布计算 |
| `src-tauri/src/jobs.rs` | 新增 6 个任务类型、入队预检、PDF / 抽帧执行器、临时目录后移入、`io-temporary` 分类、`pick_sequence_encoder`（LGPL 包自动选 libopenh264）、自动重试 |
| `src/panels/AdvancedPanels.tsx`（新） | 序列、抽帧、拼图、PDF 四个面板；PDF 面板缺 `pdftoppm` 时禁用开始并显示原因 |
| `src/panels/ImagePanel.tsx` | 新增裁剪组（none / ratio / custom），与旋转、缩放叠加 |
| `src/lib/presetPrefs.ts`（新） / `SettingsSheet.tsx` | 预设导出（白名单收集，不含路径）与导入（后端校验通过后回写 localStorage） |

### 验证（真实产物 / 单测 / headless 分开标注）

| 验证 | 结果 | 说明 |
| --- | --- | --- |
| `cargo test` | **55 / 55**（原 37 + 新增 18） | 裁剪顺序、裁剪校验、序列缺号/前缀/位宽、尺寸策略、拼图容量与颜色、PDF 合并页数与范围解析、预设拒绝路径、重试类别 |
| `p2-check`（LGPL FFmpeg + Poppler） | **7 / 7 PASS** | 报告 `samples/p2-check/run-poppler/p2-report.txt`；断言见上表与下方明细 |
| `npm run verify:core` | **31 / 31** | P0/P1 链路未回归 |
| `npm run verify:flavor:lgpl` / `:gpl` | 各 **7 / 7** | 编译期变体未回归 |
| `npm run verify:setup` | **12 / 12** | 首次引导与落位未回归 |
| `npm run check:bundle` | **40 / 40** | 新包解包合规（两变体 MSI + NSIS；新增 4 条「未随包分发 Poppler」断言） |
| `npm run verify:ui` | **39 / 39** | 引导页 UI（headless） |
| `node scripts/panel-persist-check.mjs` | **9 / 9** | 面板参数持久化 |
| `npm run build` | 通过 | `tsc --noEmit` + `vite build` |

> 以上计数是 P2 当轮的快照。格式清单修复后：`cargo test` **63/63**、`p2-check` **10/10**，
> 且新增 `verify:formats` **7/7**，详见 §4.12。

`p2-check` 明细（真实文件读取，不是命令构造）：

| 用例 | 断言数据 | 拒绝路径 |
| --- | --- | --- |
| 序列 → 视频 | 320×240、10 帧、1.00 s、1254 B | — |
| 视频 → 序列 | 20 帧、首帧 160×120 | — |
| 图片裁剪 | 480×480；150×100 | 越界矩形拒绝 |
| 图片拼图 | 410×310 | 容量不足拒绝 |
| PDF 合并 / 拆分 | 5 页 / 629 B；每页 2 个各 1 页；`1-2` → 1 个 2 页 | `1-9` 越界拒绝 |
| PDF 转图片 | 2 张 PNG、267×267、合计 1234 B | 隐藏 `TRANSMUTIO_PDF_TOOLS` 且 PATH 无 pdftoppm → `available=false`、原因含 pdftoppm、`rasterize` 拒绝 |
| 预设导入 / 导出 | 导出 479 B 且回读一致 | 版本 9 / 未知字段 / 路径模板明确拒绝 |

### 本轮产物（`dist-artifacts/`）

| 变体 | 安装包 | 字节 | SHA-256 |
| --- | --- | --- | --- |
| LGPL | `lgpl/Transmutio_0.1.0_x64_en-US.msi` | 2,662,400 | `acac3a56f7f7a9dfb15119d64b6a67dc09fb6ae2538ee7a7f5d3b5ea114362e2` |
| LGPL | `lgpl/Transmutio_0.1.0_x64-setup.exe` | 1,815,688 | `1d71c965bc28243e14da851bc43f4c1b77790fdb232731ad08a3b27f2eb1af65` |
| GPL | `gpl/Transmutio GPL_0.1.0_x64_en-US.msi` | 2,654,208 | `3c414fca80f1ba77d938fdb564620ab4423fa2ede38e043262da196719163505` |
| GPL | `gpl/Transmutio GPL_0.1.0_x64-setup.exe` | 1,813,738 | `b61ca224f3ce73b2f316dcb25914182fc71c192ad0e2bc0bac4526e8e0b5bbd2` |

### 本轮未验证

- 设置页「选择 PDF 工具目录」的真实 GUI 点击链路（本轮只验证了环境变量与便携目录解析）；
- 自动重试未在真实硬件编码故障（驱动崩溃 / GPU 被独占）下触发，只有类别映射单测；
- 加密 PDF、扫描件（纯图片页）与超大页面的栅格化未实测；
- 序列 → 视频在 GPL 包下走 libx264 的真实转码（本轮用 LGPL 包 + libopenh264 实测）。

---

## 4.12 格式清单修复（视频 6 容器 / 音频 6 格式）

用户反馈：**「视频目标格式」只有 mp4，「音频目标格式」只有 mp3**，与「P2 全项完成」不符。

### 根因：硬编码（不是探测退化，也不是竞态）

| 修复前的位置 | 证据 | 影响 |
| --- | --- | --- |
| `src/panels/VideoPanel.tsx` 目标格式组 | 只有一行静态说明「容器：MP4」，没有容器选择控件；`VideoParams` 无 `container` 字段 | 界面上不可能出现第二个容器 |
| `src/panels/AudioPanel.tsx` 格式 `Segmented` | 只有 `mp3` / `aac` 两个硬编码选项 | 界面上不可能出现第三个音频格式 |
| `src-tauri/src/args.rs::build_video` | 输出固定 `-f mp4`，不读任何容器参数 | 即使前端传 mkv 也会被写成 mp4 |
| `src-tauri/src/args.rs::audio_encoder` | `aac` → `aac`，**其余一律 `libmp3lame`** | flac/wav/ogg/opus 无法表达 |
| `src-tauri/src/args.rs::build_audio` | 只区分 mp3/aac 的码率参数 | 无损格式会被错误追加 `-b:a` |
| `src-tauri/src/jobs.rs::kind_ext` | 只认 `audio-mp3` / `audio-aac`，视频恒为 `.mp4` | 输出扩展名无法跟随容器/格式 |
| `src-tauri/src/probe.rs` 候选表 | 只有 h264/hevc/aac/libmp3lame 等少量候选 | 即使前端列全也无法判断 VP9/FLAC 是否可用 |

排除另外两类猜测：

- **不是探测退化**：探测链路正常。本机 `ffmpeg -encoders` 与试编返回 h264/hevc/aac/libmp3lame 等可用编码器，
  `capabilities.json` 有 fingerprint + 7 天 TTL，命中/未命中都会得到非空 `videoEncoders`/`audioEncoders`；
  修复前的面板在 `caps` 到达前后**都只渲染一项**，可用性变化不会增减选项。
- **不是竞态**：格式清单当时是常量，`caps` 只影响「可用/不可用」的灰显，不参与列表构造；
  首屏与探测完成后渲染的条目数完全相同。因此加「等探测完成再渲染」并不能解决该问题。

**断言缺口（交付对账为什么会漏）**：修复前的 31/31、55/55、40/40 分别覆盖
「核心链路」「单测」「安装包合规」，没有任何一条断言「视频容器数量」「音频格式数量」
「容器与编码器不兼容组合」。P2「全项完成」本身是准确的——它只承诺 PDF / 序列 / 裁剪 / 拼图 / 预设 / 重试，
**格式矩阵从来不在 P0/P1/P2 清单里**，于是被读成了「格式也已齐全」。本轮补上这三类断言。

### 修复内容（单一事实来源 + 探测只决定可用性）

| 模块 | 修复 |
| --- | --- |
| `src-tauri/src/containers.rs`（新） | 后端唯一清单：`VIDEO_CONTAINERS`（[L32](src-tauri/src/containers.rs#L32)）6 容器 + `AUDIO_FORMATS`（[L89](src-tauri/src/containers.rs#L89)）6 格式；`validate_video`（[L159](src-tauri/src/containers.rs#L159)）拒绝不兼容组合、`pick_container_audio_encoder`（[L198](src-tauri/src/containers.rs#L198)）按容器选音轨编码 |
| `src/lib/formats.ts`（新） | 前端同构清单（[L19](src/lib/formats.ts#L19) / [L52](src/lib/formats.ts#L52)）；`PROBE_MISSING_NOTICE`（[L61](src/lib/formats.ts#L61)）；`containerCodecs` / `containerAvailable` / `formatProbeNotice`（[L86-L113](src/lib/formats.ts#L86-L113)） |
| `VideoPanel.tsx` | 目标格式组改为 6 容器 `Segmented` + 兼容编码 `Segmented`，缺编码器的项 `disabled` 且 `title` 给原因（[L188-L227](src/panels/VideoPanel.tsx#L188-L227)）；`caps` 到达后若当前组合不可用，自动切到该容器首个可用编码（[L106-L114](src/panels/VideoPanel.tsx#L106-L114)） |
| `AudioPanel.tsx` | 6 格式 `Segmented`（[L158-L174](src/panels/AudioPanel.tsx#L158-L174)），不可用项灰显并说明；无损格式不显示码率 |
| `args.rs` | 容器 → muxer（[L843](src-tauri/src/args.rs#L843)）、`audio_encoder` 覆盖 6 格式（[L856](src-tauri/src/args.rs#L856)）、`build_audio` 无损跳过码率（[L867](src-tauri/src/args.rs#L867)） |
| `jobs.rs` | `kind_ext` 由 `params.container` / `params.format` 决定（[L227](src-tauri/src/jobs.rs#L227)）；`precheck_spec` 入队前再次校验（[L432](src-tauri/src/jobs.rs#L432)）；音轨编码按容器挑（[L2485](src-tauri/src/jobs.rs#L2485)） |
| `probe.rs` | 候选表补 `libvpx-vp9` / `libvpx` / `libsvtav1` / `libaom-av1` / `mpeg4` / `flac` / `pcm_s16le` / `libvorbis` / `libopus` |
| 预设 | `preset.rs` / `presetPrefs.ts` 白名单加 `container`，重启后容器选择可恢复 |

**探测不可用时的行为**（不再静默退化）：

- 探测未完成（`caps == null` 但 ffmpeg 可用）→ info：「正在探测本机编码器：格式清单已完整渲染，可用性会在探测完成后自动刷新。」
- 无 ffmpeg / 无任何可用视频或音频编码器 → warn：「未检测到 ffmpeg 或编码器不可用：格式清单保持完整，但不可用的选项已灰显并注明原因。」
- 缺某项编码器 → 该项**保留在列表里但灰显**，`title` 写明「本机没有可用于 … 的 … 编码器（需要 …）」。
- 选中组合不兼容 → 界面不提供该选项；绕过界面直接入队仍被 `precheck_spec` 拒绝。

**竞态处理选择：先渲染 + 订阅探测完成刷新**（不采用「等探测完成再渲染」）。
理由：清单是常量，先渲染不会缺项也不会闪；等待会让首屏出现空白或骨架屏，且探测超时/失败时
列表会永久为空——恰是用户反馈的那种「只剩默认项」观感。刷新由 `App` 的 `caps` state 自然触发。

### 断言结果（全部后台，无窗口）

| 验证 | 结果 | 覆盖 |
| --- | --- | --- |
| `cargo test` | **63 / 63**（原 55 + 8 新增） | 新增：容器 ≥4 / 音频 ≥5 / 不兼容组合拒绝 / 未知容器与格式拒绝 / 容器音轨候选兼容 / 编码器只从候选里挑 / 扩展名随容器与格式 / precheck 拒绝 |
| `npm run verify:formats`（新） | **7 / 7** | 容器数 6（≥4）、音频数 6（≥5）、必备项齐全、不兼容约束、探测失败提示存在且两面板渲染 |
| `p2-check`（LGPL FFmpeg + Poppler） | **10 / 10** | 新增 `format-matrix-*` 三条：数量 + 拒绝 + 6 容器与 6 格式真实转码 |
| `npm run verify:core` | **31 / 31** | P0/P1/P2 链路未回归 |
| `npm run verify:flavor:lgpl` / `:gpl` | 各 **7 / 7** | 变体未回归 |
| `npm run verify:setup` | **12 / 12** | 首启引导未回归 |
| `npm run verify:ui` | **39 / 39** | 引导页 headless |
| `node scripts/panel-persist-check.mjs` | **9 / 9** | 见下方脚本维护说明 |
| `npm run check:bundle` | **40 / 40** | 在**本轮新包**上重跑 |
| `npm run build` | 通过 | `tsc --noEmit` + `vite build`（67 modules） |

`p2-check` 格式矩阵真实产物（ffprobe 读取，非命令构造）：

| 类别 | 实测 |
| --- | --- |
| 视频容器 | mp4 `h264/mov,mp4` 132,482 B；mkv `h264/matroska,webm` 131,816 B；webm `vp9/matroska,webm` 55,184 B；mov `h264/mov,mp4` 132,533 B；ts `h264/mpegts` 140,624 B；avi `mpeg4/avi` 150,956 B |
| 音频格式 | mp3 12,897 B；aac→`.m4a` 13,071 B；flac 65,156 B；wav `pcm_s16le` 88,278 B；ogg `vorbis` 6,360 B；opus 15,328 B |
| 不兼容组合 | `validate_video("mp4","vp9")` 拒绝：`容器 MP4 不支持 vp9 编码，可组合：h264 / hevc / av1` |

> **脚本维护**：面板卡片改名（视频转 MP4→视频转码、音频转 MP3→音频转码）后，
> `panel-persist-check.mjs` 仍按旧标题定位，导致 1 次超时失败；已同步更新选择器，
> 并给 mock `caps` 补一条 `hevc` 编码器（新规则会在探测完成后把不可用编码自动回落）。
> 这是测试维护，不是产品行为变化。

### 本轮产物（`dist-artifacts/`，四包重建）

| 变体 | 安装包 | 字节 | SHA-256 |
| --- | --- | --- | --- |
| LGPL | `lgpl/Transmutio_0.1.0_x64_en-US.msi` | 2,662,400 | `88c3de015c52619eab81ef3345737b8a46fb4d4c74b199ada08793d768021859` |
| LGPL | `lgpl/Transmutio_0.1.0_x64-setup.exe` | 1,818,456 | `cab932f479942f87c5b7d39f5db6a977a3f85ac40ea8f6fa9a747b4c778c277e` |
| GPL | `gpl/Transmutio GPL_0.1.0_x64_en-US.msi` | 2,658,304 | `efd63dfbad6c4a715e3b52344db86047ff01e782cf80352d9536d6555454b293` |
| GPL | `gpl/Transmutio GPL_0.1.0_x64-setup.exe` | 1,815,220 | `3c2769a8a0bcdac68a9e385614d986b99f26463dd59c0c9635a2f2be05c27806` |

### 四、P0/P1/P2 交付项 × 界面入口对账

| 轮次 | 交付项 | 界面入口（用户实际能看到/用到） | 判定 |
| --- | --- | --- | --- |
| P0 | 视频压缩 / 画面裁剪入口接通 | 视频 › 视频压缩、画面裁剪 | 一致 |
| P0 | 旋转、比例裁剪 | 视频面板 › 画面调整 | 一致 |
| P0 | 音频响度归一化 | 音频 › 音量标准化 | 一致 |
| P0 | 每任务诊断日志 | 任务中心卡片「日志」 | 一致（窗口内未点验） |
| P1 | 合并 / 混流 / 转 GIF | 视频 › 对应三张卡片 | 一致 |
| P1 | 图片 EXIF 自动校正 | 图片面板开关 | 一致 |
| P1 | 暂停 / 继续 | 任务中心 | 一致（窗口内未点验） |
| P1 | 命名模板 / 冲突策略 | 各面板底部「输出命名」 | 一致 |
| P1 | 完成后动作 | 设置页 | 一致 |
| P1 | 面板参数持久化 | 无显式入口（自动生效） | 预期 |
| P2 | PDF 合并 / 拆分 / 转图片 | 文档分区三张卡片 | 一致；转图片依赖 `pdftoppm`，缺失时灰显 |
| P2 | 图片序列 → 视频 / 视频 → 序列 | 视频 › 两张卡片 | 一致 |
| P2 | 图片裁剪 / 拼图 | 图片 › 图片裁剪、图片拼图 | 一致 |
| P2 | 预设导入 / 导出 | 设置页 | 一致 |
| P2 | 自动重试 | 任务中心失败卡片「重试」 | 一致 |
| **本轮** | 视频 6 容器 | 视频面板 › 目标格式 › 容器 | **已补齐** |
| **本轮** | 音频 6 格式 | 音频面板 › 目标格式 › 编码格式 | **已补齐** |
| **本轮** | 不兼容组合不可选 | 同上（选项不出现 / 灰显） | **已补齐** |

**仍存在的界面落差**（已实现但入口/呈现不完整，不涉及缺功能）：

1. ~~「音频转 AAC」卡片名与能力不符~~：**已修**（§4.13）——与 audio-mp3 合并为单张「音频转码」，默认 AAC。
2. **输出扩展名可见性**：已给音频「编码格式」加了 `输出 .m4a` 标签；此前 `.m4a` 只藏在 tooltip。
3. ~~PDF 转图片灰显~~：**已修**（§4.13）——入口保持可点，面板内给安装说明 + 「重新检测」。
4. ~~`formatforge:*` localStorage 前缀~~：**已修**（§4.13）——启动时一次性迁移到 `transmutio:*`。
5. 水印、字幕、图片压缩、Office 互转、光盘/ISO、工具集仍是**真未实现**（入口置灰提示）；变速已在 §4.14 落地。

> 第 1/3/4 项在下一轮（§4.13）已落地；`localStorage` 前缀双写仅为旧版回滚兼容，一个版本周期后删除。

### 本轮未验证（需真机可见窗口，交由用户确认）

- 真实窗口内 6 容器 / 6 格式的**肉眼**渲染、灰显与 tooltip 原因；
- 切换容器时编码器自动联动、WebM 下 `原样复制` 禁用；
- 探测失败路径的文案在真实窗口中是否出现（本轮只做静态断言 + 无窗口逻辑验证）；
- 新包安装后首次启动的引导页与格式面板组合。

## 4.13 入口与存储标识修复（第四轮）

上一轮列出的四处界面落差中，三项不需要可见窗口，本轮完成。

### 改了什么

| 项 | 改动 | 关键文件 |
| --- | --- | --- |
| 音频入口合并 | 删除重复的 `audio-mp3` 卡片，保留 `audio-aac` 卡片并改名「音频转码」，描述「MP3 / AAC(m4a) / FLAC / WAV / OGG / Opus，默认 AAC」；面板逻辑不变（默认 AAC） | `src/catalog.ts`、`src/lib/presetPrefs.ts`、`src/components/MethodGrid.tsx` |
| localStorage 前缀迁移 | 新增 `src/lib/storage.ts`：启动时（模块加载）一次性把 `formatforge:*` 搬到 `transmutio:*`。顺序为「读旧 → 写新 → 读回校验 → 校验通过才删旧」，任一键失败保留旧键；标记键 `transmutio:storage-migration:v1` 保证幂等；新旧键冲突时不覆盖新值 | `src/lib/storage.ts`（新）、`src/lib/settings.ts`、`panelPrefs.ts`、`presetPrefs.ts` |
| 旧版回滚兼容 | 读取自动回退旧前缀；写入在**一个版本周期内**双写旧前缀（`MIRROR_LEGACY`，含 `TODO(v0.2)` 到期删除），因此旧版回滚仍能读到配置 | `src/lib/storage.ts` |
| pdftoppm 缺失 | 「PDF 转图片」不再置灰：入口可点、进入面板后显示「缺 pdftoppm（Poppler）/ 设置页『PDF 工具目录』填 `Library\bin` / `PDFTOPPM_PATH=...\pdftoppm.exe` / 查找顺序 PDFTOPPM_PATH → 设置目录 → `TRANSMUTIO_PDF_TOOLS` → PATH」，并提供「重新检测」（调用 `pdf_tools_status` 刷新，无需改 Rust 查找逻辑） | `src/App.tsx`、`src/panels/AdvancedPanels.tsx` |

迁移语义细节：已有新键且与旧值相同 → 只删旧键；不一致 → 保留旧键并计入 `conflicts`，绝不用旧值覆盖新值；
`localStorage` 不可用（隐私模式）时整体静默跳过，不影响启动。

### 断言结果（后台，无窗口）

| 验证 | 结果 | 新增覆盖 |
| --- | --- | --- |
| `cargo test` | **63 / 63** | 无回归（本轮未改 Rust） |
| `npm run verify:formats` | **10 / 10**（原 7 + 3） | 卡片合并与旧名残留、PDF 缺工具六条文案、迁移模块常量 |
| `node scripts/panel-persist-check.mjs` | **11 / 11**（原 9 + 2） | 前缀迁移：只注入旧键 → 新键存在且 JSON 等价、旧键删除、标记存在；reload 不重复迁移 |
| `npm run verify:core` | **31 / 31** | 无回归 |
| `verify:flavor:lgpl` / `:gpl` | 各 **7 / 7** | 无回归 |
| `npm run verify:setup` | **12 / 12** | 无回归 |
| `npm run verify:ui` | **39 / 39** | 无回归 |
| `npm run check:bundle` | **40 / 40** | 在**本轮新包**上通过 |
| `p2-check`（LGPL FFmpeg + Poppler） | **10 / 10** | 无回归 |

### 本轮产物（`dist-artifacts/`，四包重建）

| 变体 | 安装包 | 字节 | SHA-256 |
| --- | --- | --- | --- |
| LGPL | `lgpl/Transmutio_0.1.0_x64_en-US.msi` | 2,666,496 | `2578877171f93ae4fa61d0b41652a2320842a0d86928a4043dfabe85e204f208` |
| LGPL | `lgpl/Transmutio_0.1.0_x64-setup.exe` | 1,819,629 | `c0bb02890764ea627950e3a2496aa9e09ed4154a4bc2bfa496333c49ab710d80` |
| GPL | `gpl/Transmutio GPL_0.1.0_x64_en-US.msi` | 2,658,304 | `c153cbfaf826b4dd62272928a28ae6cd83f7aa304d48a34a6a343e1b48e7a750` |
| GPL | `gpl/Transmutio GPL_0.1.0_x64-setup.exe` | 1,815,679 | `cf649e12940612d20350b88f528dace9fe0992fea4e00565ad76ad29460bdcdf` |

### 仍需人工开窗确认（本轮未开窗、未点 UAC）

- 视频 6 容器肉眼齐全、灰显项 tooltip 原因；
- WebM 下 VP9 / Opus 联动、`原样复制` 禁用；
- 删除 / 移走 ffmpeg 后的「未检测到 ffmpeg 或编码器不可用」提示；
- 新版「PDF 转图片」在缺 `pdftoppm` 机器上的说明与「重新检测」实际点击效果（本机已装 Poppler，只验证了正路径与静态文案）。

## 4.14 变速（playback speed）（第五轮）

视频面板与音频面板都加入「变速」，后端参数构造集中在 `src-tauri/src/args.rs`，面板只传结构化参数（`speed` / `keepPitch`），不拼滤镜字符串。

### 改动点

| 层 | 改动 | 关键文件 |
| --- | --- | --- |
| 参数结构 | `VideoParams` / `AudioParams` 新增 `speed: f64`（1.0 = 不变）、`keep_pitch: bool`；`keep_pitch` 用 `#[serde(default = "default_true")]`，**缺字段即保持音高** | `src-tauri/src/args.rs` |
| 链式拆分 | `atempo_factors(speed)`：单次 `atempo` 只接受 0.5–2.0，按「>2 反复除以 2、<0.5 反复乘 2、余数一次」贪心分解；`speed_filters(speed, keep_pitch)` 统一产出 `(vf 片段, af 片段)`，1.0 返回 `None` | `src-tauri/src/args.rs` |
| 视频滤镜 | `build_video`：`crop → rotation(transpose / hflip+vflip) → scale → setpts=PTS/<speed> → [format=nv12,hwupload（vaapi）]`，`-vf` 只 push 一次 | `src-tauri/src/args.rs` |
| 音频滤镜 | 视频任务转码音轨：`-af` 只 push 一次（变速）；音频任务：`变速 → loudnorm(-16 LUFS)` 合并成一个 `-af` | `src-tauri/src/args.rs`（`build_video` / `build_audio`） |
| 入队校验 | `precheck_spec`：`video-mp4` / `audio-mp3` / `audio-aac` 先 `validate_speed`；变速 + `audio_mode=copy` 直接拒绝（改音轨时长与 stream copy 冲突） | `src-tauri/src/jobs.rs` |
| 面板 | 预设 `0.5 / 0.75 / 1.25 / 1.5 / 2`（1 不单列，默认值即 1 = 原速）+ 自定义 `0.25–4.0`、步进 `0.05`；「保持音高」开关默认开；变速时「原样复制」禁用并自动切到 AAC；输出时长估算按倍速缩放 | `src/lib/speed.ts`（新）、`src/panels/VideoPanel.tsx`、`src/panels/AudioPanel.tsx`、`src/lib/api.ts`、`src/components/Icon.tsx` |

### `atempo` 链规则

* 单次 `atempo` 因子必须在 `[0.5, 2.0]`，越界由 `atempo_factors` 展开成多级：
  * `0.25` → `atempo=0.5,atempo=0.5`
  * `4.0` → `atempo=2,atempo=2`
  * `2.5` → `atempo=2,atempo=1.25`
  * `3.0` → `atempo=2,atempo=1.5`
* 拆分后用 `1e-6` 容差断言「所有因子的乘积 == 目标倍速」且每个因子都在 0.5–2.0 内。
* 视频侧永远是单个 `setpts=PTS/<speed>`，不进 `atempo` 链，也不与任何其他滤镜重复。

### 保持音高开关

* **开（默认）**：`atempo` 链，音高不变，时长按倍速缩放。
* **关**：`aresample=48000,asetrate=<48000*speed>,aresample=48000`，音高随速度整体升降。副作用：输出采样率被固定为 48 kHz（与「采样率=保持原始」不再一致），极快档位高频会失真，且需要更多码率；因此面板在关闭时明确写出这一点。

### 组合优先级（不允许互相覆盖）

| 组合 | 顺序 / 规则 |
| --- | --- |
| 变速 + 裁剪 + 旋转 + 缩放 | 视频链 `crop → rotation → scale → setpts`，`setpts` 恒为最后一环（VAAPI 前的 `format/hwupload` 除外），断言 `setpts=` 只出现一次 |
| 变速 + 响度归一化（音频任务） | `atempo 链 → loudnorm`，断言只有一个 `-af` |
| 变速 + concat（视频合并） | `video-merge` 无 `speed` 参数，合并任务不参与变速；需要变速时先对单文件变速再合并。避免「合并流 copy 被 setpts 污染」 |
| 变速 + 原样复制音轨 | **拒绝**：音轨 stream copy 不会随画面变速，必然音画不同步 |
| 变速 + 硬件编码 | `setpts` 在 CPU 滤镜链中完成，VAAPI 只追加 `format=nv12,hwupload`，不再插入第二个时间滤镜 |

### 显式拒绝（不静默输出）

* `speed` 在 `(0, 0.25)` 或 `(4.0, ∞)`、`NaN`/`inf` → `validate_speed` 返回可读错误，任务入队即失败。
* `speed == 0` 与负数在后端按「未启用」处理（返回 1.0）；前端控件则**拒绝写入** 0 与负数，保留上一次合法值。
* 变速 + `audio_mode=copy` → 拒绝，提示「改为按容器转码或移除音轨」。
* **GIF** 走 `GifParams`（`fps/width/start/duration`），没有倍速字段，面板也不显示倍速控件；**音频-only** 任务走 `build_audio`，只用 `-af`，绝不带 `setpts`。
* 混流（`video-mux`）与合并（`video-merge`）不接收 `speed`，前端也不会透传。

### 断言结果（后台，无窗口）

| 验证 | 结果 | 覆盖 |
| --- | --- | --- |
| `cargo test` | **74 / 74**（原 63 + 11 条变速相关，含 1 条默认值） | 1.0 不产生滤镜；1.5 单 `atempo=1.5`；0.25 双 0.5；4.0 双 2；2.5 = `2,1.25`；乘积 1e-6；变速+裁剪+旋转链顺序与 `setpts` 唯一；关音高走 `asetrate`；越界拒绝；缺 `keepPitch` 默认 true；`atempo` 在 `loudnorm` 之前 |
| `npm run verify:formats` | **13 / 13** | `speed.ts` 常量/预设/越界逻辑，两面板引用与「保持音高」，`args.rs` 含 `atempo_factors`/`setpts`/`validate_speed` |
| `p2-check`（LGPL FFmpeg） | **13 / 13** | `speed-matrix`：`atempo_factors(2.5)` 乘积 2.5000；真实 1.5× → **1.37 s**（期望 2.00/1.5=1.33 s）、0.5× → **3.98 s**（期望 4.00 s），容差 0.1 s；两条产物音频 **48000 Hz / 2 声道**与原片一致 |
| `npm run build` | 通过 | 69 modules，372.00 kB |
| `verify:core` / `verify:setup` / `verify:ui` / `verify:flavor:*` | 无回归 | 本轮未改这些链路 |

### 本轮产物（`dist-artifacts/`，四包重建）

| 变体 | 安装包 | 字节 | SHA-256 |
| --- | --- | --- | --- |
| LGPL | `lgpl/Transmutio_0.1.0_x64_en-US.msi` | 2,666,496 | `b6ac2b69d257f4ec2151b5776914b17aa6bc488d93a88702f69b5c79699352ec` |
| LGPL | `lgpl/Transmutio_0.1.0_x64-setup.exe` | 1,821,939 | `61120d20e1e83d7fe9e76aab7d8cc559b92487f35432b0b97fefc34a3f8f310f` |
| GPL | `gpl/Transmutio GPL_0.1.0_x64_en-US.msi` | 2,662,400 | `7c61e491554e50db8acb9776d95ff4f35525e386b0d68dda28543f6b5df21f17` |
| GPL | `gpl/Transmutio GPL_0.1.0_x64-setup.exe` | 1,818,110 | `9cfc6e90a615b432b65604d2009d0aef43fdc5f6b1e0d9079df62c488dd18d5b` |

### 仍需人工开窗确认（本轮未开窗、未点 UAC）

* 预设段选中态与自定义输入框的步进手感（0.05），输入 0 / 负数 / 5 时是否回弹到上一次合法值；
* 选变速后「原样复制」是否立即禁用并自动切到 AAC（静态断言只覆盖逻辑，不覆盖真实渲染）；
* 「保持音高」关闭时的说明文案在窄窗口下是否完整；
* 1.5× / 0.5× 变速后的**主观音画同步与音质**（后台只断言时长、采样率、声道）。

## 4.15 水印（图片 + 视频）（第六轮）

两张「水印」卡片由置灰改为可用：视频走 `video-mp4`、图片走 `image`，kind 不变，避免新增任务类型。参数全部集中在 `args.rs`，面板只传结构化的 `watermark` 对象。

### 改动点

| 层 | 内容 | 位置 |
| --- | --- | --- |
| 参数结构 | `WatermarkParams { enabled, path, position, x, y, opacity, scalePercent, margin, rotation }`，`#[serde(default)]` 挂在 `VideoParams` / `ImageParams` 上；底图尺寸用 `#[serde(skip)]` 字段回填 | `src-tauri/src/args.rs` |
| 校验 | `validate_watermark`：空路径 / 文件不存在 / 非 png-jpg-jpeg-webp / opacity∉0..1 / scale∉5..50 / rotation∉±180 / 未知位置 → 可读错误，入队即失败 | `args.rs`、`jobs.rs::precheck_spec` |
| 尺寸探测 | 任务执行前用 ffprobe 读取底图（视频含裁剪/旋转后的输出宽高），按「短边 × scalePercent」算出水印像素尺寸；探测失败显式报错，不静默退化 | `jobs.rs`（`watermark_dims_for_job`）、`args.rs`（`watermark_target_size`） |
| 坐标 | 纯函数 `watermark_position`：九宫格 + 自定义；custom 越界**钳制**到 `[0, W-w] × [0, H-h]` | `args.rs` |
| 视频链 | 主链搬入 `-filter_complex`：`[0:v]crop→rotation→scale→setpts[base]`，`[1:v]scale,format=rgba,colorchannelmixer=aa=op[,rotate][wm]`，`[base][wm]overlay=x:y:format=auto:shortest=1[outv]`；水印输入 `-loop 1`，输出再加 `-shortest`；VAAPI 的 `format=nv12,hwupload` 移到 overlay 之后 | `args.rs::build_video` |
| 图片链 | 同样改为 `-filter_complex`，图片不需要 `shortest`；JPEG 输出 `yuvj420p`、WebP `yuv420p`、PNG `rgb24`，透明 PNG 水印叠 JPEG 由 overlay 自动合成，不报透明像素错误 | `args.rs::build_image` |
| 面板 | 两张水印卡启用；VideoPanel / ImagePanel 新增「水印」分组（启用开关、选择图片、3×3 位置 + 自定义 x/y、不透明度 0–1 步进 0.05、缩放 5–50「相对短边 %」、边距、旋转） | `src/catalog.ts`、`src/lib/watermark.ts`（新）、`src/panels/VideoPanel.tsx`、`src/panels/ImagePanel.tsx`、`src/lib/api.ts` |

### 滤镜链与时长处理

- **视频**：`setpts` 只在主链出现一次；overlay 在 setpts 之后，因此叠加发生在变速后的时间轴上，水印不被二次变速。
- **时长**：水印是 `-loop 1` 的静态图（无限长），靠 `-shortest` 截断到主视频长度；断言真实输出 mp4 1.33 s、mkv 1.36 s（期望 1.33 s，容差 0.1 s）。
- **不重复叠加**：有水印时恰好只有一个 `-filter_complex`、`-vf` 为 0；无水印时保持原 `-vf` 路径，行为不变。
- **失败回退**：水印路径缺失/格式不支持时入队失败，文案 `水印图片不存在：<path>（请重新选择水印图片，或关闭水印）`，绝不静默跳过。
- **不支持**：GIF、合并（concat）、混流不接收水印参数；BMP/TIFF 输出未做水印像素格式验证（本轮只保证 JPEG/PNG/WebP）。

### 断言结果（后台，无窗口）

| 验证 | 结果 | 覆盖 |
| --- | --- | --- |
| `cargo test` | **85 / 85**（原 74 + 11 条水印） | 九宫格 9 位置 × 边距坐标；custom 负值/超界钳制；20% 短边 → 640×480 底图得 96 px；水印大于底图被压到不超过底图；opacity=0.5 进入滤镜串；水印+裁剪+旋转+变速链顺序、`setpts` 仅 1 次、`-vf` 与 `-filter_complex` 不共存；三种图片格式滤镜串；`validate_watermark` 六类拒绝 |
| `npm run verify:formats` | **17 / 17**（原 13 + 4） | 两张水印卡已启用、两个面板含水印控件与位置枚举、`api.ts` 定义 `WatermarkParams` |
| `p2-check`（LGPL FFmpeg） | **19 / 19**（原 13 + 6） | 见下表真跑数字 |
| `npm run build` | 通过 | tsc + vite |

真跑明细（`p2-check`）：

| 用例 | 结果 |
| --- | --- |
| PNG 水印 → JPEG | 640×480 / `yuvj420p`；右下角采样 **RGB(232,10,19)**（底图为纯蓝）→ 水印确实出现在画面上 |
| PNG 水印 → WebP | 640×480 / `yuv420p`；采样 RGB(232,10,19) |
| PNG 水印 → PNG | 640×480 / `rgb24`；采样 RGB(233,10,20) |
| 视频 mp4（水印 + 1.5× 变速） | 320×240、**1.33 s**（期望 1.33 s）、音频 48 kHz / 2 声道 |
| 视频 mkv（水印 + 1.5× 变速） | 320×240、**1.36 s**（容差内）、音频 48 kHz / 2 声道 |
| 不存在的水印路径 | 入队拒绝，文案含「水印图片不存在」 |

像素抽样方法：对输出视频/图片右下角 `40×40` 区域 `crop` 后转 `rawvideo rgb24`，求三通道均值；底图为纯蓝，红水印落点均值 R≈232 说明不是「只有参数对」。

### 本轮产物（`dist-artifacts/`，四包重建）

| 变体 | 安装包 | 字节 | SHA-256 |
| --- | --- | --- | --- |
| LGPL | `lgpl/Transmutio_0.1.0_x64_en-US.msi` | 2,678,784 | `8f844f35e9df7b0327b3ea94941d9e290321f62b3182adbbd67c3ba804326781` |
| LGPL | `lgpl/Transmutio_0.1.0_x64-setup.exe` | 1,831,684 | `c4d197e22dfffe74d763ccf29b0599bd898587f5bd870c5120e90fc7b2adfabf` |
| GPL | `gpl/Transmutio GPL_0.1.0_x64_en-US.msi` | 2,678,784 | `8f5f2557861f357cf2f11a257a96ce017811ff382b09b7d6a1d899c78071c525` |
| GPL | `gpl/Transmutio GPL_0.1.0_x64-setup.exe` | 1,827,141 | `8d7a730f783cad378d6af126b24418ad858472fb0eae9cd08693301b26815038` |

### 仍需人工开窗确认（本轮未开窗、未点 UAC）

* 两张水印卡在真实窗口内可点、进入面板后控件齐全（九宫格选中态、滑块手感）；
* 选图对话框的过滤器（png/jpg/jpeg/webp）与路径显示；
* 水印 + 变速 + 裁剪组合时的**主观**画面位置与时长（后台已断言时长/分辨率/像素）；
* BMP/TIFF 输出叠加水印未验证；GIF/合并/混流入口未提供水印控件（符合本轮范围）。

## 4.16 字幕（内嵌 / 烧录）（第七轮）

「添加字幕」卡启用，kind 仍为 `video-mp4`。参数集中在 `args.rs`，面板只传结构化的 `subtitle` 对象；不引入新依赖，烧录依赖 FFmpeg 内置 `subtitles`（libass）。

### 两条路径的实现差异

| | 内嵌（soft） | 烧录（hard） |
| --- | --- | --- |
| 原理 | 字幕作为独立轨道写入容器，播放器可开关 | `subtitles` 滤镜把文字画进像素，不可关闭 |
| 映射 | 字幕文件各用独立 `-i`，按 `1:s:0`、`2:s:0`… 逐条 `-map` | 无额外轨道，只加一个滤镜 |
| 编码 | mp4/mov → `-c:s mov_text`；mkv → `-c:s copy`（srt/ass/ssa/webvtt） | 不涉及字幕编码，只转视频画面 |
| 元数据 | `-metadata:s:s:<n> language=<lang>`；`default_track` → `-disposition:s:0 default` | 无（样式写进画面） |
| 多文件 | 支持，同一任务内合并多条字幕轨 | 不支持（一次只烧一条；面板也只给主字幕） |
| 与变速 | 可共存（字幕轨不经过视频滤镜，时间轴由播放器按倍速处理） | **禁止**（见下） |
| 滤镜链 | 不进入 `-vf` / `-filter_complex` | `crop → rotation → scale → subtitles → [水印 overlay] → VAAPI` |

### 变速 + 烧录的取舍

选择「**变速时禁用烧录**」：`subtitles` 按帧时间戳渲染文字，而 `setpts` 会改写视频 PTS、音频又由 `atempo` 独立变速；把 `subtitles` 放在 `setpts` 前后都无法同时保证文字出现时刻与音频一致（放前面会整体压缩字幕时间、放后面则按变速后的时间重采样字幕），必然出现字幕提前/滞后。与其给出「看起来能跑」的结果，不如显式拒绝：

> 变速时不能烧录字幕（字幕时间轴会与变速后的画面/音频不同步）：请改为内嵌字幕，或把倍速设为 1

内嵌字幕 + 变速仍允许（封装层只存轨道，播放器按倍速回放）。

### 容器能力矩阵

| 容器 | 内嵌 soft | 烧录 hard |
| --- | --- | --- |
| MP4 | `mov_text` | 支持 |
| MOV | `mov_text` | 支持 |
| MKV | `srt/ass/ssa/webvtt` copy | 支持 |
| TS | **拒绝**（不承载内嵌字幕） | 支持（烧进画面） |
| WebM | **拒绝** | 支持 |
| AVI | **拒绝** | 支持 |

不支持内嵌的容器会返回可读错误并由面板提前禁用，**不会生成静默丢字幕的文件**。

### libass 检测与降级

`probe.rs` 新增 `subtitleBurn` 能力：先查 `ffmpeg -filters` 是否含 `subtitles`，否则查 `-buildconf` 是否带 `--enable-libass`；任一路径缺失即 false。本机 LGPL 构建为 **true**。为 false 时面板禁用「烧录」并说明原因，后端运行期也会显式失败。

### 失败态

字幕文件不存在 / 空路径 / 扩展名不在 srt/ass/ssa/vtt、附加轨缺失、容器不支持内嵌、变速+烧录、libass 缺失 —— 全部显式失败或面板灰显，文案可操作（例：`字幕文件不存在：<path>（请重新选择字幕文件，或关闭字幕）`）。

### 断言结果（后台，无窗口）

| 验证 | 结果 | 覆盖 |
| --- | --- | --- |
| `cargo test` | **95 / 95**（原 85 + 10 条字幕） | 单条/多条 soft 映射、默认轨与语言标签、三种容器 soft 能力与三种拒绝、hard 滤镜串（字号/描边/位置/颜色）、变速+烧录拒绝、内嵌+变速允许且 `setpts` 仅 1 次、libass=false 失败 |
| `npm run verify:formats` | **22 / 22**（原 17 + 5） | 字幕卡启用、面板控件、`SubtitleParams`/`subtitleBurn`、容器不兼容与变速+烧录的阻止逻辑 |
| `p2-check`（LGPL FFmpeg） | **24 / 24**（原 19 + 5） | 见真跑表 |
| `cargo fmt --check` | 通过 | — |

真跑明细（全部 ffprobe 复核）：

| 用例 | 结果 |
| --- | --- |
| MP4 内嵌 | 1 条 `mov_text` 字幕流、`disposition.default=1`、`language=chi` |
| MKV 内嵌 | 字幕流存在、`subrip`、`language=chi` |
| 烧录 MP4 | 字幕区域像素均值与「无字幕版」相差 **19.7**（阈值 >15）→ 文字确实画进画面；时长 **2.00 s**、音频 48 kHz / 2 声道未变 |
| WebM 内嵌 | 拒绝，文案含「不承载内嵌字幕轨」 |
| 字幕文件缺失 | 拒绝，文案含「字幕文件不存在」 |

> 说明：p2 样片仅 320×240，默认 4.5% 字号约 11 px，固定采样区差值不足以稳定超过阈值，故真跑改用 12%（约 29 px）验证像素变化；产品默认值与功能未改。

### 本轮产物（`dist-artifacts/`，四包重建）

| 变体 | 安装包 | 字节 | SHA-256 |
| --- | --- | --- | --- |
| LGPL | `lgpl/Transmutio_0.1.0_x64_en-US.msi` | 2,695,168 | `9f152bccc3a1e1270780c0150d2cd8b762e5f399bb413d7efc0938a485ee40bc` |
| LGPL | `lgpl/Transmutio_0.1.0_x64-setup.exe` | 1,839,931 | `f933a185cfb5e192f0a1510a5f6dfffd56edc756e9727db4a68200b08faf1e85` |
| GPL | `gpl/Transmutio GPL_0.1.0_x64_en-US.msi` | 2,686,976 | `d82b0d5501690d2d9ca2c569941d5f4c6fd7b6aa826639e853661fb348f76964` |
| GPL | `gpl/Transmutio GPL_0.1.0_x64-setup.exe` | 1,838,344 | `dd7e6ab931bd150a5e37e642f019416f795998cc2116b77e54a284d748c039e4` |

### 仍需人工开窗确认（本轮未开窗）

* 「添加字幕」卡可点、面板控件齐全（模式切换、附加轨列表、颜色输入）；
* 选字幕文件的过滤器（srt/ass/ssa/vtt）与路径显示；
* 本机 `subtitleBurn=true` 时「烧录」可选；人为换成无 libass 的 FFmpeg 时应灰显并给原因；
* 烧录字幕的主观字体/描边/位置观感（后台只断言像素确实变化）。

## 4.17 图片压缩（第八轮）与总账

新增 kind `image-compress`，卡片「图片压缩」启用；参数集中在 `args.rs` / `compress.rs`，面板只传结构化的 `compress` 对象。

### 两种模式与二分策略

| 模式 | 行为 |
| --- | --- |
| 目标大小（target） | 在 `[质量下限, 100]` 内二分：找「输出字节 ≤ 目标」的**最高质量**；最多 8 次；可行解与目标差值 ≤ 5% 即停；每轮写临时文件、用文件系统字节数测量 |
| 固定质量（quality） | 一次编码，不二分 |

- 目标口径：绝对 KB 或「原图百分比」（1–99）。
- **下限保护**：JPEG 下限 40、WebP 下限 35。下限仍超目标时停止，不再压到糊；输出保留，`targetMet=false`，`message` 明确「已压到质量 X 仍未达到目标大小，输出为 Y KB」；任务状态为 done + 未达标，**不是成功**。
- 每轮迭代的（质量, 字节）写入 per-job 日志，并在 `Job` 字段 `attemptsUsed` / `qualityUsed` / `targetMet` / `targetBytes` 暴露给界面复核。
- 批量：每张图独立二分与独立任务，单张失败不中断整批；汇总为「成功 N / 未达标 M / 失败 K」。
- 体积与尺寸：压缩不改变尺寸；若同时裁剪/旋转/缩放/水印，顺序为「裁剪 → 旋转 → 缩放 → 水印 → 编码」，二分在最终编码参数上迭代。

### 量纲坑（已用单测锁死）

| 编码 | 参数 | 映射 |
| --- | --- | --- |
| JPEG | `-q:v` | mjpeg 的 2–31 是**反向**量纲：质量 100 → 2，质量 1 → 31，质量 40 → 20；单调性已断言 |
| WebP | `-quality` | 1–100 正向原样；`-lossless 1` 仅固定质量可用，target + lossless 直接拒绝 |
| PNG | `-compression_level` / `pal8` | 1–100 → 0–9；target 模式只做一次最大压缩（默认调色板量化），不做二分，文案明确「PNG 重编码收益有限」 |
| BMP / TIFF | — | 压缩不适用，precheck 拒绝并给原因 |

元数据：默认 `-map_metadata -1`（剥离 EXIF/ICC，体积更小）；`keep_metadata=true` 时 `-map_metadata 0`。

### 真跑结果（LGPL FFmpeg）

| 用例 | 实测 |
| --- | --- |
| JPEG 1280×960，目标 50% | 原图 **80,307 B** → 目标 40,154 B → 输出 **39,094 B**；**3 轮**，质量 77；尺寸仍 1280×960 |
| 极低目标（5 KB） | 输出 **23,602 B**，质量 **40（下限）**，`targetMet=false`，文案「已压到质量 40 仍未达到目标大小，输出为 23.0 KB」 |
| WebP | 有损 quality=70 → 26,634 B；无损 quality=100 → 195,756 B；codec=webp |
| 元数据 | 默认 `-map_metadata -1`；保留时 `-map_metadata 0` 已写入（本机静态图编码器不回写 title，故以参数与 tag 双重校验） |
| 批量 3 张 | success=2 / unmet=1 / failed=0（q=89 / q=77 达标，q=40 未达标） |
| PNG 目标 30 KB | 原图 213,547 B → 输出 84,996 B，迭代 1 次，`targetMet=false`，文案含「PNG 重编码收益有限」 |

### 断言结果

| 验证 | 结果 |
| --- | --- |
| `cargo test` | **107 / 107**（原 95 + 12 条压缩） |
| `npm run verify:formats` | **27 / 27**（原 22 + 5） |
| `p2-check` | **30 / 30**（原 24 + 6） |
| `npm run check:bundle` | **40 / 40** |
| `cargo fmt --check` | 通过 |

### 四包（重建）

| 变体 | 产物 | 字节 | SHA-256 |
| --- | --- | --- | --- |
| LGPL | MSI | 2 711 552 | `35e5ad2ea13d9e67fd51755699cc882a98127a881107e9536dd7af5ba0690244` |
| LGPL | NSIS setup | 1 855 158 | `bb2f967705388cdbc434e5bd251620259f547b4da01e2ef6d88c8a6c37c0da56` |
| GPL | MSI | 2 707 456 | `1f5b3a3b4588c3b21cd74ff24a91114966899f2950353672c96ba1a07eb0a1b1` |
| GPL | NSIS setup | 1 852 842 | `67bafc40490aba8586d8e107d72010d33a4c74db49972bc80a8c4856de636f89` |

### 仍需人工开窗确认

* 「图片压缩」卡可点、压缩分组控件齐全（目标/质量切换、KB/百分比、元数据开关）；
* 任务行「未达标」徽标与「成功/未达标/失败」汇总的真实渲染；
* 极低目标时提示文案与任务状态在窗口内的呈现；
* PNG 提示在真实窗口中的可读性；PNG 调色板开关当前未暴露（固定开启）。

---

## 4.18 总账：已完成 / 部分 / 未实现

| 类别 | 条目 | 状态 | 入口 / 说明 |
| --- | --- | --- | --- |
| P0 | 视频压缩、画面裁剪 | 已实现 | 视频 › 视频压缩、画面裁剪 |
| P0 | 旋转、比例裁剪 | 已实现 | 视频面板 › 画面调整 |
| P0 | 音频响度归一化 | 已实现 | 音频 › 音量标准化 |
| P0 | 每任务诊断日志 | 已实现 | 任务中心「日志」（窗口内未点验） |
| P1 | 合并 / 混流 / 转 GIF | 已实现 | 视频 › 三张卡片 |
| P1 | 图片 EXIF 自动校正 | 已实现 | 图片面板开关 |
| P1 | 暂停 / 继续 | 已实现 | 任务中心（窗口内未点验） |
| P1 | 命名模板 / 冲突策略 | 已实现 | 各面板「输出命名」 |
| P1 | 完成后动作 | 已实现 | 设置页 |
| P1 | 面板参数持久化 | 已实现 | 自动生效 |
| P2 | PDF 合并 / 拆分 / 转图片 | 已实现 | 文档 › 三张卡片；转图片依赖 `pdftoppm`，缺失时可点 + 说明 + 重新检测 |
| P2 | 图片序列 → 视频 / 视频 → 序列 | 已实现 | 视频 › 两张卡片 |
| P2 | 图片裁剪 / 拼图 | 已实现 | 图片 › 两张卡片 |
| P2 | 预设导入 / 导出 | 已实现 | 设置页 |
| P2 | 自动重试 | 已实现 | 任务中心失败卡片 |
| 加固 | 视频 6 容器 / 音频 6 格式 + 不兼容组合拒绝 | 已实现 | §4.12 |
| 加固 | `transmutio:*` 存储迁移、旧版回滚兼容 | 已实现 | §4.13 |
| 加固 | 双变体（LGPL/GPL）×（MSI/NSIS） | 已实现 | 每轮重出四包 |
| 加固 | Git 私有仓库 + CI（frontend/backend/ui-headless） | 已实现 | `.github/workflows/ci.yml` |
| 加固 | Release workflow（tag v*，声明不含 FFmpeg） | 已实现 | `release.yml` |
| 加固 | 更新通道（latest.json / 密钥分离 / 灰度回滚） | **部分** | `UPDATER.md` 仅方案，未启用 |
| 队列① | CI 收尾至绿 | 已实现 | 全部提交均以 `gh run view` 复核 |
| 队列② | 更新通道骨架 | **部分** | 方案 + 接口，未接真实密钥 |
| 队列③ | 变速（0.25–4.0，atempo 链，保持音高） | 已实现 | §4.14；并入 `video-mp4` / 音频任务 |
| 队列④ | 水印（图片 + 视频，九宫格/缩放/透明度/旋转） | 已实现（限 `image` / `video-mp4`） | §4.15；GIF/合并/混流不提供，BMP/TIFF 未验证 |
| 队列⑤ | 字幕（内嵌 mov_text/copy；烧录 subtitles/libass） | 已实现（烧录与变速互斥） | §4.16；无 libass 时灰显 |
| 队列⑥ | 图片压缩（目标大小二分 + 固定质量） | 已实现（PNG 目标受限） | §4.17；PNG 只做最大压缩/调色板，不承诺达标 |
| 未实现 | Office 互转、文档转 PDF | 未实现 | 入口置灰 |
| 未实现 | 光驱 / DVD / ISO | 未实现 | 入口占位 |
| 未实现 | 工具集（改名 / 校验 / 信息 / 截图） | 未实现 | 入口占位 |
| 队列⑦ | 视频音量（相对增益 / 静音）与视频分割（按时间点，copy/encode） | 已实现（音量见 §4.19；分割仅按时间点切） | 视频 › 视频音量（面板内分组）、视频分割 |
| 队列⑧ | 图片转 GIF（多图 palettegen/paletteuse 两遍） | 已实现（多图 palettegen/paletteuse） | 图片 › 图片转 GIF |
| 未实现 | EXIF 元数据写入 | 未实现 | 入口置灰 |
| 未实现 | AVIF 编码 | 未实现 | 不在图片格式清单（JPG/PNG/WebP/BMP/TIFF） |
| 未实现 | 更新通道实装（密钥 / 签名 / 灰度） | 未实现 | 待用户拍板后再做 |

---

## 4.19 音量 / 分割 / 图片 GIF（第九轮）

本轮把三张原「未实现」入口补齐：视频音量（相对增益 + 静音）、视频分割（按时间点切一段一个任务）、图片转 GIF（多图两遍调色板）。三者都只动现有参数链路，不引入新依赖。

### 4.19.1 视频音量（相对增益）vs 音频卡「响度归一化」

- 数据模型：`VolumeParams { enabled, gain_db, mute }`，嵌入 `VideoParams` 与 `AudioParams`；`validate_volume` 要求 gain 有限且落在 `[-20, 20]`，静音时忽略 gain。
- 滤镜链顺序：`volume=<gain>dB` 放在音频链最前，其后才是 `atempo`（视频/音频）与 `loudnorm`（仅音频卡）；整链只生成一个 `-af`。静音在视频侧走 `-an` 且不再 `-map 0:a:0?`；音频侧没有音轨可丢，改用 `volume=0` 输出等长静音。
- 为什么与 loudnorm 互斥：`loudnorm` 是「测量整体响度后拉回 -16 LUFS」的归一化，任何相对增益都会被它重新拉平——用户看到的 +6 dB 最终仍接近 -16 LUFS，语义冲突且结果不可预期。因此 `validate_audio_volume` 在 `normalize && volume.enabled` 时直接拒绝，前端在音量启用时禁用归一化开关并显示原因。
- `audio_mode=copy` 与音量增益不能共存：流复制不做滤镜，precheck 明确拒绝并提示改「按容器转码」或关闭增益；前端也把「原样复制」置灰并自动切回 AAC。

### 4.19.2 视频分割（按时间点，第一期）

- 一个时间段 = 一个任务：前端按分段的文本逐行拆成多个 `JobSpec`（kind `video-split`），文件名追加 `-partN`，任务名追加 `（开始s–结束s）`，任一段失败都能在任务中心单独定位。
- `copy`：`-ss/-to` 输入侧定位 + `-map 0 -c copy -avoid_negative_ts make_zero`，速度快、无损，但只能切在关键帧，起点会带上 GOP 量级的偏移；`make_zero` 让每段时间戳从 0 起。
- `encode`：`-c:v <encoder> -preset veryfast -crf 20 -pix_fmt yuv420p -c:a aac -b:a 192k`（MP4 加 `-movflags +faststart`），帧级准确，代价是重编码时间与质量代际损失。编码器仍由 jobs 用 `pick_sequence_encoder(caps)` 选，LGPL 构建只有 `libopenh264`。
- 实测：p2-check 用 1280×720、GOP=10 帧（0.333s）的样片，copy 三段（0–3 / 3–6 / 6–9s）实测偏移均在 ±0.5s 内；encode 2–4s 实测偏差 ≤1 帧（0.04s）。偏移会写进任务消息与任务日志（「切分时长核对」）。
- 取舍：第一期不支持按关键帧列表 / 按大小 / 按章节切分，也不做分段后自动合并。

### 4.19.3 图片转 GIF（palettegen + paletteuse 两遍）

- 两遍：第一遍 `fps,scale,palettegen=stats_mode=diff` 生成全局调色板；第二遍用同一调色板 `paletteuse=dither=sierra2_4a`（可关）合成 GIF。单遍 `-vf gif` 每帧独立量化，跨帧颜色会跳，故禁用。
- concat 清单：jobs 在临时目录写 `file '<abs>'` + `duration 1/fps`，最后一张重复一行（concat demuxer 要求）；文件名单引号按 `'\''` 转义。注意重复行会让 `fps` 滤镜多输出一帧（尾帧重复），第二遍按图片张数加 `-frames:v N` 封顶，保证 GIF 帧数严格等于图片张数。
- 宽度：`max_width>0` 用它，否则取首图宽度；>2000 时前端提示并自动填 1280（后端不改参数，避免隐式行为）。
- 排序：前端按文件名 `localeCompare(..., {numeric:true})` 自然排序后合并成一个任务（`input` 第一张、`inputs` 其余），后端 precheck 要求至少 2 张、fps 1–30。
- 实测：p2-check 用红/绿/蓝三张 64×48 图生成 GIF，ffprobe 复核帧数 3、尺寸 64×48、NETSCAPE2.0 循环次数 0（无限），抽第 2 帧像素为绿色；第 1 遍结束后调色板文件存在，证明两遍都执行。
- 取舍：暂不做逐帧时长（统一 1/fps）、不做透明背景 / 抖动矩阵选择、不做 GIF 体积预估。

### 4.19.4 断言与四包

| 验证 | 结果 |
| --- | --- |
| `cargo test` | **124 / 124**（原 107 + 17：音量 11 / 分割与 GIF 6，另含 jobs 侧校验） |
| `p2-check` | **39 / 39**（原 30 + volume 3 / split 4 / gif 1 / 校验 1） |
| `npm run verify:formats` | **31 / 31**（原 27 + 4） |
| `npx tsc --noEmit` + `vite build` | 通过 |
| `cargo fmt --check` | 通过 |

| 变体 | 产物 | 字节 | SHA-256 |
| --- | --- | --- | --- |
| LGPL | MSI | 2 736 128 | `2faadd038b8b43faabcc2888d31d1bf0c5078269a872c7b5c3f8187aa61a8c1f` |
| LGPL | NSIS setup | 1 874 911 | `c54bb079372b2d0dacf9d4419e12213d8eb8d5dd43ee982d05408692867d7858` |
| GPL | MSI | 2 736 128 | `ef16c95112e3c0ebb626ae8013b31da05b0010f85e75c28cd44968f537be72a3` |
| GPL | NSIS setup | 1 872 453 | `7925bd8da190b734dfed8d2fb61a743d3afdd53595b9f6ae2ea08ce40fb59940` |

### 4.19.5 仍需人工开窗确认

* 视频面板「音量」分组（启用 / −20…20 dB / 静音）与「分割」分组（模式 / 分段文本）真实渲染；
* 分割多段入队后任务中心显示 `-partN` 与「（起s–止s）」，任一段失败可单独定位；
* 图片转 GIF 多选 ≥2 张时合并为一条任务、自然排序生效、>2000px 自动降 1280 的提示；
* 音频卡「音量启用时归一化禁用」的置灰与原因文案。

---

## 4.20 许可落定与版本发布规则（第十轮，`1.0.0-alpha*` 起点）

本轮不改功能，只把「许可」和「版本」两条工程规则定死，后续每个 alpha 都按这套规则发布。

### 4.20.1 应用代码 MIT（与 FFmpeg/Poppler 许可分开）

- 根目录新增 `LICENSE`（MIT），覆盖整个仓库（含所有子目录）；README 与 `LICENSING.md` 用两层表分开写：
  ① 应用代码 = MIT；② FFmpeg / Poppler = 各自的 LGPL / GPL 路线（且安装包不含这两个二进制）。
- 第三方许可文件（`licenses/`、`src-tauri/resources/licenses/`）保持原样，未改写。
- 明确「LGPL 版 / GPL 版」只描述**所接受的 FFmpeg 构建**，不是应用代码双许可。

### 4.20.2 版本单一来源与校验

- 唯一手改处：`package.json` 的 `version`；`npm run version:sync`（`build:lgpl|gpl` 开头自动执行）
  把它注入 `src-tauri/tauri.conf.json`、`src-tauri/Cargo.toml`、`package-lock.json`。
- 产物自证：`Transmutio.exe --version` 输出 `产品名 版本 (变体)`，版本取自 `CARGO_PKG_VERSION`，
  与关于页（`lib.rs` 的 package_info）同源。
- 规则（`scripts/version-rules.mjs`）：除第一段外每段 ≤ 10；alpha 为 `alpha<N>`，N 为正整数、
  无前导零、相对同基线已有 tag 单调递增；tag 必须等于 `v<version>`；可选 `--binary` 校验产物版本。

| 反例 | 结果 |
| --- | --- |
| `0.11.0` | 拦截：minor 段 11 超过上限 10 |
| `0.2.0-alpha0` | 拦截：alpha 序号必须为正整数且不带前导零 |
| `0.2.0-alpha01` | 拦截：同上（前导零） |
| tag `v1.0.0-alpha2` 对版本 `1.0.0-alpha1` | 拦截：tag 与版本不一致 |
| 产物报 `1.0.0-alpha0`、文件为 `1.0.0-alpha1` | 拦截：产物版本与版本文件不一致 |

- 自测：`npm run version:test` 18 项全过；`npm run version:check` 接进 CI frontend job（连同 `version:test`）。
- 版本基线：旧 `v0.1.0` 保持为历史不回改；此后从 `1.0.0-alpha1` 起，每块 +1，全部完工发 `1.0.0`。

### 4.20.3 本轮产物

四包按新版本重出（见本轮交付报告），文件名带 `1.0.0-alpha1`；Release 标 prerelease，
正文写明本阶段内容、已知限制、未签名、alpha 不建议日常使用。

---

## 4.21 发布路径收口：CI 三平台出包（第十一轮，alpha2）

本轮把「手工 Release」与「workflow 自动建 Release」两条路径合并为一条：**以 CI 产物为准，不再手工上传同名附件**。

### 4.21.1 `release.yml` 改动

- 触发：仍为 `push` tag `v*`；`concurrency` 串行、不取消（同 tag 重复推送不会并发覆盖）。
- `guard` 作业（ubuntu）：先跑 `version-rules-test`，再跑 `check-version --tag $GITHUB_REF_NAME`
  —— tag、版本文件、alpha 序号任一不一致就不进入构建。
- `build` 矩阵：`windows-latest` / `macos-latest` / `ubuntu-22.04`；每平台构建 LGPL + GPL 两个变体，
  Linux 预装 `webkit2gtk-4.1 / appindicator / rsvg / patchelf / libfuse2`。
- 每平台构建后再用产物 `--version` 与 tag 复核（Windows exe / macOS、Linux 可执行文件各读一次）。
- 附件：MSI、NSIS setup、DMG、deb、rpm、AppImage，外加上传 artifact 时各自生成的 `SHA256SUMS`。
- `release` 作业：下载三平台 artifact（`merge-multiple`）→ `release-checksums.mjs --flat --normalize-names`
  合并为一份 `SHA256SUMS`（文件名按 GitHub 资产名，空格→点）→ `softprops/action-gh-release` 创建/更新 Release。
  `prerelease` 由 tag 是否含 `-alpha` 决定，`body_path` 读取仓库内 `release-notes/<tag>.md`。
- **不改历史**：`v0.1.0` 与 `v1.0.0-alpha1` 的既有 Release 保留，后者为手工发布（当时 workflow 仍停用）。

### 4.21.2 文案与合规

- Release 正文固定包含：alpha 不建议日常使用、安装包**未签名**（SmartScreen / Gatekeeper 的后果、
  macOS 需手动放行）、安装包**不含 FFmpeg / Poppler** 与源码地址、两变体只是所接受的 FFmpeg 构建路线、
  仍未实现清单。
- 版本说明随代码走：`release-notes/v1.0.0-alpha2.md` 与 tag 一一对应，发布前由 CI 断言文件存在。

### 4.21.3 收口状态

- alpha2 由 CI 产出：Windows / macOS / Linux 三平台 + 两个变体；本机不再手工构建与上传。
- 本轮尚未验证：macOS / Linux 窗口启动与一次真实转码（下一轮单独做，属最小实跑验证）。

---

## 5. 应用级链路（`scripts/app-e2e.mjs`）与已知问题

该脚本启动真实 Tauri 应用，经 WebView2 CDP 调用命令完成全链路。
它在「禁止 GUI」约定生效**之前**运行过一次，报告 `samples/app-e2e-report.json`：
**13 PASS / 2 FAIL**。

两条 FAIL 及处理：

1. **两个同名任务输出同名** → 根因是 `enqueue` 逐个选取输出名、未预留同批任务。
   已改为「锁内取全部已用输出 + 选取 + 入队」，并有单元测试覆盖。
2. **取消后仍残留 `.ff-tmp-…part.mp4`** → 根因是 Windows 取消后句柄释放滞后。
   已加 `remove_temp_file`（12 次 ×100ms 重试）并替换所有清理点，单元测试覆盖发布逻辑。
3. **启动即闪退（`0xC0000409`）** → 根因是 `jobs::start_scheduler` 在 Tauri `setup`
   （同步上下文）里 `tokio::spawn`，panic 后 abort。修复与复验见 §4.5。
   这条 bug 恰好落在「无窗口验证」的盲区：所有检查程序都在自己的 Tokio 运行时里跑。

> 复验状态（按层级区分）：
> - **真实任务核心（无窗口）**：两项均已复验通过（§4.1，7/7）。取消路径另加了 3s 延迟兜底清理，
>   应对 Windows 句柄释放滞后；清空历史任务时也会顺手删除残留临时文件。
> - **应用级 UI 链路**：**未重跑**。重跑需要启动可见窗口，属于当前约定禁止的前台 GUI 操作。
>   因此「窗口内点击取消后零残留」只到核心层，未到 UI 层；不要把 §4.1 称作应用级 E2E。

界面证据（同一轮 E2E 采集，非新启动）：
`samples/app-running.png`（运行中任务中心 + 并发进度）、`samples/app-done.png`（全部完成）。
当时因虚拟屏 1024×768 / 200% 缩放导致窗口被裁，改用 CDP 全页截图（2000×1400）核对；
顶部工具条、6 分区、方法网格、Inspector、任务中心、状态栏均渲染正常。

---

## 6. 哪些是后台验证的，哪些没做

**后台验证（无窗口）**

- `npx tsc --noEmit`、`vite build`、`cargo test`（63/63）、`npm run verify:formats`（7/7）
- `scripts/verify-mvp.mjs` 全部转码用例（真实 ffmpeg，参数数组调用，日志重定向）
- `npm run verify:core`：真实任务核心的同名冲突、4 槽并发、取消清理、失败分类、重试保参、
  旋转、比例裁剪、响度归一化、任务日志（11/11）
- `npm run verify:ui`：引导页 UI 层 headless 验收（39/39，LGPL/nonfree/GPL 三场景，后端 mock）
- `npm run verify:setup`：真实 LGPL/GPL ffmpeg 的 inspect → 落位 → 两变体判定 → 拒绝分支 → 闭环转码（12/12）
- `npm run verify:flavor:lgpl|gpl`：变体策略 + 隔离 + 真实 libx264 转码 / 拒绝（各 7/7）
- `npm run build:lgpl|gpl` + `npm run check:bundle`：真实安装包与解包合规检查

**因需要前台而未做（如实标注）**

- 重跑应用级 E2E（会弹窗口）；仓库没有名为 `test:e2e` 的 npm script，`verify:app` 在启动可见窗口前主动退出，本轮未跑
- 在真实窗口内手动点击、拖拽、切换主题（引导页已用 headless 覆盖渲染与交互，见 §4.2a）
- macOS / Linux 构建与运行（无设备）
- Apple Silicon 真机：VideoToolbox 能力、10bit/B 帧、并发上限、冷启动
- 真实代码签名与公证（无证书）

---

## 7. 文件安全三条底线：逐条对照

| 底线 | 实现 | 验证 |
| --- | --- | --- |
| 不覆盖已存在文件 | `unique_output` 改名 + `hard_link` 发布失败即 `AlreadyExists` | 脚本冲突用例 PASS（mtime 未变）+ 单元测试 |
| 临时文件 + 原子替换，中断不留半成品 | `.ff-tmp-*.part.ext` → 成功后硬链接发布 → 删除临时；取消/失败清理 | 脚本取消用例 PASS；应用内取消**修复后待复验** |
| 删除源文件必须显式、默认关闭 | 参数与 UI 默认不删源；本版**未实现删除源文件** | 不适用（无删除路径，最安全） |

> 说明：删除源文件功能未实现，属于「默认关闭」的极限形态；转换后源文件 mtime 均未变化（已抽查）。

---

## 8. 许可（应用代码 MIT + FFmpeg/Poppler 各自路线）

- **应用代码：MIT**（根目录 `LICENSE`，覆盖整个仓库，含所有子目录）。
- 两个变体：`Transmutio`（默认，仅 LGPL 构建）与 `Transmutio GPL`（编译期固定
  `allowGpl=true`，只接受 GPL 构建）；`nonfree` 两包都拒绝。这两个名称只描述**所接受的
  FFmpeg 构建**，不代表应用代码采用 LGPL/GPL 双许可。
- 两变体都不随包分发 FFmpeg 二进制，由用户自行获取并按引导页落位；安装包内附
  许可证全文、第三方声明与源码获取说明（`licenses/`），`npm run check:bundle` 会解包断言。
- 本机开发用 gyan.dev GPL 构建**不随包分发**；分发前 `npm run check:ffmpeg-license` 会拦截。
- LGPL 构建通常无 libx264/libx265，且未必含 OpenH264 → 软件 H.264 缺口由 GPL 变体覆盖。
  变体矩阵与分发约束见 [LICENSING.md](LICENSING.md)。

---

## 9. 未验证 / 已知限制

1. **被占用文件**：占用模拟失败，错误提示与恢复路径未验证。
2. **应用内（窗口）取消的临时文件清理**：核心层已 7/7 复验；UI 层未重跑（见 §5）。
3. **FFmpeg 引导**：UI 层（headless + mock）39/39、真实落位 12/12 已通过；
   仍未验证：真实窗口内的点击（禁止 GUI）、「复制损坏后复核回滚」分支、
   macOS / Linux 上的落位与可执行权限。
4. **GPL 双变体**：构建层与逻辑层已通过；仍未验证：真实用户机器的共存/升级路径、
   安装包签名与公证、GPL 源码要约与「不随包分发二进制」措辞的**法律审查（需法律确认）**、
   Linux/macOS 安装包合规物。
5. **macOS / Apple Silicon**：全部硬件与打包结论按文档实现，待真机。
6. **Linux**：VAAPI 设备/权限/`hwupload` 已按文档接入，未在 Linux 编译运行；多 render 节点选择未实现。
7. **图片编码器**未逐项试编，缺 libwebp 时任务失败并给出原因（不会静默成功）。
8. **`hw-memory` / `invalid-params` 错误分类**：规则已写，未构造真实故障逐条复现。
9. **自动更新**：只有接口与 Null 实现，无端点与密钥；两个变体的更新通道字段已区分（`transmutio-lgpl-stable` / `transmutio-gpl-stable`）。
10. **签名**：脚本就绪，未实际签名/公证；无证书时脚本只打印流程并退出 0。
11. 默认包**不含 FFmpeg**；未放置合规构建时无法转换（有意设计，不伪造成功）。
12. **启动期验收**：加载层（manifest/入口点）与启动层（panic/闪退）都已在独立桌面复验
    （§4.4、§4.5，两变体存活 12s + 有窗口）。**可见窗口内的主界面渲染与操作仍未验收**
    （禁止前台 GUI）。教训：上一轮所有验证都是无窗口 + `#[tokio::main]` 驱动，
    恰好绕过了「Tauri `setup` 同步上下文调用 `tokio::spawn`」这条只在真实启动时走的路径。
13. **本轮新增能力**：旋转/裁剪/响度归一化都有真实 ffmpeg + ffprobe 产物断言，但**未在真实窗口内点选**，
    也未用手机竖拍素材验证「EXIF 方向 + 手动旋转」的叠加行为。
14. **响度归一化依赖 libebur128**：本机两个 ffmpeg 构建都含 `loudnorm`；缺失时任务会明确失败，
    未枚举所有第三方构建。
15. **硬件编码覆盖**：本机只实测到 AMF（`h264_amf` 真实用于编码）；NVENC / QSV / VAAPI
    **未在本机实测**，不能用「探测到」替代「真实编码成功」。失败回退软编已验证。
16. **任务日志打开按钮**：只验证了日志落盘内容与命令注册，未在窗口内点击 E2E。

---

## 10. 复验步骤（拿到对应环境后照做）

```powershell
# 1) 回归（无 GUI）
npm run build; npm run test:rust; npm run samples; npm run verify; npm run verify:core
npm run verify:ui      # 引导页 UI 层（headless，后端 mock）
npm run verify:setup   # 真实 LGPL/GPL 落位（FFMPEG_LGPL_BIN 或 samples/_dl 下需有 LGPL 构建）
npm run verify:flavor:lgpl; npm run verify:flavor:gpl
npm run build:lgpl; npm run build:gpl; npm run check:bundle
# 2) 应用内链路（需前台窗口，先取得确认）
$env:FORMATFORGE_ALLOW_GUI_E2E="1"; npm run verify:app
# 3) 许可拦截
npm run check:ffmpeg-license
```

macOS / Linux 真机项的取证命令见 [GPU-能力矩阵.md](GPU-能力矩阵.md) §7。
