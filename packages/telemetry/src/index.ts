/**
 * @formatforge/telemetry
 * 日志与崩溃收集的最小实现：分级日志、可插拔 Sink、内存环形缓冲、崩溃钩子。
 * 不上传任何数据；远端上报由宿主注入 Sink 决定。
 */

export type LogLevel = "debug" | "info" | "warn" | "error";

export interface LogRecord {
  ts: number;
  level: LogLevel;
  scope: string;
  message: string;
  data?: unknown;
}

export interface LogSink {
  write(record: LogRecord): void;
  flush?(): Promise<void>;
}

export interface Logger {
  debug(message: string, data?: unknown): void;
  info(message: string, data?: unknown): void;
  warn(message: string, data?: unknown): void;
  error(message: string, data?: unknown): void;
  child(scope: string): Logger;
}

const LEVEL_ORDER: Record<LogLevel, number> = { debug: 10, info: 20, warn: 30, error: 40 };

export interface LoggerOptions {
  scope: string;
  /** 低于该级别的日志被丢弃。 */
  minLevel?: LogLevel;
  sinks?: LogSink[];
  /** 同时输出到 console（默认 true，便于开发）。 */
  console?: boolean;
}

export function createLogger(options: LoggerOptions): Logger {
  const minLevel = options.minLevel ?? "info";
  const sinks = options.sinks ?? [];
  const useConsole = options.console ?? true;

  const emit = (level: LogLevel, scope: string, message: string, data?: unknown) => {
    if (LEVEL_ORDER[level] < LEVEL_ORDER[minLevel]) return;
    const record: LogRecord = { ts: Date.now(), level, scope, message, data };
    if (useConsole) {
      const fn = level === "error" ? console.error : level === "warn" ? console.warn : console.log;
      fn(`[${scope}] ${message}`, data ?? "");
    }
    for (const sink of sinks) {
      try {
        sink.write(record);
      } catch {
        /* sink 异常不得影响业务 */
      }
    }
  };

  const make = (scope: string): Logger => ({
    debug: (m, d) => emit("debug", scope, m, d),
    info: (m, d) => emit("info", scope, m, d),
    warn: (m, d) => emit("warn", scope, m, d),
    error: (m, d) => emit("error", scope, m, d),
    child: (sub) => make(`${scope}:${sub}`),
  });

  return make(options.scope);
}

export interface BufferSink extends LogSink {
  records(): LogRecord[];
  toText(): string;
  clear(): void;
}

/** 环形缓冲 Sink：保留最近 N 条，供"导出诊断日志"使用。 */
export function createBufferSink(capacity = 500): BufferSink {
  let items: LogRecord[] = [];
  return {
    write(record) {
      items.push(record);
      if (items.length > capacity) items = items.slice(items.length - capacity);
    },
    records: () => items.slice(),
    toText: () =>
      items
        .map((r) => {
          const time = new Date(r.ts).toISOString();
          const extra = r.data === undefined ? "" : ` ${safeStringify(r.data)}`;
          return `${time} ${r.level.toUpperCase().padEnd(5)} [${r.scope}] ${r.message}${extra}`;
        })
        .join("\n"),
    clear() {
      items = [];
    },
  };
}

function safeStringify(value: unknown): string {
  try {
    return typeof value === "string" ? value : JSON.stringify(value);
  } catch {
    return String(value);
  }
}

export interface CrashReport {
  ts: number;
  message: string;
  stack?: string;
  context?: Record<string, unknown>;
  appVersion: string;
  platform: string;
}

export interface CrashReporter {
  capture(error: unknown, context?: Record<string, unknown>): CrashReport;
  /** 安装 window.onerror / unhandledrejection；返回卸载函数。 */
  install(): () => void;
  reports(): CrashReport[];
}

export interface CrashReporterOptions {
  appVersion: string;
  platform: string;
  logger: Logger;
  /** 自定义上报；默认只留在内存缓冲。 */
  onReport?: (report: CrashReport) => void;
}

export function createCrashReporter(options: CrashReporterOptions): CrashReporter {
  const buffer: CrashReport[] = [];

  return {
    capture(error, context) {
      const report: CrashReport = {
        ts: Date.now(),
        message: error instanceof Error ? error.message : safeStringify(error),
        stack: error instanceof Error ? error.stack : undefined,
        context,
        appVersion: options.appVersion,
        platform: options.platform,
      };
      buffer.push(report);
      if (buffer.length > 100) buffer.shift();
      options.logger.error("crash.captured", { ...report });
      try {
        options.onReport?.(report);
      } catch {
        /* ignore */
      }
      return report;
    },
    install() {
      const onError = (event: ErrorEvent) => this.capture(event.error ?? event.message, { kind: "window.onerror" });
      const onRejection = (event: PromiseRejectionEvent) =>
        this.capture(event.reason, { kind: "unhandledrejection" });
      window.addEventListener("error", onError);
      window.addEventListener("unhandledrejection", onRejection);
      return () => {
        window.removeEventListener("error", onError);
        window.removeEventListener("unhandledrejection", onRejection);
      };
    },
    reports: () => buffer.slice(),
  };
}
