/**
 * 面板参数持久化的无头验收（UI 层，headless 浏览器 + mock 后端）。
 *
 * 验证三件事：
 *  1. 在视频 / 音频 / 图片面板里改参数与命名策略后，localStorage 立即写入；
 *  2. 同一 WebView 重新加载（最接近「重启应用」的 headless 近似）后，控件值恢复；
 *  3. 全新页面在应用脚本执行前注入 localStorage（模拟上一次会话留下的配置），
 *     面板直接渲染出恢复值。
 *
 * 只验证前端渲染与存储逻辑：不启动 Tauri 窗口、不驱动真实任务核心。
 * 真实转码由 `npm run verify:core` 覆盖。
 */
import { createServer } from "node:http";
import { readFile, mkdir, writeFile } from "node:fs/promises";
import path from "node:path";
import { chromium } from "playwright-core";

const DIST = path.resolve("dist");
const OUT_DIR = path.resolve("samples", "ui-overflow");
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

const encoder = (over) => ({
  id: "libopenh264",
  codec: "h264",
  label: "OpenH264",
  vendor: "cisco",
  hardware: false,
  license: "lgpl",
  allowed: true,
  compiled: true,
  available: true,
  reason: null,
  ...over,
});

const CAPS = {
  probedAt: 1793870000000,
  cacheHit: false,
  fingerprint: "mock-panel",
  platform: "windows",
  arch: "x86_64",
  ffmpegVersion: "N-128352",
  buildGpl: false,
  buildNonfree: false,
  videoEncoders: [
    encoder({}),
    encoder({ id: "h264_mf", label: "MediaFoundation", vendor: "microsoft", hardware: true }),
    // 注入场景会用 hevc 作为已选编码，探测结果里必须真有它，否则面板会按新规则自动回落到首个可用编码。
    encoder({ id: "hevc_mf", codec: "hevc", label: "MediaFoundation · HEVC", vendor: "microsoft", hardware: true }),
  ],
  audioEncoders: [
    encoder({ id: "aac", codec: "aac", label: "AAC", vendor: "ffmpeg", license: "lgpl" }),
    encoder({ id: "libmp3lame", codec: "mp3", label: "LAME", vendor: "lame", license: "lgpl" }),
  ],
  decodeAccels: [{ id: "d3d11va", label: "D3D11VA", available: true, reason: null }],
  softwareH264Available: true,
  notes: [],
};

const TOOLS = {
  ffmpeg: { path: "C:\\ffmpeg\\bin\\ffmpeg.exe", source: "path", exists: true },
  ffprobe: { path: "C:\\ffmpeg\\bin\\ffprobe.exe", source: "path", exists: true },
  ffmpegVersion: "N-128352",
  ffprobeVersion: "N-128352",
  build: { version: "N-128352", configuration: "--disable-libx264 --enable-libopenh264", gpl: false, nonfree: false, lgplOk: true, notes: [] },
  platform: "windows",
  arch: "x86_64",
  targetTriple: "x86_64-pc-windows-msvc",
  warnings: [],
  searchNote: "mock",
};

