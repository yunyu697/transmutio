#!/usr/bin/env node
/**
 * 生成真实样本库（开发/验证专用）。
 *
 * 注意：本脚本调用系统 PATH 中的 ffmpeg 生成样本，只为验证转码链路。
 * 系统 ffmpeg 若是 GPL/nonfree 构建，仅限本地开发，绝不能随安装包分发。
 *
 * 用法：
 *   node scripts/make-samples.mjs            # 生成全部样本（含 4K/120s 大样本）
 *   node scripts/make-samples.mjs --quick    # 跳过耗时的 4K 与长时长样本
 *   FFMPEG_PATH=/path/to/ffmpeg node scripts/make-samples.mjs
 */
import { spawnSync } from "node:child_process";
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const samples = path.join(root, "samples");
const ffmpeg = process.env.FFMPEG_PATH || "ffmpeg";
const quick = process.argv.includes("--quick");

function log(message) {
  console.log(`[samples] ${message}`);
}

function run(args, label) {
  const result = spawnSync(ffmpeg, ["-hide_banner", "-nostdin", "-y", ...args], {
    encoding: "utf8",
    maxBuffer: 16 * 1024 * 1024,
  });
  if (result.error) {
    throw new Error(`无法执行 ffmpeg（${label}）：${result.error.message}\n请设置 FFMPEG_PATH 或安装 ffmpeg`);
  }
  if (result.status !== 0) {
    throw new Error(`ffmpeg 失败（${label}）：\n${result.stderr?.slice(-1200) ?? ""}`);
  }
  log(`已生成 ${label}`);
}

function ensureDir(dir) {
  fs.mkdirSync(dir, { recursive: true });
}

function copyTo(source, target) {
  ensureDir(path.dirname(target));
  fs.copyFileSync(source, target);
  log(`已复制 ${path.relative(root, target)}`);
}

function probeInfo(file) {
  const ffprobe = ffmpeg.replace(/ffmpeg(\.exe)?$/i, (m) => m.replace("ffmpeg", "ffprobe"));
  const result = spawnSync(ffprobe, ["-v", "error", "-show_format", "-show_streams", "-of", "json", file], {
    encoding: "utf8",
    maxBuffer: 4 * 1024 * 1024,
  });
  if (result.status !== 0) return null;
  try {
    const json = JSON.parse(result.stdout);
    return { format: json.format?.format_name, streams: json.streams?.length ?? 0 };
  } catch {
    return null;
  }
}

