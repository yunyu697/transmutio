import { createLogger, createBufferSink, createCrashReporter } from "@formatforge/telemetry";
import { api } from "./api";

export const logBuffer = createBufferSink(600);

export const logger = createLogger({
  scope: "transmutio",
  minLevel: "info",
  sinks: [logBuffer],
});

export const crashReporter = createCrashReporter({
  appVersion: "0.1.0",
  platform: `${navigator.platform}`,
  logger,
});

/** 导出诊断日志到应用数据目录，返回文件路径。 */
export async function exportDiagnostics(): Promise<string> {
  const header = [
    `Transmutio diagnostics · ${new Date().toISOString()}`,
    `userAgent: ${navigator.userAgent}`,
    `language: ${navigator.language}`,
  ].join("\n");
  const crashes = crashReporter
    .reports()
    .map((r) => `[crash ${new Date(r.ts).toISOString()}] ${r.message}\n${r.stack ?? ""}`)
    .join("\n");
  const text = `${header}\n\n== logs ==\n${logBuffer.toText()}\n\n== crashes ==\n${crashes || "(none)"}\n`;
  return api.saveDiagnostics(text);
}
