#!/usr/bin/env node
/**
 * 版本校验（CI 与发布前都跑）：
 *   1. 三个版本文件一致（package.json / tauri.conf.json / Cargo.toml）；
 *   2. 段位规则：除第一段外每段 ≤ 10（0.11.0 直接失败）；
 *   3. alpha 序号：正数、无前导零、单调递增（alpha0 / alpha01 / 回退序号失败）；
 *   4. tag 与版本一致（tag 必须等于 v<version>）；
 *   5. 可选：产物 `--version` 与版本文件一致（--binary <path>）。
 *
 * 用法：
 *   node scripts/check-version.mjs                       # 仅校验文件与规则
 *   node scripts/check-version.mjs --tag v0.2.0-alpha1   # 额外校验 tag
 *   node scripts/check-version.mjs --tag v0.2.0-alpha1 --binary dist-artifacts/lgpl/xxx.exe
 */
import { existsSync, readFileSync } from "node:fs";
import { join } from "node:path";
import { spawnSync } from "node:child_process";
import {
  parseVersion,
  checkAlphaSequence,
  collectAlphaSequences,
  checkTag,
  checkBinaryVersion,
} from "./version-rules.mjs";

const root = process.cwd();
const args = process.argv.slice(2);
function argOf(name) {
  const i = args.indexOf(name);
  return i >= 0 ? args[i + 1] : undefined;
}
const tagArg = argOf("--tag") || (process.env.GITHUB_REF_NAME?.startsWith("v") ? process.env.GITHUB_REF_NAME : undefined);
const binaryArg = argOf("--binary");
// 构建作业里用：浅克隆拿不到历史 tag，序号单调性由独立的 guard 作业负责。
const skipSequence = args.includes("--skip-alpha-sequence");

const failures = [];
const notes = [];

const version = JSON.parse(readFileSync(join(root, "package.json"), "utf8")).version;
const parsed = parseVersion(version);
if (!parsed.ok) failures.push(`package.json 版本不合法：${parsed.reason}`);

// 1) 三个版本文件一致
const tauriConfig = JSON.parse(readFileSync(join(root, "src-tauri", "tauri.conf.json"), "utf8"));
const overlayConfig = JSON.parse(readFileSync(join(root, "src-tauri", "tauri.gpl.conf.json"), "utf8"));
const tauriVersion = tauriConfig.version;
const overlayVersion = overlayConfig.version ?? version;
const cargoVersion = (/^version\s*=\s*"([^"]+)"/m.exec(readFileSync(join(root, "src-tauri", "Cargo.toml"), "utf8")) || [])[1];
for (const [file, seen] of [["tauri.conf.json", tauriVersion], ["tauri.gpl.conf.json", overlayVersion], ["Cargo.toml", cargoVersion]]) {
  if (seen !== version) failures.push(`版本文件不一致：${file}=${seen}，package.json=${version}`);
}

// 1.1) MSI 版本（WiX 只接受数字型预发布标识）
const wixVersion = tauriConfig.bundle?.windows?.wix?.version ?? null;
if (parsed.ok && parsed.isAlpha) {
  const expectedWix = `${parsed.base}.${parsed.alpha}`;
  if (wixVersion !== expectedWix) failures.push(`MSI 版本不一致：wix.version=${wixVersion}，期望 ${expectedWix}（alpha 需用数字序号）`);
} else if (wixVersion) {
  failures.push(`正式版不应固定 wix.version（当前 ${wixVersion}）`);
}

// 2/3) alpha 序号单调递增（guard 作业负责；构建作业可跳过）
const tagList = spawnSync("git", ["tag", "--list"], { encoding: "utf8" });
const tags = tagList.status === 0 ? tagList.stdout.split(/\r?\n/).filter(Boolean) : [];
if (parsed.ok && parsed.isAlpha && !skipSequence) {
  const targetTag = tagArg || `v${parsed.value}`;
  const sequences = collectAlphaSequences(tags.filter((t) => t !== targetTag), parsed.base);
  const seqCheck = checkAlphaSequence(parsed, sequences);
  if (!seqCheck.ok) failures.push(seqCheck.reason);
  notes.push(`已有同基线 alpha：${sequences.length ? sequences.map((n) => `alpha${n}`).join(" / ") : "（无）"}`);
}

// 4) tag 与版本一致
if (tagArg) {
  const tagCheck = checkTag(parsed, tagArg);
  if (!tagCheck.ok) failures.push(tagCheck.reason);
}

// 5) 产物版本（可选）
if (binaryArg) {
  if (!existsSync(binaryArg)) {
    failures.push(`产物不存在：${binaryArg}`);
  } else {
    const run = spawnSync(binaryArg, ["--version"], { encoding: "utf8", timeout: 30000 });
    const output = `${run.stdout || ""}${run.stderr || ""}`;
    const binCheck = checkBinaryVersion(parsed, output);
    if (!binCheck.ok) failures.push(binCheck.reason);
    else notes.push(`产物版本：${binCheck.observed}`);
    if (run.status !== 0) notes.push(`产物 --version 退出码 ${run.status}（若未输出可能未识别该参数）`);
  }
}

const summary = {
  version,
  tag: tagArg ?? null,
  isAlpha: parsed.ok ? parsed.isAlpha : null,
  failures,
  notes,
};
console.log(JSON.stringify(summary, null, 2));
if (failures.length > 0) {
  console.error(`[check-version] 失败：\n - ${failures.join("\n - ")}`);
  process.exit(1);
}
console.error("[check-version] 通过");
