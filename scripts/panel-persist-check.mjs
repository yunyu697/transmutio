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
  subtitleBurn: true,
  notes: [],
};

// 与后端 device_presets.rs 同一份字段形态（只取 UI 断言用到的两条）。
const DEVICE_PRESETS = [
  {
    id: "apple-1080p",
    label: "iPhone / iPad（1080p H.264）",
    note: "Apple 设备通用档：MP4 + H.264 + AAC 128 kbps，1080p / 30 fps",
    container: "mp4",
    codec: "h264",
    resolution: "1080",
    fps: "30",
    audioBitrateKbps: 128,
  },
  {
    id: "android-720p",
    label: "Android 手机 / 平板（720p H.264）",
    note: "兼容性优先：MP4 + H.264 + AAC 128 kbps，720p / 30 fps",
    container: "mp4",
    codec: "h264",
    resolution: "720",
    fps: "30",
    audioBitrateKbps: 128,
  },
];

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
          case "list_device_presets":
            return immediate(config.presets ?? []);
          default:
            return immediate(null);
        }
      },
    };
  }, { tools: TOOLS, caps: CAPS, flavor: FLAVOR, presets: DEVICE_PRESETS, ...cfg });
}

const field = (page, label) =>
  page.locator(".field").filter({ has: page.locator("label").filter({ hasText: label }) }).first();

const switchRow = (page, label) => page.locator(".switchrow").filter({ hasText: label }).first();

/** 次要参数默认折叠（details.group.fold）；展开后内部控件才可交互。 */
async function expandFolds(page) {
  await page.evaluate(() => {
    document.querySelectorAll("details.group.fold").forEach((node) => {
      node.open = true;
    });
  });
}