const FLAVOR = {
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

/** 在页面脚本之前注入 Tauri IPC 模拟层；可选注入预先存在的 localStorage。 */
async function mockTauri(page, cfg = {}) {
  await page.addInitScript((config) => {
    if (config.prefs) {
      try {
        for (const [key, value] of Object.entries(config.prefs)) {
          localStorage.setItem(key, value);
        }
      } catch {
        // 初始化脚本若在 about:blank 上执行会抛 SecurityError，忽略即可。
      }
    }
    window.__FF_MOCK__ = { calls: [] };
    const callbacks = new Map();
    let callbackSeq = 0;
    const immediate = (value) => Promise.resolve(value);

    // 拖放事件监听在组件卸载时会调用该内部钩子；缺省会让页面抛 TypeError。
    window.__TAURI_EVENT_PLUGIN_INTERNALS__ = {
      unregisterListener(event, eventId) {
        callbacks.delete(eventId);
      },
    };

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
        window.__FF_MOCK__.calls.push({ cmd, args });
        switch (cmd) {
          case "resolve_tools":
            return immediate(config.tools);
          case "flavor_info":
            return immediate(config.flavor);
          case "probe_capabilities":
            return immediate(config.caps);
          case "default_output_dir":
            return immediate("C:\\Users\\Tyler\\Videos\\FormatForge");
          case "list_jobs":
            return immediate([]);
          case "set_job_concurrency":
          case "set_job_paused":
            return immediate(args?.value ?? args?.paused ?? true);
          case "get_job_paused":
            return immediate(false);
          case "set_after_action":
            return immediate(args?.action ?? "none");
          case "plugin:event|listen":
            return immediate(1);
          case "plugin:event|unlisten":
            return immediate(null);
          case "plugin:dialog|open":
            return immediate(null);
          case "plugin:opener|open_url":
            return immediate(null);
          default:
            return immediate(null);
        }
      },
    };
  }, { tools: TOOLS, caps: CAPS, flavor: FLAVOR, ...cfg });
}

const field = (page, label) =>
  page.locator(".field").filter({ has: page.locator("label").filter({ hasText: label }) }).first();

const switchRow = (page, label) => page.locator(".switchrow").filter({ hasText: label }).first();

async function openMethod(page, section, title) {
  const nav = page.locator(".navitem").filter({ has: page.locator("b").filter({ hasText: section }) }).first();
  await nav.click();
  await page.waitForSelector(".method-grid .card", { timeout: 10000 });
  await page.locator(".method-grid .card").filter({ hasText: title }).first().click();
  await page.waitForSelector(".workbench", { timeout: 10000 });
}

const readStorage = (page, key) =>
  page.evaluate((k) => {
    const raw = localStorage.getItem(k);
    return raw ? JSON.parse(raw) : null;
  }, key);

