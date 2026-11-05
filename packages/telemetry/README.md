# @formatforge/telemetry

日志与崩溃收集共享包，默认 **不上报任何数据**。

## 接口

- `createLogger({ scope, minLevel, sinks, console })` → `{ debug/info/warn/error, child }`
- `createBufferSink(capacity)` → 环形缓冲，`records()` / `toText()` 供"导出诊断日志"
- `createCrashReporter({ appVersion, platform, logger, onReport })` → `capture()` / `install()` / `reports()`

## 设计约束

- Sink 写入异常不得影响业务（内部已 try/catch）。
- 远端上报必须由宿主显式注入 Sink，默认只落内存。
- 本工程通过 `src/lib/telemetry.ts` 装配：控制台 + 内存缓冲 + Rust 侧写文件（导出诊断）。

## 未做

- 崩溃事件自动上报到服务端：未实现（无后端服务）。
- 日志文件滚动/清理策略：未实现。