async function openMethod(page, section, title, expand = true) {
  const nav = page.locator(".grow").filter({ hasText: section }).first();
  await nav.click();
  await page.waitForSelector(".method-grid .card", { timeout: 10000 });
  await page.locator(".method-grid .card").filter({ hasText: title }).first().click();
  await page.waitForSelector(".workbench", { timeout: 10000 });
  if (expand) await expandFolds(page);
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
    await openMethod(page, "视频", "视频转码", false);
    // 次要参数默认折叠：只有目标格式 / 画质与尺寸 / 输出命名三组是展开的。
    const foldState = await page.evaluate(() => {
      const folds = Array.from(document.querySelectorAll(".inspector-body > details.group.fold"));
      // 新版 Chromium 用 content-visibility 隐藏 details 内容，子元素的 rect 仍可能有尺寸；
      // 折叠是否真的生效，用「整组高度 == 标题行高度」来判断。
      const summaryOnly = folds.filter((node) => {
        const group = node.getBoundingClientRect().height;
        const summary = node.querySelector("summary")?.getBoundingClientRect().height ?? 0;
        return summary > 0 && Math.abs(group - summary) < 2;
      }).length;
      return {
        folded: folds.length,
        opened: folds.filter((node) => node.open).length,
        summaryOnly,
        visible: document.querySelectorAll(".inspector-body > .group:not(.fold)").length,
        titles: folds.map((node) => node.querySelector("summary")?.textContent?.trim() ?? ""),
      };
    });
    add(
      "ui-redesign",
      "次要参数默认折叠：视频面板只露 3 组，其余 6 组收起且只占标题行高度",
      foldState.folded === 6 &&
        foldState.opened === 0 &&
        foldState.summaryOnly === 6 &&
        foldState.visible === 3,
      JSON.stringify(foldState),
    );
    const fold = page.locator(".inspector-body > details.group.fold").first();
    await fold.locator("summary").click();
    const foldOpened = await fold.evaluate((node) => {
      const group = node.getBoundingClientRect().height;
      const summary = node.querySelector("summary")?.getBoundingClientRect().height ?? 0;
      return node.open && group > summary + 20;
    });
    add("ui-redesign", "点击折叠标题可展开该组参数", foldOpened, `expanded=${foldOpened}`);
    await expandFolds(page);
    // 设备预设：应用模板后容器/编码/分辨率/帧率要写进同一份面板参数。
    await field(page, "设备预设").locator("select").selectOption("apple-1080p");
    const presetStored = await readStorage(page, "transmutio:panel:video:video-mp4");
    add(
      "panel-persist",
      "设备预设模板写入面板参数",
      presetStored?.resolution === "1080" &&
        presetStored?.fps === "30" &&
        presetStored?.container === "mp4" &&
        presetStored?.codec === "h264",
      `resolution=${presetStored?.resolution} fps=${presetStored?.fps} container=${presetStored?.container} codec=${presetStored?.codec}`,
    );
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
    await field(page, "码率").locator("select").selectOption("max");
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
      audioRestored.bitrate === "max" && audioRestored.sampleRate === "48 kHz" && audioRestored.channels === "立体声",
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

    // ---------- 场景 3：旧前缀（formatforge:*）不再读取、不迁移、也不删除 ----------
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
        outputDir: "C:\\Users\\Tyler\\Videos\\FormatForge",
      }),
      "transmutio:storage-migration:v1": JSON.stringify({ migrated: 7, at: 1 }),
      "transmutio:migration-notice-dismissed": "1",
    };
    const page3 = await browser.newPage({ viewport: { width: 1440, height: 960 } });
    page3.on("pageerror", (error) => pageErrors.push(String(error)));
    await page3.addInitScript((prefs) => {
      try {
        if (!localStorage.getItem("transmutio:test-legacy-seeded")) {
          for (const [key, value] of Object.entries(prefs)) localStorage.setItem(key, value);
          localStorage.setItem("transmutio:test-legacy-seeded", "1");
        }
      } catch {
        // about:blank 等无 storage 的文档会抛 SecurityError，忽略即可。
      }
      // 记录所有对旧前缀的访问，证明应用既没读、也没写、也没删。
      try {
        const proto = Storage.prototype;
        const wrap = (name) => {
          const original = proto[name];
          proto[name] = function (key, ...rest) {
            if (!window.__legacyLogPaused && String(key).startsWith("formatforge:")) {
              (window.__legacyAccessLog ||= []).push([name, String(key)]);
            }
            return original.call(this, key, ...rest);
          };
        };
        wrap("getItem");
        wrap("setItem");
        wrap("removeItem");
        wrap("key");
      } catch {
        /* ignore */
      }
      // 统计一次性清理标记位的写入次数（sessionStorage 跨 reload 累计），
      // 用来证明「再启动一次不产生新写入」。
      try {
        const proto = Storage.prototype;
        const originalSet = proto.setItem;
        proto.setItem = function (key, value) {
          if (String(key) === "transmutio:cleanup:v2") {
            const n = Number(sessionStorage.getItem("transmutio:test-cleanup-writes") || "0") + 1;
            originalSet.call(sessionStorage, "transmutio:test-cleanup-writes", String(n));
          }
          return originalSet.call(this, key, value);
        };
      } catch {
        /* ignore */
      }
    }, legacyInjected);
    await mockTauri(page3);
    await page3.goto(origin, { waitUntil: "domcontentloaded" });
    await page3.waitForSelector(".method-grid", { timeout: 10000 });

    const legacyPrefixKeys = Object.keys(legacyInjected).filter((k) => k.startsWith("formatforge:"));
    const migrationKeys = [
      ...Object.keys(legacyInjected),
      "transmutio:panel:video:video-mp4",
      "transmutio:settings",
      "transmutio:cleanup:v2",
    ];
    const readKeys = (target, keys) =>
      target.evaluate((list) => {
        // 测试自身要读旧键做断言，暂停记录，避免把自己的读取算进「应用访问」。
        window.__legacyLogPaused = true;
        const out = {};
        for (const k of list) out[k] = localStorage.getItem(k);
        window.__legacyLogPaused = false;
        return out;
      }, keys);

    const migratedSnapshot = await readKeys(page3, migrationKeys);
    add(
      "panel-persist-legacy-ignored",
      "旧前缀键保留原值、不迁移；alpha11 遗留键被清理",
      legacyPrefixKeys.every((key) => migratedSnapshot[key] === legacyInjected[key]) &&
        migratedSnapshot["transmutio:storage-migration:v1"] === null &&
        migratedSnapshot["transmutio:migration-notice-dismissed"] === null &&
        migratedSnapshot["transmutio:cleanup:v2"] === "1",
      `legacyVideo=${migratedSnapshot["formatforge:panel:video:video-mp4"]} legacySettings=${migratedSnapshot["formatforge:settings"]} marker=${migratedSnapshot["transmutio:storage-migration:v1"]} notice=${migratedSnapshot["transmutio:migration-notice-dismissed"]} cleanupMarker=${migratedSnapshot["transmutio:cleanup:v2"]}`,
    );

    await openMethod(page3, "视频", "视频转码");
    const ignoredInPanel = {
      resolution: await field(page3, "分辨率").locator("select").inputValue(),
      fps: await field(page3, "帧率").locator("select").inputValue(),
      rotation: (await field(page3, "旋转").locator(".segmented button.on").innerText()).trim(),
      template: await field(page3, "命名模板").locator("input").inputValue(),
      conflict: await field(page3, "同名冲突").locator("select").inputValue(),
    };
    add(
      "panel-persist-legacy-ignored",
      "面板使用默认值而不是旧前缀里的值",
      ignoredInPanel.resolution === "source" &&
        ignoredInPanel.fps === "source" &&
        ignoredInPanel.rotation === "不旋转" &&
        ignoredInPanel.template === "" &&
        ignoredInPanel.conflict === "suffix",
      JSON.stringify(ignoredInPanel),
    );

    const legacyAccessBefore = await page3.evaluate(() => window.__legacyAccessLog || []);
    await page3.reload({ waitUntil: "domcontentloaded" });
    await page3.waitForSelector(".method-grid", { timeout: 10000 });
    const legacyAccessAfter = await page3.evaluate(() => window.__legacyAccessLog || []);
    const legacyAccess = [...legacyAccessBefore, ...legacyAccessAfter];
    add(
      "panel-persist-legacy-ignored",
      "启动与面板加载对旧前缀零访问",
      legacyAccess.length === 0,
      `beforeReload=${JSON.stringify(legacyAccessBefore)} afterReload=${JSON.stringify(legacyAccessAfter)}`,
    );
    const afterSnapshot = await readKeys(page3, migrationKeys);
    add(
      "panel-persist-legacy-ignored",
      "重复加载仍不读、不删旧前缀键",
      legacyPrefixKeys.every((key) => afterSnapshot[key] === legacyInjected[key]) &&
        afterSnapshot["transmutio:storage-migration:v1"] === null &&
        afterSnapshot["transmutio:migration-notice-dismissed"] === null &&
        afterSnapshot["transmutio:cleanup:v2"] === "1",
      `legacyVideo=${afterSnapshot["formatforge:panel:video:video-mp4"]} legacySettings=${afterSnapshot["formatforge:settings"]} marker=${afterSnapshot["transmutio:storage-migration:v1"]} notice=${afterSnapshot["transmutio:migration-notice-dismissed"]} cleanupMarker=${afterSnapshot["transmutio:cleanup:v2"]}`,
    );

    // 标记位写入次数：首次启动 1 次，刷新（等价再启动）后仍为 1，证明没有每次启动都写。
    const cleanupWrites = await page3.evaluate(() =>
      Number(sessionStorage.getItem("transmutio:test-cleanup-writes") || "0"),
    );
    add(
      "panel-persist-legacy-ignored",
      "清理只跑一次：二次启动不再写标记位",
      cleanupWrites === 1,
      `markerWrites=${cleanupWrites}（首次启动应写 1 次，刷新后不应增加）`,
    );

    // ---------- 场景 5：v4 界面改版的 DOM 断言（首页 / 图标 / 折叠 / 状态栏 / 预设） ----------
    const page4 = await browser.newPage({ viewport: { width: 1440, height: 960 } });
    page4.on("pageerror", (error) => pageErrors.push(String(error)));
    await mockTauri(page4);
    await page4.goto(origin, { waitUntil: "domcontentloaded" });
    await page4.waitForSelector(".method-grid", { timeout: 10000 });

    // 入口已合并：启动直接落在当前分区的方法列表，侧栏只有分区一层。
    const singleEntry = await page4.evaluate(() => ({
      grid: document.querySelectorAll(".method-grid .card").length,
      grows: document.querySelectorAll(".sidenav .grow").length,
      recent: document.querySelectorAll(".sidenav .ritem, .sidenav .side-group .side-label").length,
      subs: document.querySelectorAll(".sidenav .subs, .sidenav .sub").length,
      homeBlocks: document.querySelectorAll(".overviews, .quick, .fmtstrip").length,
      activeSection: document.querySelector(".sidenav .grow.active")?.textContent ?? "",
    }));
    add(
      "ui-redesign",
      "入口合一：启动即方法列表，侧栏仅 3 个分区、无最近使用 / 子项 / 首页速览卡",
      singleEntry.grid === 11 &&
        singleEntry.grows === 3 &&
        singleEntry.recent === 1 &&
        singleEntry.subs === 0 &&
        singleEntry.homeBlocks === 0 &&
        singleEntry.activeSection.includes("视频"),
      JSON.stringify(singleEntry),
    );
    await page4.screenshot({ path: path.join(OUT_DIR, "entry-single.png") });

    // 任务中心：底部队列条常驻，抽屉默认收起，点开后才出现完整列表。
    const barState = await page4.evaluate(() => {
      const bar = document.querySelector(".queuebar");
      const status = document.querySelector(".statusbar");
      const main = document.querySelector(".main");
      const barBox = bar?.getBoundingClientRect();
      return {
        bar: Boolean(bar),
        barHeight: Math.round(barBox?.height ?? 0),
        aboveStatus: Boolean(barBox && status && status.getBoundingClientRect().top >= barBox.bottom - 1),
        drawer: document.querySelectorAll(".taskcenter").length,
        mainHeight: Math.round(main?.getBoundingClientRect().height ?? 0),
        text: bar?.textContent?.replace(/\s+/g, " ").trim().slice(0, 80) ?? "",
      };
    });
    add(
      "ui-redesign",
      "任务中心降为底部队列条：44px 常驻在状态栏上方，抽屉默认收起",
      barState.bar &&
        barState.barHeight === 44 &&
        barState.aboveStatus &&
        barState.drawer === 0 &&
        barState.text.includes("任务队列"),
      JSON.stringify(barState),
    );

    await page4.locator(".queuebar .qb-main").click();
    await page4.waitForSelector(".taskcenter", { timeout: 5000 });
    const drawerState = await page4.evaluate(() => {
      const drawer = document.querySelector(".taskcenter");
      const bar = document.querySelector(".queuebar");
      return {
        height: Math.round(drawer?.getBoundingClientRect().height ?? 0),
        barStillVisible: Boolean(bar && bar.getBoundingClientRect().height > 0),
        expanded: document.querySelector(".qb-main")?.getAttribute("aria-expanded"),
      };
    });
    add(
      "ui-redesign",
      "点击队列条展开任务抽屉，队列条保持可见",
      drawerState.height >= 200 &&
        drawerState.height <= 340 &&
        drawerState.barStillVisible &&
        drawerState.expanded === "true",
      JSON.stringify(drawerState),
    );
    await page4.screenshot({ path: path.join(OUT_DIR, "queue-drawer.png") });

    await page4.locator(".queuebar .qb-main").click();
    const drawerClosed = (await page4.locator(".taskcenter").count()) === 0;
    add("ui-redesign", "再次点击队列条收起抽屉", drawerClosed, `drawerCount=${drawerClosed ? 0 : 1}`);

    // ---------- 输出位置：开始按钮上方可就地改 + 一键切回源文件夹 ----------
    const page5 = await browser.newPage({ viewport: { width: 1440, height: 960 } });
    page5.on("pageerror", (error) => pageErrors.push(String(error)));
    await mockTauri(page5);
    // 只播一次种：reload 时保留应用自己写回的设置，才能验证持久化。
    await page5.addInitScript((prefs) => {
      try {
        if (!localStorage.getItem("transmutio:test-out-seeded")) {
          localStorage.setItem("transmutio:settings", JSON.stringify(prefs));
          localStorage.setItem("transmutio:test-out-seeded", "1");
        }
      } catch {
        // about:blank 上无 storage，忽略。
      }
    }, {
      lastSection: "video",
      outputDirMode: "custom",
      outputDir: "D:\\Exports\\Transmutio",
    });
    await page5.goto(origin, { waitUntil: "domcontentloaded" });
    await page5.waitForSelector(".method-grid", { timeout: 10000 });
    await openMethod(page5, "视频", "视频转码");

    const outBefore = await page5.evaluate(() => {
      const bar = document.querySelector(".outdir");
      const start = document.querySelector(".bottombar .btn.primary");
      const barBox = bar?.getBoundingClientRect();
      const startBox = start?.getBoundingClientRect();
      const buttons = Array.from(bar?.querySelectorAll("button") ?? []).map((b) => b.textContent?.trim() ?? "");
      return {
        text: bar?.textContent?.replace(/\s+/g, " ").trim() ?? "",
        buttons,
        aboveStart: Boolean(barBox && startBox && barBox.bottom <= startBox.top + 1),
      };
    });
    add(
      "ui-redesign",
      "输出位置在开始按钮上方：自定义目录显示完整路径并提供「使用源文件夹」/「更改」",
      outBefore.aboveStart &&
        outBefore.text.includes("D:\\Exports\\Transmutio") &&
        outBefore.buttons.includes("更改") &&
        outBefore.buttons.includes("使用源文件夹"),
      JSON.stringify(outBefore),
    );

    await page5.getByRole("button", { name: "使用源文件夹" }).click();
    const outAfter = await page5.evaluate(() => {
      const bar = document.querySelector(".outdir");
      const buttons = Array.from(bar?.querySelectorAll("button") ?? []).map((b) => b.textContent?.trim() ?? "");
      return {
        text: bar?.textContent?.replace(/\s+/g, " ").trim() ?? "",
        buttons,
        mode: JSON.parse(localStorage.getItem("transmutio:settings") || "{}").data?.outputDirMode,
      };
    });
    add(
      "ui-redesign",
      "一键切回源文件夹：显示「与源文件相同目录」，入口随即隐藏并写入设置",
      outAfter.text.includes("与源文件相同目录") &&
        !outAfter.buttons.includes("使用源文件夹") &&
        outAfter.buttons.includes("更改") &&
        outAfter.mode === "source",
      JSON.stringify(outAfter),
    );

    await page5.reload({ waitUntil: "domcontentloaded" });
    await page5.waitForSelector(".method-grid", { timeout: 10000 });
    await openMethod(page5, "视频", "视频转码");
    const outPersisted = await page5.evaluate(() => {
      const bar = document.querySelector(".outdir");
      return {
        text: bar?.textContent?.replace(/\s+/g, " ").trim() ?? "",
        buttons: Array.from(bar?.querySelectorAll("button") ?? []).map((b) => b.textContent?.trim() ?? ""),
      };
    });
    add(
      "ui-redesign",
      "输出位置状态持久化：重启后仍是源文件夹且入口保持隐藏",
      outPersisted.text.includes("与源文件相同目录") && !outPersisted.buttons.includes("使用源文件夹"),
      JSON.stringify(outPersisted),
    );
    await page5.screenshot({ path: path.join(OUT_DIR, "output-source.png") });
    await page5.close();

    // ---------- 主题：明暗 × 配色方案分开设置、可组合、可持久化 ----------
    const page6 = await browser.newPage({ viewport: { width: 1440, height: 960 } });
    page6.on("pageerror", (error) => pageErrors.push(String(error)));
    await mockTauri(page6);
    await page6.goto(origin, { waitUntil: "domcontentloaded" });
    await page6.waitForSelector(".method-grid", { timeout: 10000 });
    const readTheme = () =>
      page6.evaluate(() => {
        const style = getComputedStyle(document.documentElement);
        return {
          theme: document.documentElement.getAttribute("data-theme"),
          palette: document.documentElement.getAttribute("data-palette"),
          bg: style.getPropertyValue("--ff-bg").trim().toLowerCase(),
          surf: style.getPropertyValue("--ff-surf").trim().toLowerCase(),
          accent: style.getPropertyValue("--ff-accent").trim().toLowerCase(),
          stored: (() => {
            try {
              return JSON.parse(localStorage.getItem("transmutio:settings") || "{}").data || {};
            } catch {
              return {};
            }
          })(),
        };
      });
    const themeDefault = await readTheme();
    add(
      "ui-redesign",
      "默认主题：深色 + 石墨，data-theme / data-palette 与 CSS 变量一致",
      themeDefault.theme === "dark" &&
        themeDefault.palette === "graphite" &&
        themeDefault.bg === "#0a0e14" &&
        themeDefault.surf === "#121a24",
      JSON.stringify(themeDefault),
    );

    await page6.locator(".side-foot .sibtn").last().click();
    await page6.waitForSelector(".drawer", { timeout: 5000 });
    const paletteCount = await page6.locator(".palette-grid .palette").count();
    add("ui-redesign", "设置页提供明暗三档 + 4 套完整配色", paletteCount === 4, `paletteCards=${paletteCount}`);
    await page6.waitForTimeout(400); // 等抽屉滑入动画结束再截图
    await page6.screenshot({ path: path.join(OUT_DIR, "palette-settings.png") });

    await page6.getByRole("button", { name: "浅色", exact: true }).click();
    const themeLight = await readTheme();
    add(
      "ui-redesign",
      "只改明暗：切到浅色后配色仍是石墨，只有亮度变量变化",
      themeLight.theme === "light" &&
        themeLight.palette === "graphite" &&
        themeLight.bg === "#edf1f6" &&
        themeLight.stored.brightness === "light",
      JSON.stringify(themeLight),
    );

    await page6.getByRole("button", { name: /深海/ }).click();
    const themeOcean = await readTheme();
    add(
      "ui-redesign",
      "明暗与配色正交组合：浅色 + 深海，背景/卡片/强调色整套切换",
      themeOcean.theme === "light" &&
        themeOcean.palette === "ocean" &&
        themeOcean.bg === "#e9f1fb" &&
        themeOcean.surf === "#ffffff" &&
        themeOcean.accent === "#1a64cc" &&
        themeOcean.stored.palette === "ocean" &&
        themeOcean.stored.brightness === "light",
      JSON.stringify(themeOcean),
    );

    await page6.reload({ waitUntil: "domcontentloaded" });
    await page6.waitForSelector(".method-grid", { timeout: 10000 });
    const themePersisted = await readTheme();
    add(
      "ui-redesign",
      "主题持久化：重启后仍是浅色 + 深海",
      themePersisted.theme === "light" &&
        themePersisted.palette === "ocean" &&
        themePersisted.bg === "#e9f1fb",
      JSON.stringify(themePersisted),
    );
    await page6.screenshot({ path: path.join(OUT_DIR, "theme-ocean-light.png") });
    await page6.close();

    const brand = await page4.evaluate(() => {
      const mark = document.querySelector(".sidenav .appmark:not(.rail-mark)");
      const imgs = [...(mark?.querySelectorAll("img") ?? [])];
      if (!mark || imgs.length !== 2) return null;
      const ms = getComputedStyle(mark);
      const shown = imgs.filter((i) => getComputedStyle(i).display !== "none");
      return {
        sources: imgs.map((i) => i.getAttribute("src")),
        visible: shown.map((i) => i.getAttribute("src")),
        box: `${ms.width}x${ms.height}`,
        radius: getComputedStyle(imgs[0]).borderRadius,
        theme: document.documentElement.dataset.theme,
      };
    });
    const expectedBrand = brand?.theme === "light" ? "brand-light" : "brand-dark";
    add(
      "ui-redesign",
      "品牌图标：明暗两版 SVG，只按 data-theme 切换（24px / 圆角 6）",
      Boolean(brand) &&
        brand.sources.some((s) => s.includes("brand-light")) &&
        brand.sources.some((s) => s.includes("brand-dark")) &&
        brand.visible.length === 1 &&
        brand.visible[0].includes(expectedBrand) &&
        brand.box === "24pxx24px" &&
        brand.radius === "6px",
      JSON.stringify(brand),
    );

    const statusGroups = await page4.evaluate(
      () => document.querySelectorAll(".statusbar > span:not(.spacer)").length,
    );
    add("ui-redesign", "状态栏仅 5 组", statusGroups === 5, `groups=${statusGroups}`);

    const bodyText = await page4.evaluate(() => document.body.innerText);
    add(
      "ui-redesign",
      "界面无 PDF / Poppler 残留文案",
      !/pdf|poppler|pdftoppm|pdftotext/i.test(bodyText),
      `matches=${(bodyText.match(/pdf|poppler|pdftoppm|pdftotext/gi) || []).join(",") || "无"}`,
    );

    await page4.locator(".collapse").click();
    await page4.waitForSelector(".sidenav.rail", { timeout: 5000 });
    const collapsedSettings = await readStorage(page4, "transmutio:settings");
    await page4.reload({ waitUntil: "domcontentloaded" });
    await page4.waitForSelector(".sidenav.rail", { timeout: 10000 });
    add(
      "ui-redesign",
      "侧栏折叠为 64px rail 且折叠态持久化",
      collapsedSettings?.data?.navCollapsed === true,
      `navCollapsed=${collapsedSettings?.data?.navCollapsed} rail=可见`,
    );

    await page4.locator('.sidenav button[aria-label="展开侧栏"]').click();
    await page4.waitForSelector(".sidenav:not(.rail) .grow", { timeout: 5000 });
    await openMethod(page4, "视频", "视频转码");
    await field(page4, "分辨率").locator("select").selectOption("720");
    await page4.locator(".presetbar > button").click();
    await page4.locator(".preset-save input").fill("验收预设");
    await page4.locator(".preset-save .btn.primary").click();
    const presetStore = await readStorage(page4, "transmutio:presets:v1");
    const saved = presetStore?.presets?.find((p) => p.name === "验收预设");
    const savedKeys = saved ? Object.keys(saved.params || {}) : [];
    add(
      "ui-redesign",
      "保存为预设：写入 transmutio:presets:v1 且不含路径 / 凭据字段",
      Boolean(saved) &&
        saved.methodId === "video-mp4" &&
        saved.params?.resolution === "720" &&
        !savedKeys.some((k) => /path|dir|token|secret|password|credential/i.test(k)),
      `presets=${presetStore?.presets?.length ?? 0} keys=${savedKeys.join(",")}`,
    );

    await field(page4, "分辨率").locator("select").selectOption("1080");
    await page4.locator(".wb-head .btn").first().click();
    await page4.waitForSelector(".method-grid", { timeout: 10000 });
    await openMethod(page4, "视频", "视频转码");
    const restored = await field(page4, "分辨率").locator("select").inputValue();
    const toastText = await page4.evaluate(
      () => document.querySelector(".toast")?.textContent ?? "",
    );
    add(
      "ui-redesign",
      "再次进入方法自动载入上次预设（分辨率回到 720，并有提示）",
      restored === "720" && toastText.includes("已载入上次预设"),
      `resolution=${restored} toast=${toastText.replace(/\s+/g, " ").slice(0, 60)}`,
    );

    await page4.close();

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