function main() {
  const version = spawnSync(ffmpeg, ["-hide_banner", "-version"], { encoding: "utf8" });
  if (version.error || version.status !== 0) {
    console.error(`找不到可用的 ffmpeg（${ffmpeg}）。请设置 FFMPEG_PATH 或把 ffmpeg 加入 PATH。`);
    process.exit(2);
  }
  log(`使用 ffmpeg：${(version.stdout || "").split("\n")[0]}`);

  const videoDir = path.join(samples, "video");
  const audioDir = path.join(samples, "audio");
  const imageDir = path.join(samples, "image");
  const edgeDir = path.join(samples, "edge");
  const outDir = path.join(samples, "out");
  [videoDir, audioDir, imageDir, edgeDir, outDir].forEach(ensureDir);

  const base = path.join(videoDir, "base-1080p.mp4");
  run(
    [
      "-f", "lavfi", "-i", "testsrc2=size=1920x1080:rate=25:duration=3",
      "-f", "lavfi", "-i", "sine=frequency=440:sample_rate=48000:duration=3",
      "-c:v", "mpeg4", "-q:v", "5", "-c:a", "aac", "-b:a", "128k", "-shortest", base,
    ],
    "video/base-1080p.mp4（1080p/3s/含音轨）",
  );

  copyTo(base, path.join(videoDir, "中文 视频 样本.mp4"));
  copyTo(base, path.join(videoDir, "🎬 emoji 视频.mp4"));
  copyTo(base, path.join(videoDir, "空格 与 ' 引号' 视频.mp4"));

  const readonly = path.join(edgeDir, "只读-视频.mp4");
  copyTo(base, readonly);
  fs.chmodSync(readonly, 0o444);
  log("已把 edge/只读-视频.mp4 设为只读");

  const locked = path.join(edgeDir, "被占用-视频.mp4");
  copyTo(base, locked);

  fs.writeFileSync(path.join(edgeDir, "0字节.mp4"), "");
  log("已生成 edge/0字节.mp4（0 字节）");

  const corrupted = path.join(edgeDir, "损坏-视频.mp4");
  const random = Buffer.alloc(2048);
  for (let i = 0; i < random.length; i += 1) random[i] = (i * 37 + 11) & 0xff;
  fs.writeFileSync(corrupted, random);
  log("已生成 edge/损坏-视频.mp4（随机字节，不是合法 MP4）");

  // 超长路径（接近 Windows 260 字符限制，验证长路径处理）
  let longDir = path.join(samples, "long-path");
  for (let i = 1; i <= 6; i += 1) longDir = path.join(longDir, `第${i}层-中文目录-abcdefghij`);
  const longFile = path.join(longDir, "长路径 中文 视频.mp4");
  try {
    copyTo(base, longFile);
    log(`已生成长路径样本（${longFile.length} 字符）`);
  } catch (error) {
    console.warn(`[samples] 长路径样本创建失败（可能是系统的路径长度限制）：${error.message}`);
  }

  run(
    [
      "-f", "lavfi", "-i", "sine=frequency=330:sample_rate=44100:duration=30",
      "-c:a", "pcm_s16le", path.join(audioDir, "纯音乐 30s.wav"),
    ],
    "audio/纯音乐 30s.wav",
  );
  run(
    [
      "-f", "lavfi", "-i", "sine=frequency=660:sample_rate=48000:duration=12",
      "-c:a", "libmp3lame", "-b:a", "192k", path.join(audioDir, "🎵 emoji 音频.mp3"),
    ],
    "audio/🎵 emoji 音频.mp3",
  );

  run(
    ["-f", "lavfi", "-i", "testsrc2=size=1920x1080:rate=1", "-frames:v", "1", path.join(imageDir, "中文 图片.png")],
    "image/中文 图片.png（1920x1080）",
  );
  run(
    ["-f", "lavfi", "-i", "testsrc2=size=3840x2160:rate=1", "-frames:v", "1", path.join(imageDir, "🖼️ emoji 图片.jpg")],
    "image/🖼️ emoji 图片.jpg（4K）",
  );

  const manifest = {
    generatedAt: new Date().toISOString(),
    ffmpeg: (version.stdout || "").split("\n")[0],
    quick,
    cases: [
      { id: "video-1080p", path: path.relative(root, base), expect: "转 MP4 成功，输出的分辨率/时长与源一致" },
      { id: "video-chinese", path: "samples/video/中文 视频 样本.mp4", expect: "中文文件名输入输出均正常" },
      { id: "video-emoji", path: "samples/video/🎬 emoji 视频.mp4", expect: "emoji 文件名输入输出均正常" },
      { id: "video-readonly", path: path.relative(root, readonly), expect: "只读源文件可读取转码，且不被修改" },
      { id: "video-locked", path: path.relative(root, locked), expect: "被占用文件应给出 input 错误而不是崩溃" },
      { id: "video-zero", path: path.relative(root, corrupted.replace("损坏-视频", "0字节")), expect: "0 字节文件在入队前被拒绝" },
      { id: "video-corrupt", path: path.relative(root, corrupted), expect: "损坏文件给出明确的探测失败信息" },
      { id: "path-long", path: path.relative(root, longFile), expect: "超长路径可转码" },
      { id: "audio-wav", path: "samples/audio/纯音乐 30s.wav", expect: "转 MP3 与 AAC 均成功" },
      { id: "image-png", path: "samples/image/中文 图片.png", expect: "批量转换/缩放成功，冲突自动改名" },
    ],
  };
  fs.writeFileSync(path.join(samples, "manifest.json"), JSON.stringify(manifest, null, 2));
  log("已写入 samples/manifest.json");

  if (!quick) {
    run(
      [
        "-f", "lavfi", "-i", "testsrc2=size=320x240:rate=5:duration=120",
        "-f", "lavfi", "-i", "sine=frequency=220:sample_rate=44100:duration=120",
        "-c:v", "mpeg4", "-q:v", "8", "-c:a", "aac", "-b:a", "96k", "-shortest",
        path.join(videoDir, "长时长 120s.mp4"),
      ],
      "video/长时长 120s.mp4",
    );
    run(
      [
        "-f", "lavfi", "-i", "testsrc2=size=3840x2160:rate=4:duration=2",
        "-c:v", "mpeg4", "-q:v", "4", path.join(videoDir, "超大 4K 2s.mp4"),
      ],
      "video/超大 4K 2s.mp4",
    );
  }

  log("样本库完成。");
  const info = probeInfo(base);
  log(`校验 base-1080p.mp4：${info ? `${info.format} / ${info.streams} 个流` : "ffprobe 不可用"}`);
}

main();
