#!/usr/bin/env node
/**
 * 构建一个发行变体的可发布产物（默认 = LGPL，必须显式传 gpl）。
 *
 * - 同一份源码：GPL 变体只通过 Cargo feature `gpl` + Tauri 配置 overlay 注入，不复制代码分支。
 * - 构建前先跑 `flavor-report`，记录实际生效的编译期开关（同样的 feature 组合）。
 * - 构建后把安装包从共享的 target 目录复制到 dist-artifacts/<flavor>/，避免两个变体互相覆盖。
 * - 产物名 / 大小 / SHA256 写入 samples/build-<flavor>.json，并复核另一变体产物仍在且哈希未变。
 *
 * 用法：node scripts/build-flavor.mjs [lgpl|gpl]
 */
import { createHash } from "node:crypto";
import { existsSync, mkdirSync, readdirSync, readFileSync, rmSync, statSync, copyFileSync, writeFileSync } from "node:fs";
import { spawnSync } from "node:child_process";
import { join, basename, extname } from "node:path";

const root = process.cwd();
const flavor = (process.argv[2] || "lgpl").toLowerCase();
if (flavor !== "lgpl" && flavor !== "gpl") {
  console.error(`未知变体：${flavor}（只接受 lgpl / gpl）`);
  process.exit(2);
}

const npm = process.platform === "win32" ? "npm.cmd" : "npm";
const cargo = process.platform === "win32" ? "cargo.exe" : "cargo";

// 版本单一来源：构建前把 package.json 的 version 注入 tauri.conf.json / Cargo.toml，
// 保证产物 --version、关于页与 tag 同源。注入失败即中止，不产出错版本包。
{
  const sync = spawnSync(process.execPath, [join(root, "scripts", "version-sync.mjs")], { stdio: "inherit" });
  if (sync.status !== 0) {
    console.error("版本注入失败，终止构建");
    process.exit(sync.status ?? 1);
  }
}

const featureArgs = flavor === "gpl" ? ["--features", "gpl"] : [];
// flavor-report 属于 dev-tools 二进制，预检时需要显式启用（安装包本身不带它）。
const preflightFeatures = flavor === "gpl" ? ["dev-tools,gpl"] : ["dev-tools"];
const overlayPath = "src-tauri/tauri.gpl.conf.json";
const distDir = join(root, "dist-artifacts", flavor);
// target 目录由两个变体共享：构建前必须清掉上一轮残留的安装包，否则会把另一变体的产物一起复制过来。
const bundleDir = join(root, "src-tauri", "target", "release", "bundle");
const reportPath = join(root, "samples", `build-${flavor}.json`);

function run(cmd, args, label) {
  console.log(`\n$ ${cmd} ${args.join(" ")}`);
  const started = Date.now();
  const result = spawnSync(cmd, args, { stdio: "inherit", shell: process.platform === "win32" });
  const seconds = Math.round((Date.now() - started) / 1000);
  if (result.status !== 0) {
    console.error(`\n[${label}] 失败：退出码 ${result.status}（${seconds}s）`);
    return false;
  }
  console.log(`[${label}] 完成（${seconds}s）`);
  return true;
}

function sha256(path) {
  const hash = createHash("sha256");
  hash.update(readFileSync(path));
  return hash.digest("hex");
}

function walk(dir, out = []) {
  if (!existsSync(dir)) return out;
  for (const entry of readdirSync(dir, { withFileTypes: true })) {
    const full = join(dir, entry.name);
    if (entry.isDirectory()) walk(full, out);
    else out.push(full);
  }
  return out;
}

const INSTALLER_EXT = new Set([".msi", ".exe", ".dmg", ".deb", ".rpm", ".appimage"]);

function writeReport(payload) {
  mkdirSync(join(root, "samples"), { recursive: true });
  writeFileSync(reportPath, JSON.stringify(payload, null, 2), "utf8");
  console.log(`\n报告已写入 ${reportPath}`);
}

// 0) 预检：先确认同一 feature 组合下编译期开关是期望值
const cargoArgs = [
  "run", "--quiet", "--manifest-path", "src-tauri/Cargo.toml",
  "--bin", "flavor-report", "--features", ...preflightFeatures,
];
let preflight;
try {
  const out = spawnSync(cargo, cargoArgs, { encoding: "utf8", shell: process.platform === "win32" });
  if (out.status !== 0) throw new Error(out.stderr || `exit ${out.status}`);
  preflight = JSON.parse(out.stdout.trim().split(/\r?\n/).pop());
} catch (error) {
  console.error(`无法运行 flavor-report：${error instanceof Error ? error.message : error}`);
  process.exit(1);
}

