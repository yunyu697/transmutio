/**
 * 配色对比度校验：把设计令牌里的 palettes 转译后逐一算 WCAG 对比度。
 * 目标：每套配色 × 浅色/深色两种基调下，正文与背景都可读，不出现浅字浅底。
 *   - 主文字 / 背景、卡片 ≥ 7
 *   - 次要文字 / 卡片 ≥ 5
 *   - 弱化文字 / 卡片 ≥ 4.5
 *   - 强调色、成功/警告/错误 / 卡片 ≥ 4.5
 * 用法：node scripts/theme-contrast-check.mjs
 */
import { readFile, writeFile, mkdir, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import path from "node:path";
import { pathToFileURL } from "node:url";
import ts from "typescript";

const root = path.resolve(import.meta.dirname, "..");
const source = await readFile(path.join(root, "packages/design-tokens/src/index.ts"), "utf8");
// 只做语法转译（去类型 / 枚举），不解析依赖：令牌文件本身无运行时依赖。
const { outputText } = ts.transpileModule(source, {
  compilerOptions: { module: ts.ModuleKind.ESNext, target: ts.ScriptTarget.ES2022 },
});
const outDir = path.join(tmpdir(), "transmutio-token-check");
await mkdir(outDir, { recursive: true });
const outFile = path.join(outDir, "tokens.mjs");
await writeFile(outFile, outputText, "utf8");
const { palettes } = await import(pathToFileURL(outFile).href);
await rm(outFile, { force: true });

function channel(value) {
  const c = value / 255;
  return c <= 0.03928 ? c / 12.92 : ((c + 0.055) / 1.055) ** 2.4;
}

function luminance(hex) {
  const raw = hex.replace("#", "");
  const full = raw.length === 3 ? raw.split("").map((c) => c + c).join("") : raw;
  const r = parseInt(full.slice(0, 2), 16);
  const g = parseInt(full.slice(2, 4), 16);
  const b = parseInt(full.slice(4, 6), 16);
  return 0.2126 * channel(r) + 0.7152 * channel(g) + 0.0722 * channel(b);
}

function ratio(fg, bg) {
  const a = luminance(fg);
  const b = luminance(bg);
  return (Math.max(a, b) + 0.05) / (Math.min(a, b) + 0.05);
}

const CHECKS = [
  { key: "fg", against: "bg", min: 7, label: "主文字/页面背景" },
  { key: "fg", against: "surf", min: 7, label: "主文字/卡片" },
  { key: "fg2", against: "surf", min: 5, label: "次要文字/卡片" },
  { key: "fg2", against: "bg", min: 5, label: "次要文字/页面背景" },
  { key: "fg3", against: "surf", min: 4.5, label: "弱化文字/卡片" },
  { key: "accent", against: "surf", min: 4.5, label: "强调色/卡片" },
  { key: "ok", against: "surf", min: 4.5, label: "成功色/卡片" },
  { key: "warn", against: "surf", min: 4.5, label: "警告色/卡片" },
  { key: "danger", against: "surf", min: 4.5, label: "错误色/卡片" },
];

const rows = [];
let failures = 0;
for (const [id, definition] of Object.entries(palettes)) {
  for (const mode of ["dark", "light"]) {
    const colors = definition[mode];
    for (const check of CHECKS) {
      const value = ratio(colors[check.key], colors[check.against]);
      const pass = value >= check.min;
      if (!pass) failures += 1;
      rows.push({
        palette: `${id}/${mode}`,
        pair: check.label,
        value: Number(value.toFixed(2)),
        min: check.min,
        pass,
        colors: `${colors[check.key]} on ${colors[check.against]}`,
      });
    }
  }
}

const width = Math.max(...rows.map((row) => row.palette.length));
for (const row of rows) {
  const flag = row.pass ? "PASS" : "FAIL";
  console.log(
    `${row.palette.padEnd(width)}  ${flag}  ${String(row.value).padStart(5)} / ${row.min}  ${row.pair}  (${row.colors})`,
  );
}
console.log(
  `\n${rows.filter((row) => row.pass).length} PASS / ${failures} FAIL · ${Object.keys(palettes).length} 套配色 × 2 种亮度`,
);
process.exitCode = failures ? 1 : 0;
