#!/usr/bin/env node
/**
 * UI 溢出度量（无头 / 隐藏桌面）：
 *   - 启动真实 Tauri 应用（由调用方在隐藏桌面启动，本脚本只连 WebView2 调试端口）；
 *   - 用 CDP 依次切换视口（1280x800 / 1440x900 / 最小窗口 1080x680 / 默认 1400x880）
 *     与 DPI 缩放（100% / 125% / 150%）；
 *   - 每个组合做一次 DOM 度量：
 *       * scrollWidth  > clientWidth  + 1
 *       * scrollHeight > clientHeight + 1
 *       * getBoundingClientRect().right > innerWidth（或 left < 0 / bottom > innerHeight）
 *       * 被 overflow != visible 的祖先裁切的子元素
 *   - 输出可直接复现的清单（选择器 + 元素文本 + 溢出像素），并截图。
 *
 * 用法：node scripts/ui-overflow-check.mjs [--port 9444] [--out samples/ui-overflow]
 */
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const args = process.argv.slice(2);
const port = Number(args.includes("--port") ? args[args.indexOf("--port") + 1] : 9444);

const VIEWPORTS = [
  { name: "default-1400x880-dsf100", width: 1400, height: 880, dsf: 1 },
  { name: "wide-1440x900-dsf100", width: 1440, height: 900, dsf: 1 },
  { name: "laptop-1280x800-dsf100", width: 1280, height: 800, dsf: 1 },
  { name: "min-1080x680-dsf100", width: 1080, height: 680, dsf: 1 },
  { name: "default-1400x880-dsf125", width: 1400, height: 880, dsf: 1.25 },
  { name: "default-1400x880-dsf150", width: 1400, height: 880, dsf: 1.5 },
  { name: "min-1080x680-dsf125", width: 1080, height: 680, dsf: 1.25 },
];

const MEASURE = String.raw`(() => {
  const round = (n) => Math.round(n * 10) / 10;
  const selector = (el) => {
    if (el.id) return "#" + el.id;
    const parts = [];
    let cur = el;
    for (let depth = 0; depth < 5 && cur && cur.nodeType === 1; depth += 1) {
      let p = cur.tagName.toLowerCase();
      const cls = Array.from(cur.classList || []).slice(0, 3);
      if (cls.length) p += "." + cls.join(".");
      const parent = cur.parentElement;
      if (parent) {
        const same = Array.from(parent.children).filter((c) => c.tagName === cur.tagName);
        if (same.length > 1) p += ":nth-of-type(" + (same.indexOf(cur) + 1) + ")";
      }
      parts.unshift(p);
      cur = parent;
    }
    return parts.join(" > ");
  };
  const textOf = (el) => (el.innerText || el.textContent || "").trim().replace(/\s+/g, " ").slice(0, 60);
  const visible = (el, cs) =>
    cs.display !== "none" && cs.visibility !== "hidden" && Number(cs.opacity) > 0 && el.getClientRects().length > 0;

  const rows = { scroll: [], offscreen: [], clipped: [] };
  const nodes = Array.from(document.querySelectorAll("body *"));
  for (const el of nodes) {
    const cs = getComputedStyle(el);
    if (!visible(el, cs)) continue;
    const r = el.getBoundingClientRect();
    if (r.width < 1 || r.height < 1) continue;
    const key = selector(el);
    const base = { selector: key, text: textOf(el), tag: el.tagName.toLowerCase() };

    const ovx = el.scrollWidth - el.clientWidth;
    const ovy = el.scrollHeight - el.clientHeight;
    if ((ovx > 1 || ovy > 1) && (cs.overflowX !== "visible" || cs.overflowY !== "visible")) {
      rows.scroll.push({
        ...base,
        overflowX: round(ovx),
        overflowY: round(ovy),
        cssOverflow: cs.overflowX + "/" + cs.overflowY,
        clientWidth: el.clientWidth,
        scrollWidth: el.scrollWidth,
      });
    }

    const right = r.right - window.innerWidth;
    const left = -r.left;
    const bottom = r.bottom - window.innerHeight;
    if (right > 1 || left > 1 || bottom > 1) {
      // 是否落在「可滚动祖先」里：能滚出来就不算缺陷，只是视口外的滚动内容。
      let scroller = null;
      let cur = el.parentElement;
      while (cur) {
        const acs = getComputedStyle(cur);
        const canX = acs.overflowX === "auto" || acs.overflowX === "scroll";
        const canY = acs.overflowY === "auto" || acs.overflowY === "scroll";
        if ((right > 1 && canX) || ((left > 1 || bottom > 1) && canY)) {
          scroller = selector(cur);
          break;
        }
        cur = cur.parentElement;
      }
      rows.offscreen.push({ ...base, right: round(right), left: round(left), bottom: round(bottom), reachableByScroll: scroller });
    }

    let cur = el.parentElement;
    while (cur) {
      const acs = getComputedStyle(cur);
      const clipsX = acs.overflowX !== "visible";
      const clipsY = acs.overflowY !== "visible";
      if (clipsX || clipsY) {
        // auto/scroll 的祖先只是滚动容器，内容可滚出；只有 hidden/clip 才是真的裁切。
        const hardX = acs.overflowX === "hidden" || acs.overflowX === "clip";
        const hardY = acs.overflowY === "hidden" || acs.overflowY === "clip";
        const ar = cur.getBoundingClientRect();
        const cx = Math.max(0, r.right - ar.right) + Math.max(0, ar.left - r.left);
        const cy = Math.max(0, r.bottom - ar.bottom) + Math.max(0, ar.top - r.top);
        if ((hardX && cx > 1) || (hardY && cy > 1)) {
          rows.clipped.push({
            ...base,
            ancestor: selector(cur),
            ancestorOverflow: acs.overflowX + "/" + acs.overflowY,
            clipX: hardX ? round(cx) : 0,
            clipY: hardY ? round(cy) : 0,
          });
        }
        break;
      }
      cur = cur.parentElement;
    }
  }
  const horizontal = rows.scroll.filter((r) => r.overflowX > 1);
  const hardScroll = horizontal.filter((r) => r.cssOverflow.startsWith("hidden") || r.cssOverflow.startsWith("clip"));
  return {
    viewport: { width: window.innerWidth, height: window.innerHeight, dpr: devicePixelRatio },
    counts: {
      horizontalOverflow: horizontal.length,
      horizontalClipped: hardScroll.length,
      verticalScroll: rows.scroll.filter((r) => r.overflowY > 1).length,
      offscreen: rows.offscreen.length,
      offscreenUnreachable: rows.offscreen.filter((r) => !r.reachableByScroll).length,
      clippedByAncestor: rows.clipped.length,
    },
    horizontalOverflow: horizontal,
    verticalScroll: rows.scroll.filter((r) => r.overflowY > 1),
    offscreen: rows.offscreen,
    clippedByAncestor: rows.clipped,
  };
})()`;

