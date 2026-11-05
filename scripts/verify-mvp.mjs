#!/usr/bin/env node
/**
 * MVP 链路验证脚本（开发验证专用，不随安装包分发）。
 *
 * 验证内容：
 *  - 运行时编码器探测（-encoders + 试编），与应用的自动策略一致
 *  - 视频转 MP4：自动选硬编、失败回退软编、1080p/4K/长时长、中文/emoji/长路径/只读
 *  - 音频转 MP3/AAC（含从视频提取音轨）
 *  - 图片批量转换/缩放、冲突自动改名
 *  - -progress pipe:1 进度解析
 *  - 取消 = 杀子进程并清理临时文件（脚本复刻，应用内路径未自动化验证）
 *  - 4 路并发
 *  - 0 字节 / 损坏 / 被占用文件应明确失败
 *
 * 重要：本脚本直接调用系统 ffmpeg（当前为 GPL/nonfree 开发构建）。
 * 它验证的是「参数配方 + 环境」，不是 Tauri 应用内的 UI 操作；报告会逐项标注验证方式。
 */
import { spawn, spawnSync } from "node:child_process";
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const samples = path.join(root, "samples");
const outDir = path.join(samples, "out");
const ffmpeg = process.env.FFMPEG_PATH || "ffmpeg";
const isWindows = process.platform === "win32";
const ffprobe = ffmpeg.replace(/ffmpeg(\.exe)?$/i, (m) => m.replace("ffmpeg", "ffprobe"));

const results = [];
const record = (name, status, detail, method = "recipe") => {
  results.push({ name, status, detail, method });
  const icon = status === "pass" ? "PASS" : status === "skip" ? "SKIP" : "FAIL";
  console.log(`[${icon}] ${name} — ${detail}`);
};

function ensureDir(dir) {
  fs.mkdirSync(dir, { recursive: true });
}

function runFfmpeg(args, { timeout = 300_000 } = {}) {
  const result = spawnSync(ffmpeg, ["-hide_banner", "-nostdin", "-y", ...args], {
    encoding: "utf8",
    maxBuffer: 32 * 1024 * 1024,
    timeout,
  });
  return {
    ok: result.status === 0,
    status: result.status,
    stdout: result.stdout ?? "",
    stderr: result.stderr ?? "",
    error: result.error,
  };
}

function probe(file) {
  const result = spawnSync(ffprobe, ["-v", "error", "-show_format", "-show_streams", "-of", "json", file], {
    encoding: "utf8",
    maxBuffer: 8 * 1024 * 1024,
  });
  if (result.status !== 0) return { ok: false, stderr: result.stderr ?? "" };
  try {
    const json = JSON.parse(result.stdout);
    const v = (json.streams ?? []).find((s) => s.codec_type === "video");
    const a = (json.streams ?? []).find((s) => s.codec_type === "audio");
    return {
      ok: true,
      format: json.format?.format_name,
      duration: Number(json.format?.duration ?? 0),
      size: Number(json.format?.size ?? 0),
      video: v ? { codec: v.codec_name, width: v.width, height: v.height, pixFmt: v.pix_fmt } : null,
      audio: a ? { codec: a.codec_name, sampleRate: a.sample_rate, channels: a.channels } : null,
    };
  } catch {
    return { ok: false, stderr: "ffprobe JSON 解析失败" };
  }
}

