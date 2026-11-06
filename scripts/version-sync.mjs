#!/usr/bin/env node
/**
 * 单一版本来源：`package.json` 的 `version` 是唯一可手改的地方。
 * 本脚本在构建前把版本注入 `src-tauri/tauri.conf.json`（含 GPL overlay）、
 * `src-tauri/Cargo.toml`、`package-lock.json`，构建产物（Cargo 的 `CARGO_PKG_VERSION` /
 * `--version` / 关于页）因此与 tag 同源。
 *
 * MSI 限制：WiX 只接受数字型预发布标识（`1.0.0-1` 可以，`1.0.0-alpha1` 不行），
 * 因此 alpha 版本会额外写入 `bundle.windows.wix.version = <major>.<minor>.<patch>.<序号>`；
 * 正式版则移除该键，由 Tauri 从应用版本推导。
 *
 * 用法：node scripts/version-sync.mjs [--check]
 *   --check  只校验不写入（CI 可用），不一致时退出码 1。
 */
import { existsSync, readFileSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { parseVersion } from "./version-rules.mjs";

const root = process.cwd();
const PKG = join(root, "package.json");
const LOCK = join(root, "package-lock.json");
const TAURI_FILES = [
  join(root, "src-tauri", "tauri.conf.json"),
  join(root, "src-tauri", "tauri.gpl.conf.json"),
];
const CARGO = join(root, "src-tauri", "Cargo.toml");
const checkOnly = process.argv.includes("--check");

function fail(message) {
  console.error(`[version-sync] ${message}`);
  process.exit(1);
}
function writeJson(path, data) {
  writeFileSync(path, JSON.stringify(data, null, 2) + "\n", "utf8");
}

const version = JSON.parse(readFileSync(PKG, "utf8")).version;
const parsed = parseVersion(version);
if (!parsed.ok) fail(`package.json 版本不合法：${parsed.reason}`);

/** alpha → 1.0.0.<序号>；正式版 → null（删除该键，交给 Tauri 推导）。 */
const wixVersion = parsed.isAlpha ? `${parsed.base}.${parsed.alpha}` : null;

const cargoText = readFileSync(CARGO, "utf8");
const cargoVersion = (/^version\s*=\s*"([^"]+)"/m.exec(cargoText) || [])[1];
const drift = [];
if (cargoVersion !== version) drift.push(`Cargo.toml=${cargoVersion}`);

const tauriPlans = [];
for (const path of TAURI_FILES) {
  const config = JSON.parse(readFileSync(path, "utf8"));
  const seenVersion = config.version;
  const wix = config.bundle?.windows?.wix;
  const seenWix = wix?.version ?? null;
  const hasWix = Boolean(wix);
  if (seenVersion !== version) drift.push(`${path}=${seenVersion}`);
  if (hasWix && seenWix !== wixVersion) drift.push(`${path} wix.version=${seenWix}（期望 ${wixVersion}）`);
  tauriPlans.push({ path, config, hasWix });
}

if (drift.length > 0 && checkOnly) {
  fail(`版本文件不一致（源 package.json=${version}；${drift.join("，")}）。请执行 npm run version:sync`);
}

if (drift.length > 0) {
  for (const plan of tauriPlans) {
    plan.config.version = version;
    if (plan.hasWix) {
      if (wixVersion) plan.config.bundle.windows.wix.version = wixVersion;
      else delete plan.config.bundle.windows.wix.version;
    }
    writeJson(plan.path, plan.config);
  }
  writeFileSync(CARGO, cargoText.replace(/^version\s*=\s*"[^"]+"/m, `version = "${version}"`), "utf8");
  if (existsSync(LOCK)) {
    const lock = JSON.parse(readFileSync(LOCK, "utf8"));
    lock.version = version;
    if (lock.packages && lock.packages[""]) lock.packages[""].version = version;
    writeFileSync(LOCK, JSON.stringify(lock, null, 2) + "\n", "utf8");
  }
  console.log(
    `[version-sync] 已注入 ${version} → tauri.conf.json / tauri.gpl.conf.json / Cargo.toml / package-lock.json` +
      (wixVersion ? `（MSI 版本 ${wixVersion}）` : "（MSI 版本由 Tauri 推导）"),
  );
} else {
  console.log(`[version-sync] 版本文件已一致：${version}${checkOnly ? "（check 模式，仅校验）" : ""}`);
}