const WIDTHS = String.raw`(() => {
  const w = (sel) => { const el = document.querySelector(sel); return el ? Math.round(el.getBoundingClientRect().width * 10) / 10 : null; };
  const h1 = document.querySelector(".wb-head h1");
  return {
    main: w(".main"),
    wbMain: w(".wb-main"),
    inspector: w(".inspector"),
    taskCenter: w(".taskcenter"),
    dropzone: w(".dropzone"),
    h1Width: h1 ? Math.round(h1.getBoundingClientRect().width) : null,
    h1Height: h1 ? Math.round(h1.getBoundingClientRect().height) : null,
  };
})()`;

async function connect() {
  for (let i = 0; i < 120; i += 1) {
    try {
      const targets = await (await fetch(`http://127.0.0.1:${port}/json/list`)).json();
      const page = targets.find((t) => t.type === "page");
      if (page) {
        const ws = new WebSocket(page.webSocketDebuggerUrl);
        await new Promise((resolve, reject) => {
          ws.onopen = resolve;
          ws.onerror = reject;
        });
        let seq = 0;
        const pending = new Map();
        ws.onmessage = (event) => {
          const msg = JSON.parse(event.data);
          if (msg.id && pending.has(msg.id)) {
            pending.get(msg.id)(msg);
            pending.delete(msg.id);
          }
        };
        const send = (method, params = {}) =>
          new Promise((resolve) => {
            const id = ++seq;
            pending.set(id, resolve);
            ws.send(JSON.stringify({ id, method, params }));
          });
        return { ws, send };
      }
    } catch {
      // 应用尚未就绪
    }
    await new Promise((resolve) => setTimeout(resolve, 500));
  }
  throw new Error(`无法连接 WebView2 调试端口 ${port}`);
}

async function evaluate(send, expression) {
  const response = await send("Runtime.evaluate", { expression, awaitPromise: true, returnByValue: true });
  if (response.result?.exceptionDetails) {
    throw new Error(response.result.exceptionDetails.exception?.description ?? "Runtime.evaluate 失败");
  }
  return response.result?.result?.value;
}

const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms));

async function shot(send, file) {
  const res = await send("Page.captureScreenshot", { format: "png", fromSurface: true });
  if (res.result?.data) fs.writeFileSync(file, Buffer.from(res.result.data, "base64"));
}

async function state(send) {
  await send("Runtime.evaluate", {
    expression: `(() => {
      const cards = Array.from(document.querySelectorAll(".card:not(.disabled)"));
      if (cards.length) { cards[0].click(); return "card"; }
      return "none";
    })()`,
    returnByValue: true,
  }).catch(() => {});
  await sleep(400);
  await send("Runtime.evaluate", {
    expression: `(() => {
      const btn = document.querySelector('.topbar .iconbtn[title="任务中心"]');
      if (btn && btn.getAttribute("aria-pressed") !== "true") { btn.click(); return "tasks-on"; }
      return "tasks-as-is";
    })()`,
    returnByValue: true,
  }).catch(() => {});
  await sleep(400);
}

