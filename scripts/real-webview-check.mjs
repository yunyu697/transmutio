#!/usr/bin/env node
/**
 * 真机验收（真实 WebView2 窗口，非 headless）：
 *   1) 启动首帧：分别以浅色 / 深色保存态冷启动，用 PrintWindow 逐帧采样，确认没有闪白或跳色；
 *   2) 启动形态：左栏一层分区入口 + 主区单一方法列表；
 *   3) 视频面板：默认露三组常用参数，其余六组收起，点标题可展开；
 *   4) 底部队列条：在状态栏上方、约 44px，点开抽屉再点收起；
 *   5) 输出位置：自定义目录后出现「使用源文件夹」，一键切回后按钮消失；
 *   6) 主题：明暗 × 配色任意组合即时生效，重启后保持。
 *
 * 为什么不在 CI：需要真实窗口、真实 GPU/WebView2 与本机 LGPL 构建的 FFmpeg，
 * CI 的 headless Edge 只能覆盖 UI 层（`verify:ui` / panel-persist）。
 *
 * 前置：`npm run build:lgpl`（产物 src-tauri/target/release/Transmutio.exe）。
 * 用法：
 *   npm run verify:real                 # 全量（含启动首帧）
 *   npm run verify:real -- --no-first-frame   # 跳过启动首帧采样
 * 环境变量：
 *   APP_EXE     指定被测可执行文件
 *   CDP_PORT    WebView2 调试端口（默认 9455，避开 verify:app 的 9333）
 *   VERIFY_FFMPEG  引导页注入用的 LGPL ffmpeg 路径；缺省时在 samples/_dl 下自动查找
 */
import { spawn, spawnSync } from "node:child_process";
import fs from "node:fs";
import path from "node:path";
import { createRequire } from "node:module";
import { fileURLToPath } from "node:url";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const require = createRequire(path.join(root, "package.json"));
const { chromium } = require("playwright-core");

const OUT = path.join(root, "samples", "real-webview");
const PORT = Number(process.env.CDP_PORT || 9455);
const skipFirstFrame = process.argv.includes("--no-first-frame") || process.env.SKIP_FIRST_FRAME === "1";
const exe = process.env.APP_EXE || path.join(root, "src-tauri", "target", "release", process.platform === "win32" ? "Transmutio.exe" : "Transmutio");
const processName = path.basename(exe).replace(/\.exe$/i, "");

fs.mkdirSync(OUT, { recursive: true });

const results = [];
function check(id, label, ok, detail) {
  results.push({ id, label, ok, detail });
  console.log(`${ok ? "PASS" : "FAIL"} | ${id} | ${label} | ${detail}`);
}
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

function parseRgb(value) {
  const text = String(value).trim();
  const hex = text.match(/^#([0-9a-f]{6})$/i);
  if (hex) {
    const n = parseInt(hex[1], 16);
    return { r: (n >> 16) & 255, g: (n >> 8) & 255, b: n & 255 };
  }
  const m = text.match(/rgba?\(([^)]+)\)/);
  if (!m) return null;
  const parts = m[1].split(",").map((n) => Number(n.trim()));
  return { r: parts[0], g: parts[1], b: parts[2] };
}
function luminance({ r, g, b }) {
  const f = (c) => {
    const v = c / 255;
    return v <= 0.03928 ? v / 12.92 : ((v + 0.055) / 1.055) ** 2.4;
  };
  return 0.2126 * f(r) + 0.7152 * f(g) + 0.0722 * f(b);
}
function contrast(a, b) {
  const la = luminance(a);
  const lb = luminance(b);
  return (Math.max(la, lb) + 0.05) / (Math.min(la, lb) + 0.05);
}

/** 引导页注入用：优先环境变量，其次 samples/_dl 下带 lgpl 的 ffmpeg。 */
function findLgplFfmpeg() {
  if (process.env.VERIFY_FFMPEG) return process.env.VERIFY_FFMPEG;
  const base = path.join(root, "samples", "_dl");
  if (!fs.existsSync(base)) return null;
  const stack = [base];
  while (stack.length) {
    const dir = stack.pop();
    for (const entry of fs.readdirSync(dir, { withFileTypes: true })) {
      const full = path.join(dir, entry.name);
      if (entry.isDirectory()) stack.push(full);
      else if (/^ffmpeg(\.exe)?$/i.test(entry.name) && /lgpl/i.test(full)) return full;
    }
  }
  return null;
}
const ffmpegPath = findLgplFfmpeg();