async function main() {
  const browser = await chromium.launch({ channel: "msedge", headless: true });
  const server = await startServer();
  const origin = `http://127.0.0.1:${server.address().port}/`;
  const results = [];
  const pageErrors = [];

  const add = (scenario, name, ok, detail) =>
    results.push({ scenario, name, status: ok ? "pass" : "fail", detail, evidence: { source: "DOM", observed: detail } });

  try {
    // ---------- 场景 1：真实改控件 → 写入 → reload → 恢复 ----------
    const page = await browser.newPage({ viewport: { width: 1440, height: 960 } });
    page.on("pageerror", (error) => pageErrors.push(String(error)));
    await mockTauri(page);
    await page.goto(origin, { waitUntil: "domcontentloaded" });
    await page.waitForSelector(".method-grid", { timeout: 10000 });

    // 视频面板
    await openMethod(page, "视频", "视频转码");
    await field(page, "分辨率").locator("select").selectOption("720");
    await field(page, "帧率").locator("select").selectOption("30");
    await field(page, "旋转").locator(".segmented button").filter({ hasText: "90°" }).click();
    await field(page, "命名模板").locator("input").fill("{name}-{index}");
    await field(page, "同名冲突").locator("select").selectOption("skip");
    const videoStored = await readStorage(page, "transmutio:panel:video:video-mp4");
    const videoNamingStored = await readStorage(page, "transmutio:panel:naming:video-mp4");
    add(
      "panel-persist",
      "视频面板参数写入 localStorage",
      videoStored?.resolution === "720" && videoStored?.fps === "30" && videoStored?.rotation === "90",
      `resolution=${videoStored?.resolution} fps=${videoStored?.fps} rotation=${videoStored?.rotation}`,
    );
    add(
      "panel-persist",
      "视频面板命名策略写入 localStorage",
      videoNamingStored?.template === "{name}-{index}" && videoNamingStored?.conflict === "skip",
      `template=${videoNamingStored?.template} conflict=${videoNamingStored?.conflict}`,
    );

    // 音频面板
    await page.locator(".wb-head .btn").first().click();
    await page.waitForSelector(".method-grid", { timeout: 10000 });
    await openMethod(page, "音频", "音频转码");
    await field(page, "码率").locator("select").selectOption("320");
    await field(page, "采样率").locator(".segmented button").filter({ hasText: "48 kHz" }).click();
    await field(page, "声道").locator(".segmented button").filter({ hasText: "立体声" }).click();
    const audioStored = await readStorage(page, "transmutio:panel:audio:audio-aac");
    add(
      "panel-persist",
      "音频面板参数写入 localStorage",
      audioStored?.bitrateKbps === 320 && audioStored?.sampleRate === "48000" && audioStored?.channels === "stereo",
      `bitrate=${audioStored?.bitrateKbps} sampleRate=${audioStored?.sampleRate} channels=${audioStored?.channels}`,
    );

    // 图片面板
    await page.locator(".wb-head .btn").first().click();
    await page.waitForSelector(".method-grid", { timeout: 10000 });
    await openMethod(page, "图片", "图片批量转换");
    await field(page, "格式").locator(".segmented button").filter({ hasText: "WebP" }).click();
    await field(page, "最长边上限").locator("select").selectOption("1920");
    await switchRow(page, "按 EXIF 自动校正").locator(".switch").click();
    await field(page, "额外旋转").locator(".segmented button").filter({ hasText: "180°" }).click();
    const imageStored = await readStorage(page, "transmutio:panel:image:image-convert");
    add(
      "panel-persist",
      "图片面板参数写入 localStorage",
      imageStored?.format === "webp" && imageStored?.maxEdge === 1920 && imageStored?.autoOrient === false && imageStored?.rotation === "180",
      `format=${imageStored?.format} maxEdge=${imageStored?.maxEdge} autoOrient=${imageStored?.autoOrient} rotation=${imageStored?.rotation}`,
    );

    // ---------- reload（同一 WebView，等价于保留数据的重启）----------
    await page.reload({ waitUntil: "domcontentloaded" });
    await page.waitForSelector(".method-grid", { timeout: 10000 });

    await openMethod(page, "视频", "视频转码");
    const videoRestored = {
      resolution: await field(page, "分辨率").locator("select").inputValue(),
      fps: await field(page, "帧率").locator("select").inputValue(),
      rotation: (await field(page, "旋转").locator(".segmented button.on").innerText()).trim(),
      template: await field(page, "命名模板").locator("input").inputValue(),
      conflict: await field(page, "同名冲突").locator("select").inputValue(),
    };
    add(
      "panel-persist",
      "重载后视频面板恢复",
      videoRestored.resolution === "720" &&
        videoRestored.fps === "30" &&
        videoRestored.rotation === "90°" &&
        videoRestored.template === "{name}-{index}" &&
        videoRestored.conflict === "skip",
      JSON.stringify(videoRestored),
    );

    await page.locator(".wb-head .btn").first().click();
    await page.waitForSelector(".method-grid", { timeout: 10000 });
    await openMethod(page, "音频", "音频转码");
    const audioRestored = {
      bitrate: await field(page, "码率").locator("select").inputValue(),
      sampleRate: (await field(page, "采样率").locator(".segmented button.on").innerText()).trim(),
      channels: (await field(page, "声道").locator(".segmented button.on").innerText()).trim(),
    };
    add(
      "panel-persist",
      "重载后音频面板恢复",
      audioRestored.bitrate === "320" && audioRestored.sampleRate === "48 kHz" && audioRestored.channels === "立体声",
      JSON.stringify(audioRestored),
    );

    await page.locator(".wb-head .btn").first().click();
    await page.waitForSelector(".method-grid", { timeout: 10000 });
    await openMethod(page, "图片", "图片批量转换");
    const imageRestored = {
      format: (await field(page, "格式").locator(".segmented button.on").innerText()).trim(),
      maxEdge: await field(page, "最长边上限").locator("select").inputValue(),
      autoOrient: await switchRow(page, "按 EXIF 自动校正").locator(".switch").getAttribute("aria-pressed"),
      rotation: (await field(page, "额外旋转").locator(".segmented button.on").innerText()).trim(),
    };
    add(
      "panel-persist",
      "重载后图片面板恢复",
      imageRestored.format === "WebP" &&
        imageRestored.maxEdge === "1920" &&
        imageRestored.autoOrient === "false" &&
        imageRestored.rotation === "180°",
      JSON.stringify(imageRestored),
    );

    await mkdir(OUT_DIR, { recursive: true });
    await page.screenshot({ path: path.join(OUT_DIR, "panel-persist-image.png"), fullPage: true });

    // ---------- 场景 2：全新会话，先注入 localStorage，再启动应用 ----------
    const injected = {
      "transmutio:panel:video:video-mp4": JSON.stringify({
        container: "mp4",
        codec: "hevc",
        encoder: "auto",
        rateMode: "bitrate",
        quality: 20,
        bitrateKbps: 6000,
        resolution: "1080",
        fps: "60",
        crop: "1:1",
        rotation: "270",
        audioMode: "none",
        audioBitrateKbps: 128,
        hwDecode: false,
        faststart: false,
      }),
      "transmutio:panel:naming:video-mp4": JSON.stringify({ template: "backup_{date}", conflict: "overwrite" }),
      "transmutio:settings": JSON.stringify({ lastSection: "video" }),
    };
    const page2 = await browser.newPage({ viewport: { width: 1440, height: 960 } });
    page2.on("pageerror", (error) => pageErrors.push(String(error)));
    await mockTauri(page2, { prefs: injected });
    await page2.goto(origin, { waitUntil: "domcontentloaded" });
    await page2.waitForSelector(".method-grid", { timeout: 10000 });
    await openMethod(page2, "视频", "视频转码");
    const injectedRestored = {
      codec: (await field(page2, "视频编码").locator(".segmented button.on").innerText()).trim(),
      resolution: await field(page2, "分辨率").locator("select").inputValue(),
      fps: await field(page2, "帧率").locator("select").inputValue(),
      rotation: (await field(page2, "旋转").locator(".segmented button.on").innerText()).trim(),
      template: await field(page2, "命名模板").locator("input").inputValue(),
      conflict: await field(page2, "同名冲突").locator("select").inputValue(),
    };
    add(
      "panel-persist",
      "注入历史 localStorage 后冷启动恢复",
      injectedRestored.codec === "HEVC" &&
        injectedRestored.resolution === "1080" &&
        injectedRestored.fps === "60" &&
        injectedRestored.rotation === "270°" &&
        injectedRestored.template === "backup_{date}" &&
        injectedRestored.conflict === "overwrite",
      JSON.stringify(injectedRestored),
    );

    // ---------- 场景 3：旧前缀（formatforge:*）在应用启动时迁移到 transmutio:* ----------
    const legacyInjected = {
      "formatforge:panel:video:video-mp4": JSON.stringify({
        resolution: "720",
        fps: "30",
        rotation: "90",
        template: "legacy_{date}",
        conflict: "skip",
      }),
      "formatforge:settings": JSON.stringify({
        lastSection: "video",
        // 与 mock 的默认输出目录一致，避免启动时回写设置而重新生成旧键。
        outputDir: "C:\\Users\\Tyler\\Videos\\FormatForge",
      }),
    };
    const page3 = await browser.newPage({ viewport: { width: 1440, height: 960 } });
    page3.on("pageerror", (error) => pageErrors.push(String(error)));
    // 只在首次加载（迁移标记尚不存在）时写入旧键；reload 时跳过，便于验证旧键不会被重新种下。
    await page3.addInitScript((prefs) => {
      try {
        if (localStorage.getItem("transmutio:storage-migration:v1")) return;
        for (const [key, value] of Object.entries(prefs)) localStorage.setItem(key, value);
      } catch {
        // about:blank 等无 storage 的文档会抛 SecurityError，忽略即可。
      }
    }, legacyInjected);
    await mockTauri(page3);
    await page3.goto(origin, { waitUntil: "domcontentloaded" });
    await page3.waitForSelector(".method-grid", { timeout: 10000 });

    const migrationKeys = [
      "formatforge:panel:video:video-mp4",
      "transmutio:panel:video:video-mp4",
      "formatforge:settings",
      "transmutio:settings",
      "transmutio:storage-migration:v1",
    ];
    const readKeys = (target, keys) =>
      target.evaluate((list) => {
        const out = {};
        for (const k of list) out[k] = localStorage.getItem(k);
        return out;
      }, keys);
    const sameJson = (raw, expected) => {
      try {
        return raw != null && JSON.stringify(JSON.parse(raw)) === JSON.stringify(expected);
      } catch {
        return false;
      }
    };

    const legacyVideo = JSON.parse(legacyInjected["formatforge:panel:video:video-mp4"]);
    const legacySettings = JSON.parse(legacyInjected["formatforge:settings"]);
    const migrateSnapshot = await readKeys(page3, migrationKeys);
    add(
      "panel-persist-prefix-migration",
      "旧前缀键迁移为新前缀并删除旧键",
      migrateSnapshot["formatforge:panel:video:video-mp4"] === null &&
        sameJson(migrateSnapshot["transmutio:panel:video:video-mp4"], legacyVideo) &&
        migrateSnapshot["formatforge:settings"] === null &&
        sameJson(migrateSnapshot["transmutio:settings"], legacySettings) &&
        migrateSnapshot["transmutio:storage-migration:v1"] != null,
      `legacyVideo=${migrateSnapshot["formatforge:panel:video:video-mp4"]} newVideo=${migrateSnapshot["transmutio:panel:video:video-mp4"]} legacySettings=${migrateSnapshot["formatforge:settings"]} marker=${migrateSnapshot["transmutio:storage-migration:v1"]}`,
    );

    await page3.reload({ waitUntil: "domcontentloaded" });
    await page3.waitForSelector(".method-grid", { timeout: 10000 });
    const afterSnapshot = await readKeys(page3, migrationKeys);
    add(
      "panel-persist-prefix-migration",
      "重复加载不重复迁移且新键保持",
      afterSnapshot["transmutio:storage-migration:v1"] === migrateSnapshot["transmutio:storage-migration:v1"] &&
        afterSnapshot["formatforge:panel:video:video-mp4"] === null &&
        sameJson(afterSnapshot["transmutio:panel:video:video-mp4"], legacyVideo) &&
        afterSnapshot["formatforge:settings"] === null &&
        sameJson(afterSnapshot["transmutio:settings"], legacySettings),
      `marker=${afterSnapshot["transmutio:storage-migration:v1"]} legacyVideo=${afterSnapshot["formatforge:panel:video:video-mp4"]} newVideo=${afterSnapshot["transmutio:panel:video:video-mp4"]}`,
    );

    add("panel-persist", "页面无未捕获异常", pageErrors.length === 0, pageErrors.join("; ") || "无");

    await page.close();
    await page2.close();
    await page3.close();
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
      commands: ["npm run build", "node scripts/panel-persist-check.mjs"],
      note: "localStorage 持久化 + 页面重载只能证明前端存储与恢复逻辑；真实 Tauri WebView 重启仍属未验证项",
    },
    summary: { passed, failed, total: results.length },
    screenshots: ["panel-persist-image.png"],
    results,
  };
  await mkdir(OUT_DIR, { recursive: true });
  await writeFile(path.join(OUT_DIR, "panel-persist-report.json"), JSON.stringify(report, null, 2), "utf8");
  console.log(`面板持久化 UI 检查完成：${passed} PASS / ${failed} FAIL（报告：samples/ui-overflow/panel-persist-report.json）`);
  process.exit(failed === 0 ? 0 : 1);
}

await main();
