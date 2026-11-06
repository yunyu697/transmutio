/**
 * 首次启动引导页的无头验收（UI 层，headless 浏览器 + mock 后端）。
 *
 * 只验证前端渲染与交互：不启动 Tauri 窗口，不驱动真实任务核心。
 * 真实落位与转码由 `npm run verify:setup`（Rust，真实 ffmpeg + 真实文件系统）覆盖。
 * 因此本报告不能称作应用级 E2E。
 */
import { createServer } from "node:http";
import { readFile, mkdir, writeFile } from "node:fs/promises";
import path from "node:path";
import { chromium } from "playwright-core";

const DIST = path.resolve("dist");
const SAMPLE_DIR = path.resolve("samples");
const LEVEL = "UI 层（headless 浏览器 + mock 后端），非应用级 E2E";

const MIME = {
  ".html": "text/html; charset=utf-8",
  ".js": "text/javascript; charset=utf-8",
  ".css": "text/css; charset=utf-8",
  ".svg": "image/svg+xml",
  ".png": "image/png",
  ".json": "application/json",
};

function startServer() {
  return new Promise((resolve) => {
    const server = createServer(async (req, res) => {
      const urlPath = decodeURIComponent((req.url || "/").split("?")[0]);
      const relative = urlPath === "/" ? "index.html" : urlPath.replace(/^\//, "");
      const file = path.join(DIST, relative);
      try {
        const body = await readFile(file);
        res.writeHead(200, { "content-type": MIME[path.extname(file)] || "application/octet-stream" });
        res.end(body);
      } catch {
        res.writeHead(404);
        res.end("not found");
      }
    });
    server.listen(0, "127.0.0.1", () => resolve(server));
  });
}

const baseTools = (over = {}) => ({
  ffmpeg: null,
  ffprobe: null,
  ffmpegVersion: null,
  ffprobeVersion: null,
  build: { version: "", configuration: "", gpl: false, nonfree: false, lgplOk: true, notes: [] },
  platform: "windows",
  arch: "x86_64",
  targetTriple: "x86_64-pc-windows-msvc",
  warnings: [],
  searchNote: "",
  ...over,
});

const baseCaps = (over = {}) => ({
  probedAt: 1793797000000,
  cacheHit: false,
  fingerprint: "mock",
  platform: "windows",
  arch: "x86_64",
  ffmpegVersion: "",
  buildGpl: false,
  buildNonfree: false,
  videoEncoders: [],
  audioEncoders: [],
  decodeAccels: [],
  softwareH264Available: false,
  subtitleBurn: false,
  notes: [],
  ...over,
});

const LGPL_PATH = "C:\\Users\\Tyler\\Downloads\\ffmpeg-lgpl\\bin\\ffmpeg.exe";
const NONFREE_PATH = "C:\\Users\\Tyler\\Downloads\\ffmpeg-nonfree\\bin\\ffmpeg.exe";
const GPL_PATH = "C:\\Users\\Tyler\\Downloads\\ffmpeg-gpl\\bin\\ffmpeg.exe";
const INSTALLED_DIR = "C:\\Users\\Tyler\\AppData\\Roaming\\app.transmutio.desktop\\tools\\ffmpeg-N-128352-lgpl";

const FLAVOR_LGPL = {
  id: "lgpl",
  productName: "Transmutio",
  appIdentifier: "app.transmutio.desktop",
  updateChannel: "transmutio-lgpl-stable",
  defaultSourceUrl: "https://ffmpeg.org/download.html",
  licenseLabel: "LGPL-2.1",
  allowGpl: false,
  allowGplCompiled: false,
  requireGplBuild: false,
  devOverrideAvailable: true,
  debug: true,
};

const FLAVOR_GPL = {
  id: "gpl",
  productName: "Transmutio GPL",
  appIdentifier: "app.transmutio.desktop.gpl",
  updateChannel: "transmutio-gpl-stable",
  defaultSourceUrl: "https://www.gyan.dev/ffmpeg/builds/",
  licenseLabel: "GPL-2.0-or-later（以所选 FFmpeg 构建为准）",
  allowGpl: true,
  allowGplCompiled: true,
  requireGplBuild: true,
  devOverrideAvailable: false,
  debug: true,
};

const lgplInspection = {
  path: LGPL_PATH,
  exists: true,
  size: 142336000,
  sha256: "a".repeat(64),
  version: "N-128352-gec887b13a8-20261103",
  configuration: "--disable-libx264 --disable-libx265 --enable-libopenh264 --enable-libmp3lame",
  gpl: false,
  nonfree: false,
  lgplOk: true,
  hasH264Encoder: true,
  hasAacEncoder: true,
  notes: [],
};

const nonfreeInspection = {
  ...lgplInspection,
  path: NONFREE_PATH,
  version: "9.0.2-full_build",
  configuration: "--enable-gpl --enable-nonfree",
  gpl: true,
  nonfree: true,
  lgplOk: false,
  notes: ["包含 nonfree 组件：默认路线禁止使用"],
};

const nonfreeTools = baseTools({
  ffmpeg: { path: NONFREE_PATH, source: "path", exists: true },
  ffmpegVersion: "9.0.2-full_build",
  build: {
    version: "9.0.2-full_build",
    configuration: "--enable-gpl --enable-nonfree",
    gpl: true,
    nonfree: true,
    lgplOk: false,
    notes: ["包含 nonfree 组件：默认路线禁止使用"],
  },
});

const installedTools = baseTools({
  ffmpeg: { path: `${INSTALLED_DIR}\\ffmpeg.exe`, source: "app-data", exists: true },
  ffprobe: { path: `${INSTALLED_DIR}\\ffprobe.exe`, source: "app-data", exists: true },
  ffmpegVersion: "N-128352-gec887b13a8-20261103",
  build: {
    version: "N-128352-gec887b13a8-20261103",
    configuration: "--disable-libx264 --enable-libopenh264",
    gpl: false,
    nonfree: false,
    lgplOk: true,
    notes: [],
  },
});

const gplInspection = {
  ...lgplInspection,
  path: GPL_PATH,
  version: "9.0.2-full_build",
  configuration: "--enable-gpl --enable-version3 --enable-libx264 --enable-libx265",
  gpl: true,
  nonfree: false,
  lgplOk: false,
  notes: ["GPL 构建：符合 GPL 版（Transmutio GPL）的许可要求"],
};

const installedGplTools = baseTools({
  ffmpeg: { path: `${INSTALLED_DIR}-gpl\\ffmpeg.exe`, source: "app-data", exists: true },
  ffprobe: { path: `${INSTALLED_DIR}-gpl\\ffprobe.exe`, source: "app-data", exists: true },
  ffmpegVersion: "9.0.2-full_build",
  build: {
    version: "9.0.2-full_build",
    configuration: "--enable-gpl --enable-version3 --enable-libx264",
    gpl: true,
    nonfree: false,
    lgplOk: false,
    notes: [],
  },
});

/** 在页面脚本之前注入 Tauri IPC 模拟层。 */
async function mockTauri(page, cfg) {
  // 变体对象在页面里不可见，必须先序列化进 config。
  const withDefaults = { flavor: FLAVOR_LGPL, ...cfg };
  await page.addInitScript((config) => {
    const calls = [];
    window.__FF_MOCK__ = { calls };
    const callbacks = new Map();
    const state = { installed: false };
    let callbackSeq = 0;
    const immediate = (value) => Promise.resolve(value);

    window.__TAURI_EVENT_PLUGIN_INTERNALS__ = { unregisterListener() {} };
    window.__TAURI_INTERNALS__ = {
      metadata: {
        currentWindow: { label: "main" },
        currentWebview: { windowLabel: "main", label: "main" },
      },
      transformCallback(callback) {
        const id = ++callbackSeq;
        callbacks.set(id, callback);
        return id;
      },
      unregisterCallback(id) {
        callbacks.delete(id);
      },
      convertFileSrc: (p) => p,
      invoke(cmd, args) {
        calls.push({ cmd, args });
        switch (cmd) {
          case "resolve_tools":
            return immediate(state.installed && config.toolsAfterInstall ? config.toolsAfterInstall : config.tools);
          case "flavor_info":
            return immediate(config.flavor);
          case "probe_capabilities":
            return immediate(config.caps);
          case "default_output_dir":
            return immediate("C:\\\\Users\\\\Tyler\\\\Videos\\\\Transmutio");
          case "list_jobs":
            return immediate([]);
          case "set_job_concurrency":
            return immediate(args?.value ?? 4);
          case "plugin:dialog|open":
            return immediate(config.dialogReturn ?? null);
          case "inspect_ffmpeg_file": {
            const list = config.inspections ?? [config.inspection];
            const idx = calls.filter((c) => c.cmd === "inspect_ffmpeg_file").length - 1;
            return immediate(list[Math.min(idx, list.length - 1)]);
          }
          case "install_ffmpeg_files":
            state.installed = true;
            return immediate(config.installReturn);
          case "plugin:event|listen":
            return immediate(1);
          case "plugin:event|unlisten":
            return immediate(null);
          case "plugin:opener|open_url":
            return immediate(null);
          default:
            return immediate(null);
        }
      },
    };
  }, withDefaults);
}

async function newPage(browser, origin, cfg) {
  const page = await browser.newPage({ viewport: { width: 1440, height: 960 } });
  const pageErrors = [];
  page.on("pageerror", (error) => pageErrors.push(String(error)));
  await mockTauri(page, cfg);
  await page.goto(origin, { waitUntil: "domcontentloaded" });
  return { page, pageErrors };
}

const text = async (page, selector) => (await page.locator(selector).first().innerText()).trim();
const count = (page, selector) => page.locator(selector).count();

async function scenarioMissing(browser, origin, results) {
  const cfg = {
    tools: baseTools(),
    caps: baseCaps(),
    inspection: lgplInspection,
    installReturn: installedTools,
    toolsAfterInstall: installedTools,
    dialogReturn: LGPL_PATH,
  };
  const { page, pageErrors } = await newPage(browser, origin, cfg);
  const add = (name, ok, detail) =>
    results.push({ scenario: "missing-ffmpeg", name, status: ok ? "pass" : "fail", detail, evidence: { source: "DOM", observed: detail } });

  await page.waitForSelector(".setup", { timeout: 10000 });
  const setupBox = await page.locator(".setup").boundingBox();
  add(
    "引导页全屏接管内容区",
    Boolean(setupBox && setupBox.width > 900 && setupBox.height > 400) &&
      (await count(page, ".sidenav")) === 0 &&
      (await count(page, ".main")) === 0 &&
      (await count(page, ".taskcenter")) === 0,
    `setup 尺寸 ${setupBox?.width}x${setupBox?.height}，侧栏/方法网格/任务中心均未渲染`,
  );

  const title = await text(page, ".setup-head h1");
  add("缺失标题文案", title === "第一次使用：先准备 FFmpeg", title);
  const note = await text(page, ".setup-head + .note");
  add("缺失原因文案", note.includes("未找到可用的 ffmpeg / ffprobe"), note);

  const sourceInput = page.locator(".setup-steps > li:nth-child(2) .setup-row input");
  const placeholder = await sourceInput.getAttribute("placeholder");
  add("下载源输入可配置（默认值）", placeholder === "https://ffmpeg.org/download.html", `placeholder=${placeholder}`);

  const customUrl = "https://example.org/ffmpeg-lgpl-builds";
  await sourceInput.fill(customUrl);
  const stored = await page.evaluate(() => globalThis.localStorage.getItem("transmutio:settings") || "");
  add("下载源修改后持久化", stored.includes(customUrl), `localStorage 已写入自定义地址=${stored.includes(customUrl)}`);

  await page.locator(".setup-steps > li:nth-child(2) .setup-row button").click();
  const openerCall = await page
    .waitForFunction(
      () => window.__FF_MOCK__.calls.filter((call) => call.cmd === "plugin:opener|open_url").pop(),
      undefined,
      { timeout: 5000 },
    )
    .then((handle) => handle.jsonValue())
    .catch(() => null);
  add(
    "打开下载页走系统浏览器（IPC）",
    openerCall?.args?.url === customUrl,
    JSON.stringify(openerCall?.args ?? null) +
      (await page.evaluate(() => document.querySelector(".setup-card > .note.danger")?.textContent ?? "")),
  );

  await page.locator(".setup-steps > li:nth-child(3) .setup-row button").click();
  await page.waitForSelector(".setup-inspect", { timeout: 10000 });
  const inspectBox = await text(page, ".setup-inspect");
  add("选文件后展示校验结果", inspectBox.includes("LGPL") && inspectBox.includes("SHA-256"), inspectBox.split("\n").slice(0, 6).join(" / "));
  add("展示真实 SHA-256 值", inspectBox.includes("a".repeat(64)), "64 位哈希已渲染");

  const installButton = page.locator(".setup-inspect button").filter({ hasText: /落位/ });
  add("LGPL 构建安装按钮可用", await installButton.isEnabled(), "enabled=true");

  await mkdir(SAMPLE_DIR, { recursive: true });
  await page.screenshot({ path: path.join(SAMPLE_DIR, "setup-ui-missing-before-install.png"), fullPage: true });

  await installButton.click();
  await page.waitForSelector(".setup", { state: "detached", timeout: 10000 });
  const installCall = await page.evaluate(() =>
    window.__FF_MOCK__.calls.filter((call) => call.cmd === "install_ffmpeg_files").pop(),
  );
  add(
    "安装调用携带路径且默认不允许 GPL",
    installCall?.args?.ffmpegPath === LGPL_PATH && installCall?.args?.allowGpl === false,
    JSON.stringify(installCall?.args ?? null),
  );

  add("安装后离开引导页进入主界面", (await count(page, ".sidenav")) === 1, "侧栏已渲染");
  add("安装后任务中心恢复", (await count(page, ".taskcenter")) === 1, "任务中心已重新出现");
  const status = await text(page, ".statusbar");
  add("状态栏显示 FFmpeg 就绪", status.includes("FFmpeg"), status.replace(/\s+/g, " ").slice(0, 80));
  add(
    "状态栏标出当前变体与构建配置",
    status.includes("Transmutio · LGPL") && status.includes("FFmpeg 构建：LGPL"),
    status.replace(/\s+/g, " ").slice(0, 120),
  );

  const enqueued = await page.evaluate(() => window.__FF_MOCK__.calls.filter((c) => c.cmd === "enqueue_jobs").length);
  add("不伪造转换（未调用入队）", enqueued === 0, `enqueue_jobs 调用次数=${enqueued}`);
  add("页面无未捕获异常", pageErrors.length === 0, pageErrors.join("; ") || "无");

  await page.screenshot({ path: path.join(SAMPLE_DIR, "setup-ui-missing-after-install.png"), fullPage: true });
  await page.close();
  return ["setup-ui-missing-before-install.png", "setup-ui-missing-after-install.png"];
}

async function scenarioNonfree(browser, origin, results) {
  const cfg = {
    tools: nonfreeTools,
    caps: baseCaps({ buildGpl: true, buildNonfree: true }),
    inspection: nonfreeInspection,
    installReturn: installedTools,
    dialogReturn: NONFREE_PATH,
  };
  const { page, pageErrors } = await newPage(browser, origin, cfg);
  const add = (name, ok, detail) =>
    results.push({ scenario: "nonfree-present", name, status: ok ? "pass" : "fail", detail, evidence: { source: "DOM", observed: detail } });

  await page.waitForSelector(".setup", { timeout: 10000 });
  const title = await text(page, ".setup-head h1");
  add("已有不合规构建时标题", title === "当前 FFmpeg 不符合 LGPL 许可路线", title);
  const note = await text(page, ".setup-head + .note");
  add("说明为何被接管", note.includes("nonfree") && note.includes(NONFREE_PATH), note);

  await page.locator(".setup-steps > li:nth-child(3) .setup-row button").click();
  await page.waitForSelector(".setup-inspect", { timeout: 10000 });
  const inspectBox = await text(page, ".setup-inspect");
  add("nonfree 校验结果标红拒绝", inspectBox.includes("nonfree"), inspectBox.split("\n").slice(0, 5).join(" / "));

  const installButton = page.locator(".setup-inspect button").filter({ hasText: /落位/ });
  add("nonfree 时安装按钮禁用", await installButton.isDisabled(), "disabled=true");
  const errorNote = await text(page, ".setup-card > .note.danger");
  add("nonfree 拒绝原因可见", errorNote.includes("nonfree"), errorNote);
  const innerNote = await text(page, ".setup-inspect .note.danger");
  add("GPL 默认拒绝提示", innerNote.includes("GPL"), innerNote);

  add("提供仅本机开发的显式开关", (await count(page, ".setup-steps li:has-text('仅本机开发：允许 GPL')")) === 1, "开发覆盖入口存在");
  const status = await text(page, ".statusbar");
  add(
    "状态栏标注 nonfree 构建",
    status.includes("Transmutio · LGPL") && status.includes("FFmpeg 构建：nonfree"),
    status.replace(/\s+/g, " ").slice(0, 100),
  );

  const installed = await page.evaluate(() =>
    window.__FF_MOCK__.calls.filter((call) => call.cmd === "install_ffmpeg_files").length,
  );
  add("未发生任何落位", installed === 0, `install_ffmpeg_files 调用次数=${installed}`);
  const enqueued = await page.evaluate(() => window.__FF_MOCK__.calls.filter((c) => c.cmd === "enqueue_jobs").length);
  add("不伪造转换（未调用入队）", enqueued === 0, `enqueue_jobs 调用次数=${enqueued}`);
  add("页面无未捕获异常", pageErrors.length === 0, pageErrors.join("; ") || "无");

  await mkdir(SAMPLE_DIR, { recursive: true });
  await page.screenshot({ path: path.join(SAMPLE_DIR, "setup-ui-nonfree.png"), fullPage: true });
  await page.close();
  return "setup-ui-nonfree.png";
}

async function scenarioGplVariant(browser, origin, results) {
  const cfg = {
    flavor: FLAVOR_GPL,
    tools: baseTools(),
    caps: baseCaps(),
    inspections: [lgplInspection, gplInspection],
    installReturn: installedGplTools,
    toolsAfterInstall: installedGplTools,
    dialogReturn: GPL_PATH,
  };
  const { page, pageErrors } = await newPage(browser, origin, cfg);
  const add = (name, ok, detail) =>
    results.push({ scenario: "gpl-variant", name, status: ok ? "pass" : "fail", detail, evidence: { source: "DOM", observed: detail } });

  await page.waitForSelector(".setup", { timeout: 10000 });
  const title = await text(page, ".setup-head h1");
  add("GPL 版标题文案", title === "第一次使用：先准备 GPL 构建的 FFmpeg", title);
  const intro = await text(page, ".setup-head p");
  add("GPL 版说明文案", intro.includes("GPL 版") && intro.includes("libx264"), intro.replace(/\s+/g, " ").slice(0, 100));
  const placeholder = await page.locator(".setup-steps > li:nth-child(2) .setup-row input").getAttribute("placeholder");
  add("GPL 版默认下载源指向 gyan.dev", placeholder === "https://www.gyan.dev/ffmpeg/builds/", `placeholder=${placeholder}`);
  add(
    "GPL 版步骤标题区分",
    (await text(page, ".setup-steps > li:nth-child(1) > b")).includes("GPL 构建"),
    await text(page, ".setup-steps > li:nth-child(1) > b"),
  );
  add(
    "GPL 版合规文案区分",
    (await page.locator(".setup-card").innerText()).includes("THIRD-PARTY-NOTICES-GPL.txt"),
    "提及 GPL 专属 notices",
  );
  add(
    "GPL 版不展示 LGPL 开发开关",
    !(await page.locator(".setup-card").innerText()).includes("仅本机开发"),
    "无开发覆盖入口",
  );

  // 第一次选 LGPL 构建：应拒绝并禁用安装
  await page.locator(".setup-steps > li:nth-child(3) .setup-row button").click();
  await page.waitForSelector(".setup-inspect", { timeout: 10000 });
  const installButton = page.locator(".setup-inspect button").filter({ hasText: /落位/ });
  const rejectNote = await text(page, ".setup-inspect .note.danger");
  add(
    "GPL 版拒绝 LGPL 构建",
    (await installButton.isDisabled()) && rejectNote.includes("GPL 构建"),
    `disabled=${await installButton.isDisabled()} note=${rejectNote}`,
  );

  // 第二次选 GPL 构建：应放行，且安装调用固定 allowGpl=true
  await page.locator(".setup-steps > li:nth-child(3) .setup-row button").click();
  await page.waitForFunction(
    () => document.querySelector(".setup-inspect .setup-kv b")?.textContent?.includes("GPL"),
    undefined,
    { timeout: 10000 },
  );
  add("GPL 版接受 GPL 构建", await installButton.isEnabled(), "安装按钮可用");

  await mkdir(SAMPLE_DIR, { recursive: true });
  await page.screenshot({ path: path.join(SAMPLE_DIR, "setup-ui-gpl.png"), fullPage: true });

  await installButton.click();
  await page.waitForSelector(".setup", { state: "detached", timeout: 10000 });
  const installCall = await page.evaluate(() =>
    window.__FF_MOCK__.calls.filter((call) => call.cmd === "install_ffmpeg_files").pop(),
  );
  add(
    "GPL 版安装调用固定 allowGpl=true",
    installCall?.args?.allowGpl === true,
    JSON.stringify(installCall?.args ?? null),
  );
  const status = await text(page, ".statusbar");
  add(
    "GPL 版状态栏标出变体与 GPL 构建",
    status.includes("Transmutio GPL · GPL") && status.includes("FFmpeg 构建：GPL"),
    status.replace(/\s+/g, " ").slice(0, 120),
  );
  add("GPL 版安装后任务中心恢复", (await count(page, ".taskcenter")) === 1, "任务中心已重新出现");
  add("页面无未捕获异常", pageErrors.length === 0, pageErrors.join("; ") || "无");
  await page.close();
  return "setup-ui-gpl.png";
}

const browser = await chromium.launch({ channel: "msedge", headless: true });
const server = await startServer();
const origin = `http://127.0.0.1:${server.address().port}/`;
const results = [];
const screenshots = [];

try {
  screenshots.push(...(await scenarioMissing(browser, origin, results)));
  screenshots.push(await scenarioNonfree(browser, origin, results));
  screenshots.push(await scenarioGplVariant(browser, origin, results));
} finally {
  await browser.close();
  server.close();
}

const passed = results.filter((r) => r.status === "pass").length;
const failed = results.filter((r) => r.status === "fail").length;
const report = {
  generatedAt: Date.now(),
  level: LEVEL,
  environment: {
    browser: "Microsoft Edge (headless, channel=msedge)",
    viewport: "1440x960",
    target: "dist/ 前端构建产物",
    backend: "mock（window.__TAURI_INTERNALS__.invoke 由脚本注入）",
    commands: ["npm run build", "node scripts/setup-ui-check.mjs"],
    note: "本报告只证明引导页的渲染与交互逻辑；不证明真实落位/转码，不代表应用级 E2E",
  },
  summary: { passed, failed, total: results.length },
  screenshots,
  results,
};

await mkdir(SAMPLE_DIR, { recursive: true });
await writeFile(path.join(SAMPLE_DIR, "setup-ui-report.json"), JSON.stringify(report, null, 2), "utf8");
console.log(`引导页 UI 检查完成：${passed} PASS / ${failed} FAIL（报告：samples/setup-ui-report.json）`);
process.exit(failed === 0 ? 0 : 1);