let app = null;
async function startApp() {
  if (!fs.existsSync(exe)) {
    console.error(`找不到被测产物：${exe}\n请先执行 npm run build:lgpl`);
    process.exit(2);
  }
  // 单实例：上一轮残留的窗口会让新进程直接转发退出，调试端口不会打开
  if (process.platform === "win32") spawnSync("taskkill", ["/IM", `${processName}.exe`, "/F"], { stdio: "ignore" });
  app = spawn(exe, [], {
    env: {
      ...process.env,
      WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS: `--remote-debugging-port=${PORT}`,
    },
    stdio: ["ignore", "pipe", "pipe"],
  });
  app.stdout.on("data", () => {});
  app.stderr.on("data", () => {});
  for (let i = 0; i < 120; i += 1) {
    try {
      const targets = await (await fetch(`http://127.0.0.1:${PORT}/json/list`)).json();
      if (targets.some((t) => t.type === "page")) return;
    } catch {
      /* 还没起来 */
    }
    await sleep(500);
  }
  throw new Error(`WebView2 调试端口 ${PORT} 不可用`);
}
async function stopApp() {
  if (app) {
    app.kill();
    await sleep(800);
    if (app.exitCode === null) app.kill("SIGKILL");
    app = null;
  }
  // 单实例转发时上面拿到的是转发进程，真正持有窗口的实例要按镜像名收掉
  if (process.platform === "win32") spawnSync("taskkill", ["/IM", `${processName}.exe`, "/F"], { stdio: "ignore" });
  await sleep(400);
}

async function connectPage() {
  const browser = await chromium.connectOverCDP(`http://127.0.0.1:${PORT}`);
  const context = browser.contexts()[0];
  const page = context.pages().find((p) => !p.url().startsWith("devtools://")) ?? context.pages()[0];
  if (!page) throw new Error("没有找到 WebView2 页面");
  return { browser, page };
}

/** 首次启动若停在引导页（本机 PATH 的 FFmpeg 是 GPL 构建），注入本机 LGPL 构建后重载。 */
async function ensureMainUi(page) {
  if (await page.locator(".sidenav").count()) return;
  console.log(`WIZARD=1，注入 LGPL ffmpeg${ffmpegPath ? `：${ffmpegPath}` : "路径（未找到，可能继续停在引导页）"}`);
  if (ffmpegPath) {
    await page.evaluate((ffmpeg) => {
      const raw = localStorage.getItem("transmutio:settings");
      const payload = raw ? JSON.parse(raw) : { version: 1, data: {} };
      payload.data = { ...(payload.data ?? {}), ffmpegPath: ffmpeg };
      localStorage.setItem("transmutio:settings", JSON.stringify(payload));
    }, ffmpegPath);
  }
  await page.reload({ waitUntil: "domcontentloaded" });
}

async function waitForUi(page) {
  await page.waitForSelector(".sidenav", { timeout: 120000 });
}

/* ---------------- 0. 启动首帧（浅色 / 深色各一次冷启动） ---------------- */
const startup = [];
async function seedTheme(brightness, palette) {
  await startApp();
  const { page } = await connectPage();
  try {
    await ensureMainUi(page);
    await waitForUi(page);
    await page.evaluate(
      ([b, p]) => {
        const raw = localStorage.getItem("transmutio:settings");
        const payload = raw ? JSON.parse(raw) : { version: 1, data: {} };
        payload.data = { ...(payload.data ?? {}), brightness: b, palette: p };
        localStorage.setItem("transmutio:settings", JSON.stringify(payload));
      },
      [brightness, palette],
    );
    await page.reload({ waitUntil: "domcontentloaded" });
    await waitForUi(page);
    // 等 set_window_chrome 落盘，下一次冷启动才会读到新配色
    await sleep(1800);
  } finally {
    await stopApp();
  }
}