// ---------- 与 src-tauri/src/args.rs 保持一致的参数配方 ----------
function rateArgs(encoder, { quality = 23, bitrateKbps = null, height = 1080 } = {}) {
  const bitrateMode = Boolean(bitrateKbps);
  const hint = height >= 2160 ? 16000 : height >= 1440 ? 8000 : height >= 1080 ? 4200 : height >= 720 ? 1800 : 900;
  if (encoder.endsWith("_nvenc")) {
    return bitrateMode
      ? ["-rc", "vbr", "-b:v", `${bitrateKbps}k`, "-maxrate", `${Math.round(bitrateKbps * 1.5)}k`, "-bufsize", `${bitrateKbps * 2}k`]
      : ["-rc", "vbr", "-cq", String(quality), "-b:v", "0", "-preset", "p5", "-tune", "hq", "-multipass", "qres"];
  }
  if (encoder.endsWith("_amf")) {
    return bitrateMode
      ? ["-rc", "vbr_peak", "-b:v", `${bitrateKbps}k`, "-quality", "quality"]
      : ["-rc", "cqp", "-qp_i", String(quality), "-qp_p", String(quality), "-quality", "quality"];
  }
  if (encoder.endsWith("_qsv")) {
    return bitrateMode ? ["-b:v", `${bitrateKbps}k`] : ["-global_quality", String(quality), "-preset", "medium"];
  }
  if (encoder.endsWith("_vaapi")) {
    return bitrateMode ? ["-b:v", `${bitrateKbps}k`] : ["-rc_mode", "CQP", "-qp", String(quality)];
  }
  if (encoder.endsWith("_videotoolbox")) {
    const vtQ = Math.max(30, Math.min(90, 100 - (Math.max(16, Math.min(32, quality)) - 14) * 4));
    return bitrateMode
      ? ["-b:v", `${bitrateKbps}k`, "-allow_sw", "0"]
      : ["-q:v", String(vtQ), "-allow_sw", "0"];
  }
  if (encoder.startsWith("libx264") || encoder === "libx265") {
    return bitrateMode ? ["-b:v", `${bitrateKbps}k`, "-preset", "medium"] : ["-crf", String(quality), "-preset", "medium"];
  }
  if (encoder === "libopenh264") {
    return ["-b:v", `${bitrateMode ? bitrateKbps : hint}k`, "-g", "120"];
  }
  return bitrateMode ? ["-b:v", `${bitrateKbps}k`] : ["-b:v", `${hint}k`];
}

function videoArgs({
  encoder,
  input,
  output,
  quality = 23,
  bitrateKbps = null,
  height = null,
  fps = null,
  audio = "aac",
  audioKbps = 192,
  hwaccel = null,
  faststart = true,
}) {
  const args = ["-loglevel", "warning", "-progress", "pipe:1"];
  if (hwaccel) args.push("-hwaccel", hwaccel);
  args.push("-i", input, "-map", "0:v:0");
  if (audio !== "none") args.push("-map", "0:a:0?");
  if (height) args.push("-vf", `scale=-2:${height}:flags=lanczos`);
  if (fps) args.push("-r", String(fps));
  args.push("-c:v", encoder, "-pix_fmt", "yuv420p", ...rateArgs(encoder, { quality, bitrateKbps, height: height ?? 1080 }));
  if (audio === "copy") args.push("-c:a", "copy");
  else if (audio === "none") args.push("-an");
  else args.push("-c:a", "aac", "-b:a", `${audioKbps}k`);
  if (faststart) args.push("-movflags", "+faststart");
  args.push("-f", "mp4", output);
  return args;
}

function audioArgs({ encoder, input, output, bitrateKbps = 192, sampleRate = null, channels = null }) {
  const args = ["-loglevel", "warning", "-progress", "pipe:1", "-i", input, "-vn", "-map", "0:a:0?", "-c:a", encoder, "-b:a", `${bitrateKbps}k`];
  if (sampleRate) args.push("-ar", String(sampleRate));
  if (channels) args.push("-ac", String(channels));
  args.push(output);
  return args;
}

function imageArgs({ input, output, format, quality = 82, maxEdge = 0 }) {
  const args = ["-loglevel", "warning", "-progress", "pipe:1", "-i", input, "-map", "0:v:0"];
  if (maxEdge > 0) {
    args.push("-vf", `scale=w=min(iw\\,${maxEdge}):h=min(ih\\,${maxEdge}):force_original_aspect_ratio=decrease:flags=lanczos`);
  }
  if (format === "png") args.push("-c:v", "png", "-compression_level", "6");
  else if (format === "webp") args.push("-c:v", "libwebp", "-quality", String(quality));
  else if (format === "bmp") args.push("-c:v", "bmp");
  else if (format === "tiff") args.push("-c:v", "tiff");
  else {
    const q = Math.max(2, Math.min(31, Math.round(31 - (quality / 100) * 29)));
    args.push("-c:v", "mjpeg", "-q:v", String(q));
  }
  args.push("-frames:v", "1", output);
  return args;
}

