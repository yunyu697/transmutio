#!/usr/bin/env node
/**
 * 生成 Tauri updater 清单 latest.json（LGPL）与 latest-gpl.json（GPL）。
 *
 * 用法：
 *   node scripts/release-manifest.mjs --flat dist-flat --tag v1.0.0-alpha7
 *
 * 每个平台条目都带 `signature`（即同目录 `<资产名>.sig` 的内容）与 `size`、`url`。
 * 资产名按 GitHub 上传规则归一（空格 → 点），与 Release 实际文件名一致。
 * 缺少 `.sig` 或缺少某平台安装包时直接失败：签名是安装的硬前提，不做降级。
 */
import { readFileSync, readdirSync, statSync, writeFileSync } from "node:fs";
import { join } from "node:path";

const REPO = "yunyu697/transmutio";
const args = process.argv.slice(2);
const flatIndex = args.indexOf("--flat");
const flatDir = flatIndex >= 0 ? args[flatIndex + 1] : "dist-flat";
const tagIndex = args.indexOf("--tag");
const tag = (tagIndex >= 0 ? args[tagIndex + 1] : process.env.GITHUB_REF_NAME || "").trim();
if (!tag.startsWith("v")) {
  console.error("缺少 --tag v<version>（或 GITHUB_REF_NAME）");
  process.exit(1);
}
const version = tag.replace(/^v/, "");

const files = readdirSync(flatDir).filter((name) => statSync(join(flatDir, name)).isFile());
const normalize = (name) => name.replace(/ /g, ".");

const isGpl = (name) => /^Transmutio(\.| )GPL_/.test(name);
const isLgpl = (name) => /^Transmutio_/.test(name);

function pick(flavor, predicate) {
  const found = files.filter((name) => (flavor === "gpl" ? isGpl(name) : isLgpl(name)) && predicate(name));
  // 同一平台若同时匹配多个文件，优先 NSIS/AppImage；其余情况取排序后的第一个。
  found.sort();
  return found[0] || null;
}

const platformMatchers = [
  ["windows-x86_64", (name) => name.endsWith("_x64-setup.exe")],
  ["darwin-aarch64", (name) => name.endsWith("_aarch64.dmg")],
  ["linux-x86_64", (name) => name.endsWith("_amd64.AppImage")],
];

function manifestFor(flavor, outName) {
  const platforms = {};
  const missing = [];
  for (const [key, matcher] of platformMatchers) {
    const asset = pick(flavor, matcher);
    if (!asset) {
      missing.push(`${flavor}:${key}`);
      continue;
    }
    const assetName = normalize(asset);
    let signature;
    try {
      signature = readFileSync(join(flatDir, `${asset}.sig`), "utf8").trim();
    } catch {
      console.error(`缺少签名文件 ${asset}.sig（先执行 signer sign），拒绝生成清单`);
      process.exit(1);
    }
    // Tauri signer 输出的是 base64 包裹的 minisign 签名；也兼容直接写明文 minisign 文本。
    const looksSigned =
      signature.startsWith("untrusted comment:") ||
      Buffer.from(signature, "base64").toString("utf8").startsWith("untrusted comment:");
    if (!looksSigned) {
      console.error(`${asset}.sig 内容不是 minisign 签名，拒绝生成清单`);
      process.exit(1);
    }
    platforms[key] = {
      signature,
      url: `https://github.com/${REPO}/releases/download/${tag}/${assetName}`,
      size: statSync(join(flatDir, asset)).size,
    };
  }
  if (missing.length) {
    console.error(`缺少平台安装包：${missing.join(", ")}`);
    process.exit(1);
  }
  const manifest = {
    version,
    notes: readNotes(),
    pub_date: new Date().toISOString().replace(/\.\d{3}Z$/, "Z"),
    platforms,
  };
  const outPath = join(flatDir, outName);
  writeFileSync(outPath, JSON.stringify(manifest, null, 2) + "\n", "utf8");
  console.log(`已写入 ${outPath}`);
  for (const [key, entry] of Object.entries(platforms)) {
    console.log(`  ${key}  ${entry.size} bytes  ${entry.url}`);
  }
}

function readNotes() {
  try {
    return readFileSync(join("release-notes", `${tag}.md`), "utf8").trim();
  } catch {
    return "";
  }
}

manifestFor("lgpl", "latest.json");
manifestFor("gpl", "latest-gpl.json");
