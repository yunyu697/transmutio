#!/usr/bin/env node
/**
 * 分发前许可检查：确保随包放入的 FFmpeg 不是 GPL / nonfree 构建。
 *
 * 检查范围：
 *  - src-tauri/resources/ffmpeg/**（打包时会复制到资源目录）
 *  - src-tauri/binaries/**（Tauri externalBin 约定目录）
 *  - 命令行传入的额外路径
 *
 * 退出码：
 *  0 = 未发现问题（包括"默认包未放二进制"）
 *  1 = 发现 GPL / nonfree 二进制或包含 libx264/libx265/fdk-aac 等组件
 */
import { spawnSync } from "node:child_process";
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const scanDirs = [
  path.join(root, "src-tauri", "resources", "ffmpeg"),
  path.join(root, "src-tauri", "binaries"),
  ...process.argv.slice(2).map((p) => path.resolve(p)),
];

const FORBIDDEN_ENCODERS = ["libx264", "libx264rgb", "libx265", "libfdk_aac"];
const FORBIDDEN_CONFIG = ["--enable-gpl", "--enable-nonfree", "--enable-libx264", "--enable-libx265", "--enable-libfdk-aac"];
const binaryNames = /^(ffmpeg|ffprobe)(-[\w.-]+)?(\.exe)?$/i;

function walk(dir, found = []) {
  if (!fs.existsSync(dir)) return found;
  for (const entry of fs.readdirSync(dir, { withFileTypes: true })) {
    const full = path.join(dir, entry.name);
    if (entry.isDirectory()) walk(full, found);
    else if (binaryNames.test(entry.name)) found.push(full);
  }
  return found;
}

function inspect(file) {
  const version = spawnSync(file, ["-hide_banner", "-version"], { encoding: "utf8", timeout: 20_000 });
  if (version.error || version.status !== 0) {
    return { ok: false, reason: `无法执行（${version.error?.message ?? `退出码 ${version.status}`}）` };
  }
  const text = `${version.stdout ?? ""}\n${version.stderr ?? ""}`;
  const configLine = text.split("\n").find((line) => line.includes("--enable-")) ?? "";
  const encoders = spawnSync(file, ["-hide_banner", "-encoders"], { encoding: "utf8", timeout: 20_000 });
  const encoderText = encoders.stdout ?? "";
  const gpl = configLine.includes("--enable-gpl");
  const nonfree = configLine.includes("--enable-nonfree");
  const forbidden = FORBIDDEN_ENCODERS.filter((id) => new RegExp(`\\s${id}\\s`).test(encoderText));
  return {
    ok: true,
    version: (version.stdout ?? "").split("\n")[0],
    configLine,
    gpl,
    nonfree,
    forbidden,
    violations: [...FORBIDDEN_CONFIG.filter((c) => configLine.includes(c)), ...forbidden.map((id) => `encoder:${id}`)],
  };
}

let failures = 0;
let checked = 0;

console.log("== FFmpeg 许可检查 ==");
for (const dir of scanDirs) {
  const files = walk(dir);
  if (!files.length) {
    console.log(`- ${path.relative(root, dir)}：未发现 ffmpeg/ffprobe 二进制（默认包即为此状态）`);
    continue;
  }
  for (const file of files) {
    checked += 1;
    const info = inspect(file);
    const rel = path.relative(root, file);
    if (!info.ok) {
      console.log(`- ${rel}：${info.reason}（无法确认许可，按失败处理）`);
      failures += 1;
      continue;
    }
    const bad = info.violations.length > 0;
    console.log(`- ${rel}`);
    console.log(`  版本：${info.version}`);
    console.log(`  配置：${info.configLine || "（未解析到 configuration）"}`);
    console.log(`  GPL=${info.gpl} nonfree=${info.nonfree} 违规组件=${info.violations.join(", ") || "无"}`);
    if (bad) failures += 1;
  }
}

if (!checked) {
  console.log("\n结论：包里没有 FFmpeg 二进制。默认构建产物不具备转码能力，需要在分发前放入 LGPL 构建（见 LICENSING.md）。");
} else if (failures) {
  console.error(`\n结论：发现 ${failures} 个 GPL/nonfree 二进制或组件，禁止随默认安装包分发。`);
  process.exitCode = 1;
} else {
  console.log("\n结论：检查到的二进制均为 LGPL 安全构建，可分发。");
}
