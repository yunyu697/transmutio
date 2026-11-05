#!/usr/bin/env node
/**
 * 应用级端到端验证：直接启动真实 Tauri 应用（debug 构建），通过 WebView2 调试端口
 * 调用应用自己的 Tauri 命令，走完「探测 → 入队 → 4 槽并发 → 进度 → 取消 → 重试 → 原子输出」全链路。
 *
 * 这不是脚本复刻：任务调度、子进程管理、进度事件、临时文件与改名都由应用 Rust 侧完成。
 * 前置：已执行 `npx tauri build --debug --no-bundle`，且本机 ffmpeg 可用（设置 FFMPEG_PATH 或 PATH）。
 *
 * 用法（会启动可见的应用窗口，按项目约定属于「前台操作」，必须显式确认）：
 *   FORMATFORGE_ALLOW_GUI_E2E=1 node scripts/app-e2e.mjs
 *
 * 日常验证请使用 `npm run verify`（不启动任何 GUI）与 `npm run test:rust`。
 */
import { spawn, spawnSync } from "node:child_process";
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

if (process.env.FORMATFORGE_ALLOW_GUI_E2E !== "1") {
  console.log("本脚本会启动带窗口的 Tauri 应用，按项目约定默认不执行。");
  console.log("确需运行（会短暂占用桌面焦点）时设置 FORMATFORGE_ALLOW_GUI_E2E=1 后重试。");
  process.exit(0);
}

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const samples = path.join(root, "samples");
const outDir = path.join(samples, "out-app");
const exe = process.env.APP_EXE || path.join(root, "src-tauri", "target", "debug", "formatforge.exe");
const ffmpeg = process.env.FFMPEG_PATH || "ffmpeg";
const ffprobe = ffmpeg.replace(/ffmpeg(\.exe)?$/i, (m) => m.replace("ffmpeg", "ffprobe"));
const port = Number(process.env.CDP_PORT || 9333);

const results = [];
const record = (name, status, detail) => {
  results.push({ name, status, detail });
  console.log(`[${status.toUpperCase()}] ${name} — ${detail}`);
};

function ensureDir(dir) {
  fs.mkdirSync(dir, { recursive: true });
}

function probe(file) {
  const result = spawnSync(ffprobe, ["-v", "error", "-show_format", "-show_streams", "-of", "json", file], {
    encoding: "utf8",
    maxBuffer: 8 * 1024 * 1024,
  });
  if (result.status !== 0) return { ok: false };
  try {
    const json = JSON.parse(result.stdout);
    const v = (json.streams ?? []).find((s) => s.codec_type === "video");
    const a = (json.streams ?? []).find((s) => s.codec_type === "audio");
    return {
      ok: true,
      duration: Number(json.format?.duration ?? 0),
      size: Number(json.format?.size ?? 0),
      video: v ? { codec: v.codec_name, width: v.width, height: v.height } : null,
      audio: a ? { codec: a.codec_name, sampleRate: a.sample_rate } : null,
    };
  } catch {
    return { ok: false };
  }
}

async function connect() {
  for (let i = 0; i < 60; i += 1) {
    try {
      const targets = await (await fetch(`http://127.0.0.1:${port}/json/list`)).json();
      const page = targets.find((t) => t.type === "page");
      if (page) {
        const ws = new WebSocket(page.webSocketDebuggerUrl);
        await new Promise((resolve, reject) => {
          ws.onopen = resolve;
          ws.onerror = reject;
        });
        let seq = 0;
        const pending = new Map();
        ws.onmessage = (event) => {
          const msg = JSON.parse(event.data);
          if (msg.id && pending.has(msg.id)) {
            pending.get(msg.id)(msg);
            pending.delete(msg.id);
          }
        };
        const send = (method, params = {}) =>
          new Promise((resolve) => {
            const id = ++seq;
            pending.set(id, resolve);
            ws.send(JSON.stringify({ id, method, params }));
          });
        return { ws, send };
      }
    } catch {
      // 应用还没起来
    }
    await new Promise((resolve) => setTimeout(resolve, 500));
  }
  throw new Error("无法连接 WebView2 调试端口（9222/9333），请确认应用已启动");
}

async function evaluate(send, expression) {
  const response = await send("Runtime.evaluate", { expression, awaitPromise: true, returnByValue: true });
  if (response.result?.exceptionDetails) {
    throw new Error(response.result.exceptionDetails.exception?.description ?? "Runtime.evaluate 失败");
  }
  return response.result?.result?.value;
}

