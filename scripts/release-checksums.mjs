#!/usr/bin/env node
/**
 * 生成 Release 用的 SHA256SUMS。
 *
 * 两种模式：
 *   node scripts/release-checksums.mjs                     # 扫 dist-artifacts/{lgpl,gpl}
 *   node scripts/release-checksums.mjs --flat <dir> [--normalize-names] [输出路径]
 *
 * `--normalize-names` 把文件名里的空格替换为点：GitHub Release 上传时会这样做，
 * 下载后的文件名与 SHA256SUMS 里的名字一致，`sha256sum -c` 才能直接通过。
 */
import { createHash } from "node:crypto";
import { readdirSync, readFileSync, statSync, writeFileSync } from "node:fs";
import { join, extname } from "node:path";

// `.sig` 也纳入校验清单：下载签名文件后可独立核对，避免"签名与包不配套"。
const INSTALLER_EXT = new Set([".msi", ".exe", ".dmg", ".deb", ".rpm", ".appimage", ".gz", ".sig"]);
const args = process.argv.slice(2);
const flatIndex = args.indexOf("--flat");
const flatDir = flatIndex >= 0 ? args[flatIndex + 1] : null;
const normalize = args.includes("--normalize-names");
const positional = args.filter((a, i) => !a.startsWith("--") && (flatIndex < 0 || i !== flatIndex + 1));
const outPath = positional[0] || join(flatDir || join(process.cwd(), "dist-artifacts"), "SHA256SUMS");

const rows = [];
function collect(dir) {
  let entries = [];
  try {
    entries = readdirSync(dir);
  } catch {
    return;
  }
  for (const name of entries) {
    if (!INSTALLER_EXT.has(extname(name).toLowerCase())) continue;
    const path = join(dir, name);
    if (!statSync(path).isFile()) continue;
    rows.push({
      name: normalize ? name.replace(/ /g, ".") : name,
      hash: createHash("sha256").update(readFileSync(path)).digest("hex"),
      size: statSync(path).size,
    });
  }
}

if (flatDir) {
  collect(flatDir);
} else {
  for (const flavor of ["lgpl", "gpl"]) collect(join(process.cwd(), "dist-artifacts", flavor));
}

if (rows.length === 0) {
  console.error("没有找到安装包，未生成 SHA256SUMS");
  process.exit(1);
}
rows.sort((a, b) => a.name.localeCompare(b.name));
writeFileSync(outPath, rows.map((r) => `${r.hash}  ${r.name}`).join("\n") + "\n", "utf8");
console.log(`已写入 ${outPath}（${rows.length} 个产物）`);
for (const r of rows) console.log(`  ${r.size} bytes  ${r.name}  ${r.hash}`);