function runStartupFrame(label, mode) {
  const ps1 = path.join(root, "scripts", "startup-frame-check.ps1");
  const probe = spawnSync("pwsh", ["-NoProfile", "-Command", "exit 0"], { encoding: "utf8" });
  const candidates = probe.error ? ["powershell"] : ["pwsh"];
  let run = null;
  for (const cmd of candidates) {
    run = spawnSync(
      cmd,
      ["-NoProfile", "-ExecutionPolicy", "Bypass", "-File", ps1, "-Label", label, "-Mode", mode, "-Exe", exe, "-OutDir", OUT, "-ProcessName", processName],
      { encoding: "utf8", timeout: 120000 },
    );
    if (!run.error) break;
  }
  if (!run || run.error) throw new Error(`无法执行启动帧采样脚本：${run?.error?.message ?? "未知错误"}`);
  const output = `${run.stdout || ""}${run.stderr || ""}`;
  const verdict = /VERDICT=(PASS|FAIL|UNKNOWN)(.*)/.exec(output);
  const first = /first t=(\d+)ms y=([\d.]+) center=([\d,]+)/.exec(output);
  const range = /y_range=([\d.]+)\.\.([\d.]+)/.exec(output);
  const frames = /frames=(\d+)/.exec(output);
  const record = {
    label,
    mode,
    pass: verdict?.[1] === "PASS",
    frames: frames ? Number(frames[1]) : 0,
    first: first ? { t: Number(first[1]), y: Number(first[2]), center: first[3] } : null,
    yRange: range ? [Number(range[1]), Number(range[2])] : null,
    detail: (verdict?.[1] === "PASS" ? "无越界帧" : verdict?.[2]?.trim() ?? "未取得结论") + (range ? `；y_range=${range[1]}..${range[2]}` : ""),
  };
  startup.push(record);
  check(
    `0.${startup.length}`,
    `${mode === "light" ? "浅色" : "深色"}启动无${mode === "light" ? "暗帧（闪白/跳色）" : "亮帧（跳色）"}`,
    record.pass,
    `首帧=${record.first ? `${record.first.t}ms y=${record.first.y} center=${record.first.center}` : "未采样"}，帧数=${record.frames}，${record.detail}`,
  );
  return record;
}

const firstFrameEnabled = !skipFirstFrame && process.platform === "win32" && fs.existsSync(path.join(root, "scripts", "startup-frame-check.ps1"));
if (!firstFrameEnabled) {
  console.log(skipFirstFrame ? "按参数跳过启动首帧采样" : "非 Windows 或缺少采样脚本，跳过启动首帧采样");
} else {
  console.log("阶段 A：启动首帧（浅色 / 深色各做一次冷启动采样）");
  await seedTheme("light", "ocean");
  runStartupFrame("light", "light");
  await seedTheme("dark", "graphite");
  runStartupFrame("dark", "dark");
}