const invoke = (send, command, args = {}) =>
  evaluate(send, `window.__TAURI_INTERNALS__.invoke(${JSON.stringify(command)}, ${JSON.stringify(args)})`);

async function capture(send, file) {
  const shot = await send("Page.captureScreenshot", { format: "png", captureBeyondViewport: true, fromSurface: true });
  if (shot.result?.data) {
    fs.writeFileSync(file, Buffer.from(shot.result.data, "base64"));
    return true;
  }
  return false;
}

const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms));

const VIDEO_PARAMS = {
  codec: "h264",
  encoder: "auto",
  rateMode: "quality",
  quality: 23,
  bitrateKbps: 8000,
  resolution: "720",
  fps: "source",
  audioMode: "aac",
  audioBitrateKbps: 192,
  hwDecode: true,
  faststart: true,
};

const AUDIO_PARAMS = { format: "mp3", bitrateKbps: 192, sampleRate: "source", channels: "source" };
const AAC_PARAMS = { ...AUDIO_PARAMS, format: "aac" };
const IMAGE_PARAMS = { format: "jpg", quality: 82, maxEdge: 0 };

async function waitForAll(send, timeoutMs = 300_000) {
  const started = Date.now();
  let jobs = [];
  while (Date.now() - started < timeoutMs) {
    jobs = await invoke(send, "list_jobs");
    if (!jobs.some((job) => job.status === "queued" || job.status === "running")) return jobs;
    await sleep(700);
  }
  return jobs;
}

