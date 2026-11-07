# 项目约定（Contributing）

本仓库是 FormatForge（格式工坊）独立工程，不与其它项目共享代码与资源：
不要跨目录读取、复制或写入其它工程的代码与资源。共享基础层（设计令牌、i18n、设置存储、
任务队列、日志/崩溃上报、自动更新）以本仓库 `packages/*` 为唯一来源，接口保持可替换，
待共享包定稿后再做适配层对接。

## 1. 执行方式：一律后台、无 GUI

**硬性要求：所有构建、测试、转码验证、截图验证都在后台执行，输出重定向到日志后再读取。**

```powershell
# 正确：后台执行 + 日志
cargo test 2>&1 | Tee-Object -FilePath ..\cargo-test.log
npm run build 2>&1 | Tee-Object -FilePath build.log
node scripts/verify-mvp.mjs 2>&1 | Tee-Object -FilePath verify.log
# 真实任务核心（队列/调度/取消），无窗口：
cargo run --manifest-path src-tauri/Cargo.toml --bin headless-check -- samples/core-e2e-report.json 2>&1 | Tee-Object -FilePath headless.log
# 真实落位链路（需 LGPL 构建）：
$env:FFMPEG_LGPL_BIN="...\bin\ffmpeg.exe"
cargo run --manifest-path src-tauri/Cargo.toml --bin setup-check -- samples/setup-install-report.json 2>&1 | Tee-Object -FilePath setup-check.log
# 引导页 UI 层（headless Edge，无窗口）：
node scripts/setup-ui-check.mjs 2>&1 | Tee-Object -FilePath setup-ui.log
```

- 不启动带窗口的进程：`tauri dev`、直接运行 `formatforge.exe`、会抢占桌面的浏览器等。
- 需要看界面时，用无头渲染 / CDP 截图，读图确认，不弹窗口、不反复抢焦点。
- GPU/硬件编码验证不要为了「看一眼」启动播放器或预览窗口；用 `ffprobe` 校验输出、
  查看编码日志与 fps/速度统计，判断是否真的走了硬编。
- 必须前台运行时：先说明原因、预计耗时与资源占用，等用户单独确认后再执行。
  `scripts/app-e2e.mjs` 会启动可见窗口，默认拒绝执行，必须显式设置
  `FORMATFORGE_ALLOW_GUI_E2E=1` 并取得确认。

## 2. 用户文件安全三条底线

1. **绝不覆盖已存在文件**：输出同名时自动改名为 `文件名 (1).ext`、`文件名 (2).ext`；
   入队时同时预留其他任务的输出名；发布阶段使用硬链接原子创建，目标存在则失败并改名重试。
2. **临时文件 + 原子替换**：先写 `.ff-tmp-<id>.part.<ext>`，成功后才发布；取消/失败会清理
   临时文件（Windows 文件句柄释放有延迟，删除带重试）。中断不留下半成品。
3. **删除源文件默认关闭**：MVP 不提供删除源文件选项；任何后续实现都必须是显式选项且默认关闭。

## 3. 许可策略

- 默认只允许 LGPL：不使用 libx264 / libx265 / fdk-aac / nonfree 组件。
- `allowGplEncoders` 设置默认 `false`，仅供本地开发验证；发布构建必须保持关闭。
- 新增编码器时同时声明 `license`，并在 `probe.rs` 的能力表中按许可过滤。
- 分发前运行 `npm run check:ffmpeg-license`，GPL/nonfree 二进制不得进入安装包。

## 4. 验证要求

- 三条主线（视频转 MP4、音频 MP3/AAC、图片转换/缩放）必须用真实样本验证。
- 真实样本由 `npm run samples` 生成，包含中文/emoji 文件名、长路径、只读、被占用、
  0 字节、损坏、长时长、4K 等边界情况。
- 未实际跑过的项目不得写进验证说明；平台未实测（如 Apple Silicon）必须标注
  「按文档实现、待真机验证」。
- 参数配方改动后至少运行：`npm run test:rust`、`npm run verify`、`npm run verify:core`、`npm run build`。
  引导/落位改动另加 `npm run verify:ui` 与 `npm run verify:setup`
  （后者需 `FFMPEG_LGPL_BIN` 指向 LGPL 构建，或把 LGPL 包解压到 `samples/_dl/`）。
- 验证层级必须写清楚：`npm run verify` 是**配方层**（独立脚本 + 真实 ffmpeg），
  `npm run verify:core` 是**真实任务核心 + 真实文件系统（无窗口）**，
  `npm run verify:ui` 是**UI 层（headless 浏览器 + mock 后端）**，
  `npm run verify:setup` 是**真实落位链路（真实 LGPL 二进制 + 真实文件系统）**，
  `npm run verify:app` 才是应用级，但会启动窗口，默认禁止。各层不得混称。

## 5. 代码与目录

- Rust 后端：`src-tauri/src/{tools,probe,args,jobs,setup,lib}.rs`，命令统一在 `lib.rs` 暴露。
- 任务核心 `jobs.rs` 不直接依赖 Tauri：入口是 `jobs::Ctx`（共享状态 + 事件回调）；
  `headless.rs` 与 `bin/headless-check.rs` 复用同一份核心做无窗口验证，不要另写替身管线。
- 引导落位 `setup.rs` 的核心是 `install_blocking(&app_data_dir, …)`，Tauri 命令只做
  app_data 解析与状态写回；`setup_check.rs` 与 `bin/setup-check.rs` 复用同一份实现。
- 前端：`src/`；共享包：`packages/*`，每个包独立 `README.md` 与公开接口。
- 业务层不直接读写设计令牌/i18n/设置的底层实现，通过包接口调用。
- 未实现的方法保留入口并明确标注「未实现」，不做看起来能用的空壳。