console.log("阶段 B：真实窗口功能验收");
await startApp();
const { browser, page } = await connectPage();
try {
  // 每次都从「重启后首屏」开始，避免上一轮停留在某个面板里
  await ensureMainUi(page);
  await waitForUi(page);
  await page.reload({ waitUntil: "domcontentloaded" });
  await waitForUi(page);
  await sleep(1500);

  const readTheme = () =>
    page.evaluate(() => {
      const cs = getComputedStyle(document.documentElement);
      const v = (name) => cs.getPropertyValue(name).trim();
      const root = document.documentElement;
      const body = getComputedStyle(document.body);
      return {
        theme: root.dataset.theme,
        palette: root.dataset.palette,
        colorScheme: root.style.colorScheme,
        bg: v("--ff-bg"),
        bg2: v("--ff-bg2"),
        surf: v("--ff-surf"),
        line: v("--ff-line"),
        fg: v("--ff-fg"),
        fg2: v("--ff-fg2"),
        fg3: v("--ff-fg3"),
        accent: v("--ff-accent"),
        bodyBg: body.backgroundColor,
        bodyFg: body.color,
      };
    });

  const storedSettings = () =>
    page.evaluate(() => {
      const raw = localStorage.getItem("transmutio:settings");
      const parsed = raw ? JSON.parse(raw) : null;
      return parsed?.data ?? parsed ?? {};
    });

  /* ---------------- 1. 启动形态：左栏一层分区 + 单一方法列表 ---------------- */
  const theme0 = await readTheme();
  const side = await page.evaluate(() => ({
    groups: document.querySelectorAll(".sidenav .side-group").length,
    labels: [...document.querySelectorAll(".sidenav .side-label")].map((n) => n.textContent.trim()),
    sections: [...document.querySelectorAll(".sidenav .grow")].map((n) => n.textContent.trim()),
    legacies: {
      subs: document.querySelectorAll(".sidenav .subs").length,
      sub: document.querySelectorAll(".sidenav .sub").length,
      ritem: document.querySelectorAll(".ritem").length,
      quick: document.querySelectorAll(".qcard").length,
      overviews: document.querySelectorAll(".overviews").length,
    },
    cards: document.querySelectorAll(".method-grid .card").length,
    title: document.querySelector(".method-grid .card h3")?.textContent ?? "",
    heading: document.querySelector(".page-head h1")?.textContent ?? "",
    navWidth: Math.round(document.querySelector(".sidenav").getBoundingClientRect().width),
  }));

  check("1.1", "启动默认主题", theme0.theme === "dark" && theme0.palette === "graphite", `${theme0.theme}/${theme0.palette}, --ff-bg=${theme0.bg}`);
  check(
    "1.2",
    "左栏只有一层分区入口",
    side.groups === 1 && side.sections.length === 3,
    `side-group=${side.groups}, labels=${JSON.stringify(side.labels)}, 分区=${JSON.stringify(side.sections)}`,
  );
  check("1.3", "旧入口已不存在", Object.values(side.legacies).every((n) => n === 0), JSON.stringify(side.legacies));
  check("1.4", "主区为单一方法列表", side.cards === 11 && side.heading === "视频", `标题=${side.heading}, 卡片=${side.cards}, 首个=${side.title}, 侧栏宽=${side.navWidth}px`);
  await page.screenshot({ path: path.join(OUT, "real-01-start.png") });

  /* ---------------- 2. 视频面板参数折叠 ---------------- */
  await page.locator(".method-grid .card", { hasText: "视频转码" }).first().click();
  await page.waitForSelector(".bottombar", { timeout: 15000 });
  await sleep(400);

  const foldInfo = await page.evaluate(() => {
    const sections = [...document.querySelectorAll(".group:not(.fold)")].map((n) => ({
      title: n.querySelector("header")?.textContent.trim() ?? "",
      h: Math.round(n.getBoundingClientRect().height),
    }));
    const folds = [...document.querySelectorAll(".group.fold")].map((n) => {
      const summary = n.querySelector("summary");
      return {
        title: summary?.textContent.trim() ?? "",
        open: n.open,
        groupH: Math.round(n.getBoundingClientRect().height),
        summaryH: Math.round(summary.getBoundingClientRect().height),
      };
    });
    return { sections, folds };
  });

  const allCollapsed = foldInfo.folds.every((f) => !f.open && Math.abs(f.groupH - f.summaryH) <= 2);
  check(
    "2.1",
    "默认只露常用项，其余折叠",
    foldInfo.sections.length === 3 && foldInfo.folds.length === 6 && allCollapsed,
    `展开=${foldInfo.sections.map((s) => s.title).join("/")}；折叠=${foldInfo.folds.map((f) => `${f.title}(${f.groupH}vs${f.summaryH})`).join("/")}`,
  );

  const firstFold = page.locator(".group.fold").first();
  const beforeH = await page.evaluate(() => {
    const n = document.querySelector(".group.fold");
    return { open: n.open, h: Math.round(n.getBoundingClientRect().height) };
  });
  await firstFold.locator("summary").click();
  await sleep(450);
  const afterH = await page.evaluate(() => {
    const n = document.querySelector(".group.fold");
    const summary = n.querySelector("summary");
    return { open: n.open, h: Math.round(n.getBoundingClientRect().height), s: Math.round(summary.getBoundingClientRect().height) };
  });
  check("2.2", "点折叠标题可展开", afterH.open && afterH.h > afterH.s + 20, `展开前 ${beforeH.h}px(open=${beforeH.open}) → 展开后 ${afterH.h}px(summary ${afterH.s}px)`);
  await page.screenshot({ path: path.join(OUT, "real-02-video-folds.png") });

  /* ---------------- 3. 底部队列条 + 抽屉 ---------------- */
  await page.locator(".group.fold").first().locator("summary").click();
  await sleep(300);

  const queue0 = await page.evaluate(() => {
    const bar = document.querySelector(".queuebar");
    const status = document.querySelector(".statusbar");
    const br = bar.getBoundingClientRect();
    const sr = status.getBoundingClientRect();
    return {
      height: Math.round(br.height),
      top: Math.round(br.top),
      bottom: Math.round(br.bottom),
      statusTop: Math.round(sr.top),
      drawer: document.querySelectorAll(".taskcenter").length,
      viewport: window.innerHeight,
    };
  });
  check(
    "3.1",
    "队列条在状态栏上方且约 44px",
    queue0.height >= 42 && queue0.height <= 46 && Math.abs(queue0.bottom - queue0.statusTop) <= 1 && queue0.drawer === 0,
    `高=${queue0.height}px, 底=${queue0.bottom}px, 状态栏顶=${queue0.statusTop}px, 窗口高=${queue0.viewport}px, 抽屉=${queue0.drawer}`,
  );

  await page.locator(".qb-main").click();
  await sleep(500);
  const queue1 = await page.evaluate(() => {
    const drawer = document.querySelector(".taskcenter");
    const bar = document.querySelector(".queuebar");
    return {
      h: drawer ? Math.round(drawer.getBoundingClientRect().height) : 0,
      barVisible: Boolean(bar && bar.getBoundingClientRect().height > 30),
      expanded: document.querySelector(".qb-main")?.getAttribute("aria-expanded"),
    };
  });
  check("3.2", "点队列条展开抽屉", queue1.h >= 200 && queue1.h <= 340 && queue1.barVisible && queue1.expanded === "true", `抽屉高=${queue1.h}px, 队列条仍在=${queue1.barVisible}`);
  await page.screenshot({ path: path.join(OUT, "real-03-queue-drawer.png") });

  await page.locator(".tc-head .iconbtn").click();
  await sleep(500);
  const queue2 = await page.evaluate(() => ({
    drawer: document.querySelectorAll(".taskcenter").length,
    expanded: document.querySelector(".qb-main")?.getAttribute("aria-expanded"),
  }));
  check("3.3", "再点收起抽屉", queue2.drawer === 0 && queue2.expanded === "false", `抽屉=${queue2.drawer}, aria-expanded=${queue2.expanded}`);

  /* ---------------- 4. 输出位置 ---------------- */
  const out0 = await page.evaluate(() => {
    const wrap = document.querySelector(".outdir");
    const code = wrap.querySelector("code");
    const start = document.querySelector(".bottombar .btn.primary");
    const wr = wrap.getBoundingClientRect();
    const sr = start.getBoundingClientRect();
    return {
      text: code.textContent.trim(),
      hasUseSource: [...wrap.querySelectorAll("button")].some((b) => b.textContent.includes("使用源文件夹")),
      buttons: [...wrap.querySelectorAll("button")].map((b) => b.textContent.trim()),
      outBottom: Math.round(wr.bottom),
      startTop: Math.round(sr.top),
    };
  });
  check("4.1", "默认显示与源文件相同目录", out0.text === "与源文件相同目录" && !out0.hasUseSource, `文本=${out0.text}, 按钮=${JSON.stringify(out0.buttons)}`);
  check("4.2", "输出行在开始按钮上方", out0.outBottom <= out0.startTop + 1, `输出行底=${out0.outBottom}px, 开始按钮顶=${out0.startTop}px`);

  await page.locator('.sibtn[title="设置"]').click();
  await page.waitForSelector(".drawer", { timeout: 8000 });
  await sleep(300);
  const customDir = path.join(OUT, "out");
  await page.locator(".drawer .radio", { hasText: "自定义目录" }).locator("input").check();
  await page.locator('.drawer input[placeholder="选择或填写输出目录"]').fill(customDir);
  await sleep(300);
  await page.locator(".drawer header .iconbtn").click();
  await sleep(400);

  const out1 = await page.evaluate(() => {
    const wrap = document.querySelector(".outdir");
    return {
      text: wrap.querySelector("code").textContent.trim(),
      buttons: [...wrap.querySelectorAll("button")].map((b) => b.textContent.trim()),
    };
  });
  check("4.3", "自定义目录后出现「使用源文件夹」", out1.text === customDir && out1.buttons.includes("使用源文件夹"), `文本=${out1.text}, 按钮=${JSON.stringify(out1.buttons)}`);
  await page.screenshot({ path: path.join(OUT, "real-04-output-custom.png") });

  await page.locator(".outdir button", { hasText: "使用源文件夹" }).click();
  await sleep(400);
  const out2 = await page.evaluate(() => {
    const wrap = document.querySelector(".outdir");
    const raw = localStorage.getItem("transmutio:settings");
    const data = raw ? (JSON.parse(raw).data ?? JSON.parse(raw)) : {};
    return {
      text: wrap.querySelector("code").textContent.trim(),
      buttons: [...wrap.querySelectorAll("button")].map((b) => b.textContent.trim()),
      mode: data.outputDirMode,
      keepCustom: data.outputDir,
    };
  });
  check(
    "4.4",
    "一键切回源文件夹且按钮消失",
    out2.text === "与源文件相同目录" && !out2.buttons.includes("使用源文件夹") && out2.mode === "source",
    `文本=${out2.text}, 按钮=${JSON.stringify(out2.buttons)}, mode=${out2.mode}, 保留的自定义目录=${out2.keepCustom}`,
  );
  await page.screenshot({ path: path.join(OUT, "real-05-output-source.png") });

  /* ---------------- 5. 主题正交组合 + 持久化 ---------------- */
  await page.locator('.sibtn[title="设置"]').click();
  await page.waitForSelector(".drawer", { timeout: 8000 });
  await sleep(300);

  await page.locator(".drawer .segmented button", { hasText: "浅色" }).click();
  await sleep(250);
  await page.locator(".drawer .palette", { hasText: "深海" }).click();
  await sleep(450);
  const lightOcean = await readTheme();
  const contrastLight = {
    fgOnBg: contrast(parseRgb(lightOcean.bodyFg), parseRgb(lightOcean.bodyBg)),
    fgOnSurf: contrast(parseRgb(lightOcean.bodyFg), parseRgb(lightOcean.surf)),
    fg3OnSurf: contrast(parseRgb(lightOcean.fg3), parseRgb(lightOcean.surf)),
  };
  check(
    "5.1",
    "浅色 + 深海即时生效",
    lightOcean.theme === "light" && lightOcean.palette === "ocean" && lightOcean.bg.toLowerCase() === "#e9f1fb",
    `data-theme=${lightOcean.theme}, data-palette=${lightOcean.palette}, bg=${lightOcean.bg}, surf=${lightOcean.surf}, accent=${lightOcean.accent}, colorScheme=${lightOcean.colorScheme}`,
  );
  check(
    "5.2",
    "浅色+深海实渲染对比度可读",
    contrastLight.fgOnBg >= 7 && contrastLight.fgOnSurf >= 7 && contrastLight.fg3OnSurf >= 4.5,
    `正文/底=${contrastLight.fgOnBg.toFixed(2)}, 正文/卡=${contrastLight.fgOnSurf.toFixed(2)}, 次要/卡=${contrastLight.fg3OnSurf.toFixed(2)}`,
  );
  await page.screenshot({ path: path.join(OUT, "real-06-ocean-light.png") });

  await page.locator(".drawer .segmented button", { hasText: "深色" }).click();
  await sleep(250);
  await page.locator(".drawer .palette", { hasText: "苔原" }).click();
  await sleep(450);
  const darkForest = await readTheme();
  const contrastDark = {
    fgOnBg: contrast(parseRgb(darkForest.bodyFg), parseRgb(darkForest.bodyBg)),
    fgOnSurf: contrast(parseRgb(darkForest.bodyFg), parseRgb(darkForest.surf)),
    fg3OnSurf: contrast(parseRgb(darkForest.fg3), parseRgb(darkForest.surf)),
  };
  check(
    "5.3",
    "深色 + 苔原即时生效（组合可换）",
    darkForest.theme === "dark" && darkForest.palette === "forest" && darkForest.bg.toLowerCase() === "#0a1210",
    `data-theme=${darkForest.theme}, data-palette=${darkForest.palette}, bg=${darkForest.bg}, surf=${darkForest.surf}, accent=${darkForest.accent}`,
  );
  check(
    "5.4",
    "深色+苔原实渲染对比度可读",
    contrastDark.fgOnBg >= 7 && contrastDark.fgOnSurf >= 7 && contrastDark.fg3OnSurf >= 4.5,
    `正文/底=${contrastDark.fgOnBg.toFixed(2)}, 正文/卡=${contrastDark.fgOnSurf.toFixed(2)}, 次要/卡=${contrastDark.fg3OnSurf.toFixed(2)}`,
  );
  await page.screenshot({ path: path.join(OUT, "real-07-forest-dark.png") });

  await page.locator(".drawer .segmented button", { hasText: "浅色" }).click();
  await sleep(250);
  await page.locator(".drawer .palette", { hasText: "暖砂" }).click();
  await sleep(450);
  const lightAmber = await readTheme();
  check(
    "5.5",
    "浅色 + 暖砂（第三种组合）",
    lightAmber.theme === "light" && lightAmber.palette === "amber" && lightAmber.bg.toLowerCase() === "#f7f1e8",
    `data-theme=${lightAmber.theme}, data-palette=${lightAmber.palette}, bg=${lightAmber.bg}, surf=${lightAmber.surf}, accent=${lightAmber.accent}`,
  );

  // 回到 深色 + 苔原，重启验证持久化
  await page.locator(".drawer .segmented button", { hasText: "深色" }).click();
  await sleep(250);
  await page.locator(".drawer .palette", { hasText: "苔原" }).click();
  await sleep(450);
  await page.locator(".drawer header .iconbtn").click();
  await sleep(300);

  const beforeReload = await readTheme();
  const persisted = await storedSettings();
  await page.reload({ waitUntil: "domcontentloaded" });
  await waitForUi(page);
  await sleep(1200);
  const afterReload = await readTheme();
  check(
    "5.6",
    "主题设置重启后保持",
    persisted.brightness === "dark" &&
      persisted.palette === "forest" &&
      afterReload.theme === "dark" &&
      afterReload.palette === "forest" &&
      afterReload.bg.toLowerCase() === beforeReload.bg.toLowerCase(),
    `存储=${persisted.brightness}/${persisted.palette}, reload 前=${beforeReload.theme}/${beforeReload.palette} ${beforeReload.bg}, reload 后=${afterReload.theme}/${afterReload.palette} ${afterReload.bg}`,
  );

  const persistedOutput = await storedSettings();
  check("5.7", "输出目录设置重启后保持 source", persistedOutput.outputDirMode === "source" && persistedOutput.outputDir === customDir, `mode=${persistedOutput.outputDirMode}, custom=${persistedOutput.outputDir}`);

  await page.screenshot({ path: path.join(OUT, "real-08-after-reload.png") });

  // 复原为默认深色 + 石墨，避免把验证用的配色留在用户设置里
  await page.locator('.sibtn[title="设置"]').click();
  await page.waitForSelector(".drawer", { timeout: 8000 });
  await sleep(300);
  await page.locator(".drawer .palette", { hasText: "石墨" }).click();
  await sleep(350);
  await page.locator(".drawer header .iconbtn").click();
  await sleep(300);
  const restored = await readTheme();
  console.log(`RESTORED ${restored.theme}/${restored.palette} ${restored.bg}`);

  const pass = results.filter((r) => r.ok).length;
  const fail = results.length - pass;
  fs.writeFileSync(
    path.join(OUT, "real-webview-report.json"),
    JSON.stringify({ generatedAt: new Date().toISOString(), exe, platform: process.platform, startup, pass, fail, results }, null, 2),
  );
  console.log(`\n合计 ${pass} PASS / ${fail} FAIL，报告 samples/real-webview/real-webview-report.json`);
  await browser.close().catch(() => {});
  process.exitCode = fail ? 1 : 0;
} finally {
  await stopApp();
}