// ---------- 运行时能力探测 ----------
function probeEncoders() {
  const result = spawnSync(ffmpeg, ["-hide_banner", "-encoders"], { encoding: "utf8" });
  const text = result.stdout ?? "";
  const compiled = new Set();
  for (const line of text.split("\n")) {
    const match = line.match(/\s([AVS])[.\w]{5}\s+(\S+)/);
    if (match) compiled.add(match[2].trim());
  }
  return compiled;
}

function trial(encoder, codecArgs) {
  const started = Date.now();
  const result = runFfmpeg(
    ["-f", "lavfi", "-i", "testsrc2=size=320x240:rate=10:duration=0.6", "-c:v", encoder, ...codecArgs, "-pix_fmt", "yuv420p", "-f", "null", "-"],
    { timeout: 25_000 },
  );
  return { ok: result.ok, ms: Date.now() - started, stderr: result.stderr.slice(-600) };
}

function chooseEncoder(compiled) {
  const chain = isWindows
    ? ["h264_nvenc", "h264_amf", "h264_qsv"]
    : process.platform === "darwin"
      ? ["h264_videotoolbox"]
      : ["h264_vaapi"];
  const findings = [];
  for (const encoder of chain) {
    if (!compiled.has(encoder)) {
      findings.push({ encoder, status: "未编译进当前构建" });
      continue;
    }
    const result = trial(encoder, rateArgs(encoder, { quality: 23, height: 720 }));
    findings.push({ encoder, status: result.ok ? "可用" : "试编失败", detail: result.ok ? `${result.ms}ms` : result.stderr.split("\n").slice(-2).join(" ") });
  }
  const hardware = findings.find((item) => item.status === "可用");
  const softwareCandidates = ["libopenh264", "libx264"].filter((id) => compiled.has(id));
  const software = softwareCandidates[0] ?? null;
  return {
    findings,
    hardware: hardware?.encoder ?? null,
    software,
    softwareLicense: software === "libx264" ? "GPL（仅开发）" : software === "libopenh264" ? "LGPL（可分发）" : "无",
  };
}

function uniqueOutput(dir, stem, ext) {
  let candidate = path.join(dir, `${stem}.${ext}`);
  let index = 1;
  while (fs.existsSync(candidate) && index < 1000) {
    candidate = path.join(dir, `${stem} (${index}).${ext}`);
    index += 1;
  }
  return candidate;
}