async function main() {
  ensureDir(outDir);
  if (!fs.existsSync(exe)) {
    console.error(`找不到应用二进制：${exe}\n请先执行 npx tauri build --debug --no-bundle`);
    process.exit(2);
  }
  const app = spawn(exe, [], {
    env: {
      ...process.env,
      WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS: `--remote-debugging-port=${port} --force-device-scale-factor=1`,
    },
    stdio: ["ignore", "pipe", "pipe"],
  });
  const appLog = [];
  app.stdout.on("data", (chunk) => appLog.push(String(chunk)));
  app.stderr.on("data", (chunk) => appLog.push(String(chunk)));

  try {
    const { ws, send } = await connect();

    // 等应用完成启动探测
    let ready = false;
    for (let i = 0; i < 40; i += 1) {
      try {
        await invoke(send, "list_jobs");
        ready = true;
        break;
      } catch {
        await sleep(500);
      }
    }
    if (!ready) throw new Error("应用命令未就绪");
    record("应用启动与命令通道", "pass", `Tauri 窗口已启动，调试端口 ${port} 可访问`);

    const tools = await invoke(send, "resolve_tools", { customFfmpeg: null });
    record(
      "工具解析",
      tools?.ffmpeg ? "pass" : "fail",
      `${tools?.ffmpeg?.path ?? "未找到"}（source=${tools?.ffmpeg?.source}，version=${tools?.ffmpegVersion}，GPL=${tools?.build?.gpl}）`,
    );

    const caps = await invoke(send, "probe_capabilities", { customFfmpeg: null, allowGpl: false, force: false });
    const hw = caps.videoEncoders.filter((e) => e.hardware && e.available && e.allowed);
    record(
      "运行时能力表（应用内缓存）",
      caps ? "pass" : "fail",
      `缓存命中=${caps?.cacheHit}，指纹=${caps?.fingerprint}，可用硬编=${hw.map((e) => e.id).join(",") || "无"}，软件H264=${caps?.softwareH264Available}`,
    );

    await invoke(send, "set_job_concurrency", { value: 4 });
    record("设置并发槽", "pass", "set_job_concurrency(4) 成功");

    const videoDir = path.join(samples, "video");
    const specs = [
      { kind: "video-mp4", input: path.join(videoDir, "base-1080p.mp4"), outputDir: outDir, outputStem: "ui-base", params: VIDEO_PARAMS },
      { kind: "video-mp4", input: path.join(videoDir, "中文 视频 样本.mp4"), outputDir: outDir, outputStem: "ui-中文", params: { ...VIDEO_PARAMS, resolution: "480" } },
      { kind: "video-mp4", input: path.join(videoDir, "🎬 emoji 视频.mp4"), outputDir: outDir, outputStem: "ui-emoji", params: { ...VIDEO_PARAMS, resolution: "480" } },
      { kind: "video-mp4", input: path.join(videoDir, "长时长 120s.mp4"), outputDir: outDir, outputStem: "ui-long", params: { ...VIDEO_PARAMS, resolution: "480" } },
      { kind: "video-mp4", input: path.join(videoDir, "base-1080p.mp4"), outputDir: outDir, outputStem: "ui-base", params: VIDEO_PARAMS },
      { kind: "audio-mp3", input: path.join(samples, "audio", "纯音乐 30s.wav"), outputDir: outDir, outputStem: "ui-audio", params: AUDIO_PARAMS },
      { kind: "audio-aac", input: path.join(samples, "audio", "纯音乐 30s.wav"), outputDir: outDir, outputStem: "ui-aac", params: AAC_PARAMS },
      { kind: "image", input: path.join(samples, "image", "中文 图片.png"), outputDir: outDir, outputStem: "ui-image", params: IMAGE_PARAMS },
      { kind: "image", input: path.join(samples, "image", "🖼️ emoji 图片.jpg"), outputDir: outDir, outputStem: "ui-resize", params: { ...IMAGE_PARAMS, maxEdge: 1280 } },
    ];
    const created = await invoke(send, "enqueue_jobs", { specs });
    record("入队 9 个任务（含 2 个同名冲突）", created?.length === 9 ? "pass" : "fail", `返回 ${created?.length} 个任务；输出目录 ${outDir}`);
    const conflict = created.filter((job) => job.inputName === "base-1080p.mp4");
    record(
      "同名输出冲突改名",
      conflict.length === 2 && conflict[0].output !== conflict[1].output && conflict[1].output.includes("(1)") ? "pass" : "fail",
      conflict.map((job) => path.basename(job.output)).join(" / "),
    );

    await sleep(1800);
    await capture(send, path.join(samples, "app-running.png"));
    record("运行中截图", "pass", "samples/app-running.png（任务中心显示并发与进度）");

    const jobs = await waitForAll(send);
    const terminal = jobs.filter((job) => job.status === "done" || job.status === "failed");
    const done = jobs.filter((job) => job.status === "done");
    record(
      "4 槽并发完成全部任务",
      done.length === 9 ? "pass" : "fail",
      `完成 ${done.length}/9；状态=${terminal.map((job) => `${path.basename(job.output)}:${job.status}`).join(", ")}`,
    );

    for (const job of jobs) {
      if (job.status === "done") {
        const info = probe(job.output);
        const ok = info.ok && fs.existsSync(job.output) && fs.statSync(job.output).size > 0;
        if (!ok || job.fallbackNote) {
          record(
            `输出校验 ${path.basename(job.output)}`,
            ok ? "pass" : "fail",
            ok ? `${info.video ? `${info.video.width}x${info.video.height}` : info.audio?.codec} · ${(info.size / 1024).toFixed(0)}KB · 编码器=${job.encoderUsed}${job.fallbackNote ? ` · 回退=${job.fallbackNote}` : ""}` : "文件缺失或无法解析",
          );
        }
      } else {
        record(`任务状态 ${job.inputName}`, "fail", `${job.status} ${job.errorKind ?? ""} ${job.message ?? ""}`);
      }
    }
    const withEncoder = jobs.find((job) => job.kind === "video-mp4" && job.status === "done");
    record(
      "视频任务实际编码器",
      withEncoder ? "pass" : "fail",
      withEncoder ? `${withEncoder.encoderUsed}${withEncoder.fallbackNote ? `（${withEncoder.fallbackNote}）` : ""}，进度最终 ${Math.round(withEncoder.progress * 100)}%` : "无成功的视频任务",
    );

    const tempLeftovers = fs.readdirSync(outDir).filter((name) => name.startsWith(".ff-tmp-"));
    record("临时文件清理（成功后）", tempLeftovers.length === 0 ? "pass" : "fail", tempLeftovers.join(", ") || "没有 .ff-tmp- 残留");

    await capture(send, path.join(samples, "app-done.png"));

    // 取消：长任务运行 1.2s 后杀掉
    const cancelSpec = [
      { kind: "video-mp4", input: path.join(videoDir, "长时长 120s.mp4"), outputDir: outDir, outputStem: "ui-cancel", params: { ...VIDEO_PARAMS, resolution: "1080", quality: 18 } },
    ];
    const [cancelJob] = await invoke(send, "enqueue_jobs", { specs: cancelSpec });
    await sleep(1200);
    await invoke(send, "cancel_job", { id: cancelJob.id });
    await sleep(1200);
    const cancelled = (await invoke(send, "list_jobs")).find((job) => job.id === cancelJob.id);
    const leftoversAfterCancel = fs.readdirSync(outDir).filter((name) => name.startsWith(".ff-tmp-"));
    record(
      "取消任务（杀子进程）",
      cancelled?.status === "cancelled" && leftoversAfterCancel.length === 0 ? "pass" : "fail",
      `状态=${cancelled?.status}，临时文件残留=${leftoversAfterCancel.join(",") || "无"}，输出是否被误创建=${fs.existsSync(cancelJob.output)}`,
    );

    // 损坏文件：失败 + 重试
    const corruptPath = path.join(samples, "edge", "损坏-视频.mp4");
    const [corruptJob] = await invoke(send, "enqueue_jobs", {
      specs: [{ kind: "video-mp4", input: corruptPath, outputDir: outDir, outputStem: "ui-corrupt", params: VIDEO_PARAMS }],
    });
    await sleep(600);
    let corruptState = await waitForAll(send);
    corruptState = corruptState.find((job) => job.id === corruptJob.id);
    record(
      "损坏文件失败分类",
      corruptState?.status === "failed" ? "pass" : "fail",
      `status=${corruptState?.status} errorKind=${corruptState?.errorKind} message=${(corruptState?.message ?? "").slice(0, 80)}`,
    );
    await invoke(send, "retry_job", { id: corruptJob.id });
    await sleep(800);
    const retried = (await waitForAll(send)).find((job) => job.id === corruptJob.id);
    record(
      "失败任务重试",
      retried?.attempts === 1 && retried?.status === "failed" ? "pass" : "fail",
      `attempts=${retried?.attempts} status=${retried?.status}`,
    );

    // 0 字节：入队前拒绝
    let zeroRejected = false;
    let zeroMessage = "";
    const zeroEnvelope = await evaluate(
      send,
      `window.__TAURI_INTERNALS__.invoke("enqueue_jobs", ${JSON.stringify({
        specs: [{ kind: "video-mp4", input: path.join(samples, "edge", "0字节.mp4"), outputDir: outDir, params: VIDEO_PARAMS }],
      })}).then(() => JSON.stringify({ ok: true })).catch((e) => JSON.stringify({ ok: false, error: String(e) }))`,
    );
    const zeroParsed = JSON.parse(zeroEnvelope);
    zeroRejected = !zeroParsed.ok;
    zeroMessage = zeroParsed.error ?? "";
    record("0 字节文件拒绝入队", zeroRejected ? "pass" : "fail", zeroMessage.slice(0, 120));

    const finished = (await invoke(send, "list_jobs")).filter((job) => job.status !== "running" && job.status !== "queued");
    await invoke(send, "remove_finished_jobs");
    const afterClear = await invoke(send, "list_jobs");
    record("清空已完成任务", afterClear.length === 0 ? "pass" : "fail", `清理前 ${finished.length} 条，清理后 ${afterClear.length} 条`);

    // 汇总
    const passed = results.filter((r) => r.status === "pass").length;
    const failed = results.filter((r) => r.status === "fail").length;
    const report = {
      generatedAt: new Date().toISOString(),
      app: exe,
      platform: process.platform,
      arch: process.arch,
      ffmpeg: tools?.ffmpegVersion,
      buildFlags: tools?.build,
      capabilities: { cacheHit: caps?.cacheHit, fingerprint: caps?.fingerprint, hardware: hw.map((e) => e.id) },
      summary: { passed, failed, total: results.length },
      results,
    };
    fs.writeFileSync(path.join(samples, "app-e2e-report.json"), JSON.stringify(report, null, 2));
    console.log(`\n应用级端到端：PASS ${passed} / FAIL ${failed}，报告 samples/app-e2e-report.json`);
    if (failed > 0) process.exitCode = 1;
    ws.close();
  } finally {
    app.kill();
    await new Promise((resolve) => setTimeout(resolve, 500));
    if (app.exitCode === null) app.kill("SIGKILL");
    if (appLog.some((line) => /panic|error/i.test(line))) {
      fs.writeFileSync(path.join(samples, "app-e2e-stderr.log"), appLog.join(""));
    }
  }
}

main().catch((error) => {
  console.error(error);
  process.exit(1);
});