console.log(`变体：${flavor}`);
console.log(`编译期开关：${JSON.stringify(preflight)}`);
if (preflight.flavor !== flavor) {
  console.error(`编译期变体不一致：期望 ${flavor}，实际 ${preflight.flavor}`);
  process.exit(1);
}
if (flavor === "gpl" && (preflight.allowGpl !== true || preflight.requireGplBuild !== true)) {
  console.error("GPL 变体的编译期约束未生效（allowGpl / requireGplBuild 应为 true）");
  process.exit(1);
}
if (flavor === "lgpl" && (preflight.allowGpl !== false || preflight.requireGplBuild !== false)) {
  console.error("LGPL 变体的编译期约束不符合预期（allowGpl / requireGplBuild 应为 false）");
  process.exit(1);
}

// 1) 清理本变体的产物目录 + 共享的 bundle 目录（另一个 dist-artifacts 目录不动）
rmSync(distDir, { recursive: true, force: true });
mkdirSync(distDir, { recursive: true });
rmSync(bundleDir, { recursive: true, force: true });

// 2) 构建（前端由 beforeBuildCommand 负责）
const tauriArgs = ["run", "tauri", "--", "build", ...featureArgs];
if (flavor === "gpl") tauriArgs.push("--config", overlayPath);
const startedAt = Date.now();
const ok = run(npm, tauriArgs, `tauri build (${flavor})`);
const seconds = Math.round((Date.now() - startedAt) / 1000);

// 3) 收集安装包
const artifacts = [];
for (const path of walk(bundleDir)) {
  if (!INSTALLER_EXT.has(extname(path).toLowerCase())) continue;
  const name = basename(path);
  const target = join(distDir, name);
  copyFileSync(path, target);
  artifacts.push({
    file: target,
    name,
    size: statSync(target).size,
    sha256: sha256(target),
  });
}

// 3.1) 归档本次构建生成的 NSIS 脚本，供 check:bundle 做静态身份断言
//      （target/release/nsis 会被下一个变体覆盖，必须随产物一起留档）。
const generatedNsi = join(root, "src-tauri", "target", "release", "nsis", "x64", "installer.nsi");
if (existsSync(generatedNsi)) {
  copyFileSync(generatedNsi, join(distDir, "installer.generated.nsi"));
}

// 4) 复核另一变体没有被覆盖
const other = flavor === "lgpl" ? "gpl" : "lgpl";
const otherReport = join(root, "samples", `build-${other}.json`);
let otherCheck = { flavor: other, reportExists: existsSync(otherReport), intact: null };
if (existsSync(otherReport)) {
  const prior = JSON.parse(readFileSync(otherReport, "utf8"));
  const rows = (prior.artifacts ?? []).map((item) => ({
    name: item.name,
    exists: existsSync(item.file),
    sameSize: existsSync(item.file) && statSync(item.file).size === item.size,
    sameSha256: existsSync(item.file) && sha256(item.file) === item.sha256,
  }));
  otherCheck.intact = rows.length > 0 && rows.every((r) => r.exists && r.sameSize && r.sameSha256);
  otherCheck.files = rows;
}

// 5) 安装包名必须能区分两个变体
const gplMarker = artifacts.every((a) => !/GPL/i.test(a.name));
const namesDistinct = flavor === "lgpl" ? gplMarker : artifacts.every((a) => /GPL/i.test(a.name));

writeReport({
  generatedAt: new Date().toISOString(),
  flavor,
  status: ok ? "ok" : "build-failed",
  seconds,
  compileTimeFlags: preflight,
  preflightProfile: "debug（flavor-report 的编译 profile；安装包为 release，debug_assertions=false）",
  cargoFeatures: featureArgs.join(" ") || "(none)",
  tauriConfig: flavor === "gpl" ? ["src-tauri/tauri.conf.json", overlayPath] : ["src-tauri/tauri.conf.json"],
  artifacts,
  artifactsEmpty: artifacts.length === 0,
  artifactNamesDistinct: namesDistinct,
  otherVariant: otherCheck,
  note:
    "安装包从 src-tauri/target/release/bundle 复制到 dist-artifacts/<flavor>/；" +
    "构建后的产物名/SHA256 用于确认两个变体互不覆盖。",
});

if (!ok || artifacts.length === 0) {
  console.error(`构建失败或未产出安装包（${flavor}）`);
  process.exit(1);
}
if (!namesDistinct) console.error("安装包名称没有区分出 GPL/非 GPL，请检查 productName");
console.log(`\n产物已复制到 dist-artifacts/${flavor}/（${artifacts.length} 个安装包）`);
for (const a of artifacts) console.log(`  ${a.name}  ${a.size} bytes  sha256=${a.sha256}`);