async function openSettings(send) {
  await send("Runtime.evaluate", {
    expression: `(() => {
      const btn = document.querySelector('.topbar .iconbtn[title="设置"]');
      if (btn) { btn.click(); return "opened"; }
      return "missing";
    })()`,
    returnByValue: true,
  }).catch(() => {});
  await sleep(600);
}

async function main() {
  const outDir = path.join(root, "samples", "ui-overflow");
  fs.mkdirSync(outDir, { recursive: true });
  const { ws, send } = await connect();
  await send("Page.enable");
  await send("Runtime.enable");
  // 等应用启动探测完成（命令通道可用即可）
  for (let i = 0; i < 40; i += 1) {
    try {
      const ok = await evaluate(send, `typeof window.__TAURI_INTERNALS__ !== "undefined"`);
      if (ok) break;
    } catch {
      // ignore
    }
    await sleep(500);
  }
  await sleep(1500);
  // 进入「有方法面板 + 任务中心打开」这一最拥挤的状态再量，避免只量到初始方法网格。
  await state(send);
  await sleep(800);

  const report = [];
  for (const vp of VIEWPORTS) {
    await send("Emulation.setDeviceMetricsOverride", {
      width: vp.width,
      height: vp.height,
      deviceScaleFactor: vp.dsf,
      mobile: false,
    });
    await sleep(700);
    const measured = await evaluate(send, MEASURE);
    const widths = await evaluate(send, WIDTHS);
    report.push({ name: vp.name, requested: vp, widths, ...measured });
    await shot(send, path.join(outDir, `${vp.name}.png`));
    console.log(
      `\n[${vp.name}] viewport=${measured.viewport.width}x${measured.viewport.height} dpr=${measured.viewport.dpr}`,
    );
    console.log(
      `  主区=${widths.main} wb-main=${widths.wbMain} 检查器=${widths.inspector} 任务中心=${widths.taskCenter} 拖拽区=${widths.dropzone} 标题=${widths.h1Width}x${widths.h1Height}`,
    );
    console.log(
      `  横向溢出=${measured.counts.horizontalOverflow}(其中被硬裁=${measured.counts.horizontalClipped}) 纵向滚动=${measured.counts.verticalScroll} 视口外=${measured.counts.offscreen}(不可滚出=${measured.counts.offscreenUnreachable}) 被祖先裁切=${measured.counts.clippedByAncestor}`,
    );
    for (const row of measured.horizontalOverflow) {
      console.log(`  X  ${row.overflowX}px  ${row.selector}  「${row.text}」`);
    }
    for (const row of measured.offscreen) {
      if (row.reachableByScroll) continue;
      console.log(`  V  right=${row.right} left=${row.left} bottom=${row.bottom}  ${row.selector}  「${row.text}」`);
    }
    for (const row of measured.clippedByAncestor) {
      console.log(`  C  clipX=${row.clipX} clipY=${row.clipY} by ${row.ancestor}(${row.ancestorOverflow})  ${row.selector}  「${row.text}」`);
    }
  }
  // 设置抽屉是另一个常见挤压点：在最小窗口下单独量一次。
  await openSettings(send);
  await send("Emulation.setDeviceMetricsOverride", { width: 1080, height: 680, deviceScaleFactor: 1, mobile: false });
  await sleep(700);
  const settingsMeasured = await evaluate(send, MEASURE);
  report.push({ name: "settings-min-1080x680-dsf100", requested: { width: 1080, height: 680, dsf: 1 }, ...settingsMeasured });
  await shot(send, path.join(outDir, "settings-min-1080x680-dsf100.png"));
  console.log(
    `\n[settings-min-1080x680-dsf100] 横向溢出=${settingsMeasured.counts.horizontalOverflow}(被硬裁=${settingsMeasured.counts.horizontalClipped}) 视口外不可滚出=${settingsMeasured.counts.offscreenUnreachable} 被祖先裁切=${settingsMeasured.counts.clippedByAncestor}`,
  );
  for (const row of settingsMeasured.horizontalOverflow) console.log(`  X  ${row.overflowX}px  ${row.selector}  「${row.text}」`);
  for (const row of settingsMeasured.clippedByAncestor) console.log(`  C  clipY=${row.clipY} by ${row.ancestor}  ${row.selector}  「${row.text}」`);

  fs.writeFileSync(path.join(outDir, "report.json"), JSON.stringify({ generatedAt: new Date().toISOString(), viewports: report }, null, 2));
  console.log(`\n报告：${path.join(outDir, "report.json")}`);
  ws.close();
}

main().catch((error) => {
  console.error(error);
  process.exit(1);
});
