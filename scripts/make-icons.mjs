#!/usr/bin/env node
/**
 * 生成应用图标集（唯一的图标生成入口，`npm run icons`）。
 *
 * 规则：
 * - 软件图标一律用浅色底版本（任务栏 / 开始菜单 / 安装器 / 窗口图标都同一份）；
 * - 16 / 24 / 32px 用光学版本 `brand-light-small.svg`（去投影、加宽卡片、加粗字形），
 *   避免原图直接缩放后在小尺寸糊成一块；
 * - 48px 以上用原图 `brand-light.svg`，保证渐变与玻璃质感；
 * - 界面内的品牌图标不在这里生成，由 `src/components/BrandMark.tsx` 按明暗直接引用
 *   `brand-light.svg` / `brand-dark.svg`，只跟亮度、不跟配色。
 *
 * 产物：src-tauri/icons 下 Tauri 全套 PNG / icon.ico（16/24/32/48/64/128/256）/
 * icon.icns（16…1024）、Windows Store 的 Square*Logo 与 StoreLogo，以及 android / ios。
 * 预览图写到 samples/icon-preview/（不入库），便于人眼检查小尺寸。
 */
import { spawnSync } from "node:child_process";
import fs from "node:fs";
import path from "node:path";
import { createRequire } from "node:module";
import { fileURLToPath } from "node:url";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const require = createRequire(path.join(root, "package.json"));
const { chromium } = require("playwright-core");

const ICONS = path.join(root, "src-tauri", "icons");
const PREVIEW = path.join(root, "samples", "icon-preview");
const ASSETS = path.join(root, "src", "assets");
const light = fs.readFileSync(path.join(ASSETS, "brand-light.svg"));
const lightSmall = fs.readFileSync(path.join(ASSETS, "brand-light-small.svg"));

fs.mkdirSync(ICONS, { recursive: true });
fs.mkdirSync(PREVIEW, { recursive: true });

const browser = await chromium.launch({ channel: "msedge", headless: true });

/** 用 Chromium 渲染 SVG 到指定像素（与 WebView 内实际观感一致）。 */
async function render(svg, size) {
  const page = await browser.newPage({ viewport: { width: size, height: size }, deviceScaleFactor: 1 });
  const url = `data:image/svg+xml;base64,${Buffer.from(svg).toString("base64")}`;
  await page.setContent(
    `<style>html,body{margin:0;padding:0;background:transparent}img{display:block;width:${size}px;height:${size}px}</style><img src="${url}">`,
  );
  await page.waitForFunction(() => document.images[0]?.complete);
  const png = await page.screenshot({ omitBackground: true });
  await page.close();
  return png;
}

const png = {};
for (const size of [1024, 512, 310, 284, 256, 150, 142, 128, 107, 89, 71, 64, 50, 48]) png[size] = await render(light, size);
for (const size of [44, 32, 30, 24, 16]) png[size] = await render(lightSmall, size);
await browser.close();

/** ICO：16 字节目录 + PNG 载荷；256 的宽高字节写 0。 */
function buildIco(entries) {
  const header = Buffer.alloc(6);
  header.writeUInt16LE(0, 0);
  header.writeUInt16LE(1, 2);
  header.writeUInt16LE(entries.length, 4);
  const dir = Buffer.alloc(16 * entries.length);
  let offset = 6 + dir.length;
  entries.forEach(([size, data], i) => {
    const o = i * 16;
    dir.writeUInt8(size >= 256 ? 0 : size, o);
    dir.writeUInt8(size >= 256 ? 0 : size, o + 1);
    dir.writeUInt8(0, o + 2);
    dir.writeUInt8(0, o + 3);
    dir.writeUInt16LE(1, o + 4);
    dir.writeUInt16LE(32, o + 6);
    dir.writeUInt32LE(data.length, o + 8);
    dir.writeUInt32LE(offset, o + 12);
    offset += data.length;
  });
  return Buffer.concat([header, dir, ...entries.map(([, data]) => data)]);
}

/** ICNS：8 字节头 + 每个条目（4 字节类型 + 4 字节总长 + PNG）。 */
function buildIcns(entries) {
  const chunks = entries.map(([type, data]) => {
    const head = Buffer.alloc(8);
    head.write(type, 0, 4, "ascii");
    head.writeUInt32BE(data.length + 8, 4);
    return Buffer.concat([head, data]);
  });
  const body = Buffer.concat(chunks);
  const head = Buffer.alloc(8);
  head.write("icns", 0, 4, "ascii");
  head.writeUInt32BE(body.length + 8, 4);
  return Buffer.concat([head, body]);
}