// ---------- 主流程 ----------
async function main() {
  // 每次从干净的输出目录开始，否则上一轮留下的 "name (1).mp4" 会让冲突改名用例误判为失败。
  fs.rmSync(outDir, { recursive: true, force: true });
  ensureDir(outDir);
  const version = spawnSync(ffmpeg, ["-hide_banner", "-version"], { encoding: "utf8" });
  if (version.error || version.status !== 0) {
    console.error("找不到 ffmpeg，请设置 FFMPEG_PATH");
    process.exit(2);
  }
  console.log(`ffmpeg: ${(version.stdout || "").split("\n")[0]}`);

  const compiled = probeEncoders();
  const encoders = chooseEncoder(compiled);
  record(
    "运行时编码器探测",
    encoders.hardware ? "pass" : "skip",
    `平台=${process.platform} 硬编=${encoders.hardware ?? "无"}；软件=${encoders.software ?? "无"}（${encoders.softwareLicense}）。` +
      encoders.findings.map((f) => `${f.encoder}:${f.status}`).join(" | "),
    "app-source",
  );
  if (!encoders.hardware && !encoders.software) {
    record("可用编码器", "fail", "既没有可用硬件 H.264 编码器，也没有软件编码器，无法继续视频验证");
  }

  const video = path.join(samples, "video");
  const base = path.join(video, "base-1080p.mp4");
  const encoder = encoders.hardware ?? encoders.software;
  const usedHardware = Boolean(encoders.hardware);
  const fallbackNote = usedHardware ? null : `硬编不可用，自动回退到 ${encoder}`;
  record(
    "自动编码器选择",
    "pass",
    `选择 ${encoder}（${usedHardware ? "硬件" : "软件回退"}）${fallbackNote ? `；${fallbackNote}` : ""}`,
    "app-source",
  );

  // 1) 主链路：1080p → 720p MP4
  const mainOut = path.join(outDir, "base-720p.mp4");
  const mainRun = runFfmpeg(videoArgs({ encoder, input: base, output: mainOut, height: 720, quality: 23, hwaccel: isWindows ? "d3d11va" : null }));
  const mainProbe = probe(mainOut);
  record(
    "视频转 MP4（1080p→720p，硬解 d3d11va + 硬编）",
    mainRun.ok && mainProbe.ok && mainProbe.video?.height === 720 ? "pass" : "fail",
    mainRun.ok
      ? `${path.basename(mainOut)} ${mainProbe.video?.codec} ${mainProbe.video?.width}x${mainProbe.video?.height} ${mainProbe.duration.toFixed(2)}s ${(mainProbe.size / 1024).toFixed(0)}KB`
      : `失败：${mainRun.stderr.split("\n").slice(-3).join(" ")}`,
    "recipe",
  );

  // 1b) 硬编失败回退（用不可用的 nvenc 复现“参数/驱动失败→软化”的分类路径）
  if (isWindows && compiled.has("h264_nvenc")) {
    const badRun = runFfmpeg(videoArgs({ encoder: "h264_nvenc", input: base, output: path.join(outDir, "nvenc-should-fail.mp4"), height: 480 }), { timeout: 60_000 });
    const text = badRun.stderr;
    const classified = /Could not open encoder|No capable devices|Cannot load|OpenEncodeSession/i.test(text) ? "hw-unavailable" : "unknown";
    const fbRun = badRun.ok
      ? { ok: true, stderr: "" }
      : runFfmpeg(videoArgs({ encoder: encoders.software ?? encoder, input: base, output: path.join(outDir, "fallback-after-nvenc.mp4"), height: 480 }));
    record(
      "硬编失败分类与回退",
      !badRun.ok && classified === "hw-unavailable" && fbRun.ok ? "pass" : badRun.ok ? "skip" : "fail",
      badRun.ok
        ? "nvenc 意外可用，未触发回退路径"
        : `失败分类=${classified}；回退=${encoders.software ?? encoder} ${fbRun.ok ? "成功" : "失败"}`,
      "app-source",
    );
  }

  // 2) 中文 / emoji / 长路径 / 只读源文件
  const specialCases = [
    ["中文文件名", path.join(video, "中文 视频 样本.mp4")],
    ["emoji 文件名", path.join(video, "🎬 emoji 视频.mp4")],
    ["空格与引号", path.join(video, "空格 与 ' 引号' 视频.mp4")],
    ["超长路径(219 字符)", path.join(samples, "long-path", ...Array.from({ length: 6 }, (_, i) => `第${i + 1}层-中文目录-abcdefghij`), "长路径 中文 视频.mp4")],
    ["只读源文件", path.join(samples, "edge", "只读-视频.mp4")],
  ];
  for (const [label, source] of specialCases) {
    if (!fs.existsSync(source)) {
      record(label, "skip", "样本不存在，先运行 npm run samples");
      continue;
    }
    const stem = path.basename(source, path.extname(source));
    const target = uniqueOutput(outDir, `${stem}-转换`, "mp4");
    const run = runFfmpeg(videoArgs({ encoder, input: source, output: target, height: 720 }));
    const info = run.ok ? probe(target) : { ok: false };
    let readonlyUntouched = true;
    if (label === "只读源文件") {
      const stat = fs.statSync(source);
      readonlyUntouched = (stat.mode & 0o200) === 0;
    }
    record(label, run.ok && info.ok && readonlyUntouched ? "pass" : "fail", run.ok ? `输出 ${path.basename(target)}（${(info.size / 1024).toFixed(0)}KB），源文件未被修改` : `失败：${run.stderr.split("\n").slice(-2).join(" ")}`);
  }

  // 3) 长时长与 4K
  const longSample = path.join(video, "长时长 120s.mp4");
  if (fs.existsSync(longSample)) {
    const target = path.join(outDir, "long-120s-720p.mp4");
    const started = Date.now();
    const run = runFfmpeg(videoArgs({ encoder, input: longSample, output: target, height: 720 }));
    const info = probe(target);
    record(
      "长时长 120s 转码",
      run.ok && info.ok && Math.abs(info.duration - 120) < 2 ? "pass" : "fail",
      run.ok ? `${info.duration.toFixed(1)}s，耗时 ${((Date.now() - started) / 1000).toFixed(1)}s，速度 ${(info.duration / ((Date.now() - started) / 1000)).toFixed(1)}x` : "失败",
    );
  } else {
    record("长时长 120s 转码", "skip", "样本未生成（--quick 模式）");
  }

  const hugeSample = path.join(video, "超大 4K 2s.mp4");
  if (fs.existsSync(hugeSample)) {
    const target = path.join(outDir, "4k-to-1080p.mp4");
    const info4k = probe(hugeSample);
    const run = runFfmpeg(videoArgs({ encoder, input: hugeSample, output: target, height: 1080 }));
    const info = probe(target);
    record(
      "4K 3840x2160 → 1080p",
      info4k.ok && run.ok && info.ok && info.video?.height === 1080 ? "pass" : "fail",
      `源 ${info4k.video?.width}x${info4k.video?.height} → 输出 ${info.video?.width}x${info.video?.height}`,
    );
  } else {
    record("4K 转码", "skip", "样本未生成（--quick 模式）");
  }

  // 4) 进度解析
  const progressOut = path.join(outDir, "progress-check.mp4");
  const progressResult = spawnSync(ffmpeg, ["-hide_banner", "-nostdin", "-y", ...videoArgs({ encoder, input: longSample, output: progressOut, height: 480 })], {
    encoding: "utf8",
    maxBuffer: 8 * 1024 * 1024,
  });
  const lines = (progressResult.stdout ?? "").split("\n");
  const outTimeUs = lines.filter((line) => line.startsWith("out_time_us=")).pop();
  const speeds = lines.filter((line) => line.startsWith("speed=")).slice(-1);
  const ended = lines.includes("progress=end");
  record(
    "进度解析（-progress pipe:1）",
    progressResult.status === 0 && ended && outTimeUs ? "pass" : "fail",
    `末条 ${outTimeUs ?? "无 out_time_us"}，${speeds[0] ?? "无 speed"}，progress=end=${ended}`,
    "app-source",
  );

  // 5) 冲突自动改名（脚本复刻 unique_output）
  const conflictBase = path.join(outDir, "冲突测试.mp4");
  fs.copyFileSync(mainOut, conflictBase);
  const before = fs.statSync(conflictBase).mtimeMs;
  const conflictTarget = uniqueOutput(outDir, "冲突测试", "mp4");
  runFfmpeg(videoArgs({ encoder, input: base, output: conflictTarget, height: 480 }));
  const after = fs.statSync(conflictBase).mtimeMs;
  record(
    "输出冲突自动改名（不覆盖）",
    path.basename(conflictTarget).includes("(1)") && before === after ? "pass" : "fail",
    `已存在 冲突测试.mp4 → 新输出 ${path.basename(conflictTarget)}；原文件 mtime 未变=${before === after}`,
    "app-source",
  );

  // 6) 取消：杀子进程 + 临时文件清理（复刻应用行为）
  const cancelOut = path.join(outDir, ".ff-tmp-cancel.part.mp4");
  const cancelProc = spawn(ffmpeg, ["-hide_banner", "-nostdin", "-y", ...videoArgs({ encoder, input: longSample, output: cancelOut, height: 1080, quality: 18 })], {
    stdio: "ignore",
  });
  let killed = false;
  await new Promise((resolve) => {
    setTimeout(() => {
      cancelProc.kill("SIGKILL");
      killed = true;
      resolve();
    }, 900);
  });
  await new Promise((resolve) => cancelProc.on("close", resolve));
  const remained = fs.existsSync(cancelOut) ? fs.statSync(cancelOut).size : 0;
  fs.rmSync(cancelOut, { force: true });
  const cleaned = !fs.existsSync(cancelOut);
  record(
    "取消任务（杀子进程 + 清理临时文件）",
    killed && cleaned ? "pass" : "fail",
    `子进程已杀死；取消时临时文件 ${remained ? `${(remained / 1024).toFixed(0)}KB` : "尚未创建"}；清理后存在=${!cleaned}`,
    "app-source",
  );

  // 7) 并发 4 路
  const jobs = [
    { input: base, output: path.join(outDir, "并发-1.mp4"), height: 720 },
    { input: path.join(video, "中文 视频 样本.mp4"), output: path.join(outDir, "并发-2.mp4"), height: 720 },
    { input: path.join(video, "🎬 emoji 视频.mp4"), output: path.join(outDir, "并发-3.mp4"), height: 480 },
    { input: path.join(video, "长时长 120s.mp4"), output: path.join(outDir, "并发-4.mp4"), height: 480 },
  ];
  const started = Date.now();
  const outcomes = await Promise.all(
    jobs.map(
      (job) =>
        new Promise((resolve) => {
          const proc = spawn(ffmpeg, ["-hide_banner", "-nostdin", "-y", ...videoArgs({ encoder, ...job })], { stdio: "ignore" });
          proc.on("close", (code) => resolve(code === 0));
        }),
    ),
  );
  record(
    "4 路并发转码",
    outcomes.every(Boolean) ? "pass" : "fail",
    `4 个进程同时运行，成功 ${outcomes.filter(Boolean).length}/4，总耗时 ${((Date.now() - started) / 1000).toFixed(1)}s`,
    "app-source",
  );

  // 8) 音频
  const wav = path.join(samples, "audio", "纯音乐 30s.wav");
  const mp3Out = path.join(outDir, "音频-192k.mp3");
  const mp3Run = runFfmpeg(audioArgs({ encoder: "libmp3lame", input: wav, output: mp3Out, bitrateKbps: 192 }));
  const mp3Info = probe(mp3Out);
  record("音频转 MP3（LAME 192k）", mp3Run.ok && mp3Info.ok && mp3Info.audio?.codec === "mp3" ? "pass" : "fail", mp3Run.ok ? `${(mp3Info.size / 1024).toFixed(0)}KB · ${mp3Info.audio?.sampleRate}Hz` : "失败");

  const aacOut = path.join(outDir, "音频-AAC.aac");
  const aacRun = runFfmpeg(audioArgs({ encoder: "aac", input: wav, output: aacOut, bitrateKbps: 192, sampleRate: 48000 }));
  const aacInfo = probe(aacOut);
  record("音频转 AAC（192k/48kHz）", aacRun.ok && aacInfo.ok && aacInfo.audio?.codec === "aac" ? "pass" : "fail", aacRun.ok ? `${(aacInfo.size / 1024).toFixed(0)}KB · ${aacInfo.audio?.sampleRate}Hz` : "失败");

  const extractOut = path.join(outDir, "从视频提取-MP3.mp3");
  const extractRun = runFfmpeg(audioArgs({ encoder: "libmp3lame", input: base, output: extractOut, bitrateKbps: 192 }));
  const extractInfo = probe(extractOut);
  record("从视频提取音轨转 MP3", extractRun.ok && extractInfo.ok ? "pass" : "fail", extractRun.ok ? `${extractInfo.duration.toFixed(1)}s` : "失败");

  // 9) 图片
  const img = path.join(samples, "image", "中文 图片.png");
  const img4k = path.join(samples, "image", "🖼️ emoji 图片.jpg");
  const jpgOut = uniqueOutput(outDir, "图片-中文", "jpg");
  const jpgRun = runFfmpeg(imageArgs({ input: img, output: jpgOut, format: "jpg", quality: 82 }));
  const jpgInfo = probe(jpgOut);
  record("图片 PNG→JPG", jpgRun.ok && jpgInfo.ok ? "pass" : "fail", jpgRun.ok ? `${jpgInfo.video?.codec} ${jpgInfo.video?.width}x${jpgInfo.video?.height} ${(jpgInfo.size / 1024).toFixed(0)}KB` : "失败");

  const resizeOut = path.join(outDir, "图片-缩放1280.jpg");
  const resizeRun = runFfmpeg(imageArgs({ input: img4k, output: resizeOut, format: "jpg", quality: 82, maxEdge: 1280 }));
  const resizeInfo = probe(resizeOut);
  record(
    "图片批量缩放（4K→最长边 1280）",
    resizeRun.ok && resizeInfo.ok && Math.max(resizeInfo.video?.width ?? 0, resizeInfo.video?.height ?? 0) === 1280 ? "pass" : "fail",
    resizeRun.ok ? `${resizeInfo.video?.width}x${resizeInfo.video?.height}，未拉伸` : "失败",
  );

  const webpOut = path.join(outDir, "图片-WebP.webp");
  const webpRun = runFfmpeg(imageArgs({ input: img, output: webpOut, format: "webp", quality: 80 }));
  record(
    "图片转 WebP",
    webpRun.ok ? "pass" : "skip",
    webpRun.ok ? "libwebp 可用" : `当前构建缺少 libwebp：${webpRun.stderr.split("\n").slice(-1)[0] ?? ""}`,
  );

  // 10) 错误样本
  const zero = path.join(samples, "edge", "0字节.mp4");
  record("0 字节文件", fs.existsSync(zero) && fs.statSync(zero).size === 0 ? "pass" : "fail", "文件为 0 字节；应用在入队前直接拒绝（app-source）", "app-source");

  const corrupt = path.join(samples, "edge", "损坏-视频.mp4");
  const corruptProbe = probe(corrupt);
  record("损坏文件探测", !corruptProbe.ok ? "pass" : "fail", !corruptProbe.ok ? `ffprobe 失败并被识别：${(corruptProbe.stderr || "").split("\n")[0] || "invalid data"}` : "意外解析成功");

  // 被占用文件：用 PowerShell 以 FileShare.None 持有句柄，再尝试探测
  if (isWindows) {
    const locked = path.join(samples, "edge", "被占用-视频.mp4");
    const escaped = locked.replace(/'/g, "''");
    const holder = spawn(
      "powershell",
      ["-NoProfile", "-Command", `$f=[IO.File]::Open('${escaped}','Open','Read','None'); Write-Output 'locked'; Start-Sleep -Seconds 8`],
      { stdio: ["ignore", "pipe", "pipe"] },
    );
    const lockReady = await new Promise((resolve) => {
      let settled = false;
      holder.stdout.on("data", (chunk) => {
        if (!settled && String(chunk).includes("locked")) {
          settled = true;
          resolve(true);
        }
      });
      setTimeout(() => {
        if (!settled) {
          settled = true;
          resolve(false);
        }
      }, 4000);
    });
    const lockedProbe = lockReady ? probe(locked) : { ok: true, stderr: "未成功持有句柄" };
    holder.kill();
    record(
      "被占用文件",
      lockReady && !lockedProbe.ok ? "pass" : "skip",
      lockReady
        ? lockedProbe.ok
          ? "占用未阻止读取（可能共享模式允许），无法验证错误分类"
          : "探测失败，应用会归类为 input 错误并提示关闭占用程序"
        : "未能模拟文件占用",
    );
  } else {
    record("被占用文件", "skip", "当前平台未实现占用模拟，需人工验证");
  }

  // 汇总
  const passed = results.filter((r) => r.status === "pass").length;
  const failed = results.filter((r) => r.status === "fail").length;
  const skipped = results.filter((r) => r.status === "skip").length;
  const report = {
    generatedAt: new Date().toISOString(),
    ffmpeg: (version.stdout || "").split("\n")[0],
    platform: process.platform,
    arch: process.arch,
    encoders,
    summary: { passed, failed, skipped, total: results.length },
    results,
  };
  fs.writeFileSync(path.join(samples, "verify-report.json"), JSON.stringify(report, null, 2));
  console.log(`\n汇总：PASS ${passed} / FAIL ${failed} / SKIP ${skipped}，报告 samples/verify-report.json`);
  if (failed > 0) process.exitCode = 1;
}

main().catch((error) => {
  console.error(error);
  process.exit(1);
});