const ico = buildIco([
  [16, png[16]],
  [24, png[24]],
  [32, png[32]],
  [48, png[48]],
  [64, png[64]],
  [128, png[128]],
  [256, png[256]],
]);

const icns = buildIcns([
  ["icp4", png[16]],
  ["icp5", png[32]],
  ["icp6", png[64]],
  ["ic07", png[128]],
  ["ic08", png[256]],
  ["ic09", png[512]],
  ["ic10", png[1024]],
]);

// 先让 Tauri CLI 铺一遍 android / ios / 各平台图标，再用本脚本的光学版本覆盖桌面部分。
// 直接调用本地 CLI 的 JS 入口：Windows 上 spawnSync("npx.cmd") 在部分 Node 版本会 EINVAL。
const source = path.join(ICONS, ".source-1024.png");
fs.writeFileSync(source, png[1024]);
const tauriCli = path.join(root, "node_modules", "@tauri-apps", "cli", "tauri.js");
const tauri = spawnSync(process.execPath, [tauriCli, "icon", source, "-o", ICONS], { encoding: "utf8", cwd: root });
if (tauri.status !== 0) {
  console.error(tauri.stdout || "");
  console.error(tauri.stderr || "");
  throw new Error("tauri icon 失败");
}

const write = (name, data) => fs.writeFileSync(path.join(ICONS, name), data);
write("icon.png", png[1024]);
write("icon.ico", ico);
write("icon.icns", icns);
write("32x32.png", png[32]);
write("64x64.png", png[64]);
write("128x128.png", png[128]);
write("128x128@2x.png", png[256]);
write("Square30x30Logo.png", png[30]);
for (const [name, size] of [
  ["Square44x44Logo.png", 44],
  ["Square71x71Logo.png", 71],
  ["Square89x89Logo.png", 89],
  ["Square107x107Logo.png", 107],
  ["Square142x142Logo.png", 142],
  ["Square150x150Logo.png", 150],
  ["Square284x284Logo.png", 284],
  ["Square310x310Logo.png", 310],
  ["StoreLogo.png", 50],
]) {
  write(name, png[size]);
}
fs.rmSync(source, { force: true });

// 人眼检查用：实际像素 + 放大对照（nearest 放大，不用平滑，方便看轮廓）
for (const size of [16, 24, 32, 48, 64, 128, 256, 512]) {
  fs.writeFileSync(path.join(PREVIEW, `${size}.png`), png[size]);
}
const sheetBrowser = await chromium.launch({ channel: "msedge", headless: true });
for (const [name, sizes, zoom] of [
  ["small-8x", [16, 24, 32], 8],
  ["small-4x", [16, 24, 32], 4],
  ["all", [16, 24, 32, 48, 64, 128, 256], 1],
]) {
  const page = await sheetBrowser.newPage({ viewport: { width: 1100, height: 460 }, deviceScaleFactor: 1 });
  const cells = sizes
    .map((size) => {
      const url = `data:image/png;base64,${png[size].toString("base64")}`;
      return `<figure><img src="${url}" style="width:${size * zoom}px;height:${size * zoom}px;image-rendering:pixelated"><figcaption>${size}px${zoom > 1 ? `（${zoom}× 放大）` : ""}</figcaption></figure>`;
    })
    .join("");
  await page.setContent(
    `<style>body{margin:0;padding:24px;background:#eef2f8;font:12px/1.4 "Segoe UI",sans-serif;color:#33415c;display:flex;gap:28px;align-items:flex-end}figure{margin:0;text-align:center}figcaption{margin-top:8px}img{border-radius:6px;box-shadow:0 1px 4px rgba(0,0,0,.18)}</style>${cells}`,
  );
  await page.screenshot({ path: path.join(PREVIEW, `sheet-${name}.png`) });
  await page.close();
}
await sheetBrowser.close();

const list = (buffer) => {
  const sizes = [];
  for (let i = 0; i < buffer.readUInt16LE(4); i += 1) {
    const w = buffer.readUInt8(6 + i * 16);
    sizes.push(w === 0 ? 256 : w);
  }
  return sizes;
};
console.log(`ICO 尺寸：${list(ico).join(" / ")}（${ico.length} 字节）`);
console.log(`ICNS 条目：${icns.readUInt32BE(4)} 字节，含 16/32/64/128/256/512/1024`);
console.log(`图标目录：${ICONS}`);
console.log(`预览图：${PREVIEW}`);
