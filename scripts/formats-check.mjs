// 静态断言：格式清单完整性 + 探测失败提示存在。
// 真实转码断言由 `cargo run --features dev-tools --bin p2-check` 覆盖（format-matrix）。
import { readFileSync, readdirSync } from "node:fs";
import { join } from "node:path";
import { fileURLToPath } from "node:url";

const root = fileURLToPath(new URL("../", import.meta.url));
const results = [];
function check(name, ok, detail) {
  results.push({ name, ok, detail });
}

const formats = readFileSync(join(root, "src/lib/formats.ts"), "utf8");
const videoPanel = readFileSync(join(root, "src/panels/VideoPanel.tsx"), "utf8");
const audioPanel = readFileSync(join(root, "src/panels/AudioPanel.tsx"), "utf8");

const videoBlock = formats.slice(formats.indexOf("export const VIDEO_CONTAINERS"), formats.indexOf("export const CODEC_LABELS"));
const audioBlock = formats.slice(formats.indexOf("export const AUDIO_FORMATS"), formats.indexOf("export const PROBE_MISSING_NOTICE"));
const videoCount = (videoBlock.match(/id: "\w+"/g) || []).length;
const audioCount = (audioBlock.match(/id: "\w+"/g) || []).length;

check(
  "视频容器数量 ≥ 4",
  videoCount >= 4,
  `VIDEO_CONTAINERS=${videoCount}，期望 ≥4（mp4/mkv/webm/mov 及可选 ts/avi）`,
);
check(
  "音频格式数量 ≥ 5",
  audioCount >= 5,
  `AUDIO_FORMATS=${audioCount}，期望 ≥5（mp3/aac/flac/wav/ogg/opus）`,
);

const requiredContainers = ["mp4", "mkv", "webm", "mov"];
const missingContainer = requiredContainers.filter((id) => !videoBlock.includes(`id: "${id}"`));
check("必备容器齐全", missingContainer.length === 0, missingContainer.length ? `缺少 ${missingContainer.join(", ")}` : "mp4/mkv/webm/mov 均在清单中");

const requiredAudio = ["mp3", "aac", "flac", "wav", "ogg", "opus"];
const missingAudio = requiredAudio.filter((id) => !audioBlock.includes(`id: "${id}"`));
check("必备音频格式齐全", missingAudio.length === 0, missingAudio.length ? `缺少 ${missingAudio.join(", ")}` : "六种音频格式均在清单中");

check(
  "不兼容组合在容器清单中被限制",
  !videoBlock.includes('id: "mp4", label: "MP4", ext: "mp4", codecs: ["vp9"') &&
    videoBlock.includes('id: "mp4"') &&
    videoBlock.includes('id: "webm"'),
  "mp4 不含 vp9、webm 不含 h264（由容器 codecs 列表约束）",
);

check(
  "探测失败提示存在",
  formats.includes("未检测到 ffmpeg 或编码器不可用") && videoPanel.includes("probeNotice") && audioPanel.includes("probeNotice"),
  "PROBE_MISSING_NOTICE 文案存在，且两个面板都渲染 probeNotice",
);

check(
  "面板引用集中清单（非硬编码单格式）",
  videoPanel.includes("VIDEO_CONTAINERS") && audioPanel.includes("AUDIO_FORMATS"),
  "VideoPanel 使用 VIDEO_CONTAINERS，AudioPanel 使用 AUDIO_FORMATS",
);

// 卡片改名：音频分区只剩一张「音频转码」卡（audio-aac），旧标题「音频转 AAC」全仓不再出现。
const catalog = readFileSync(join(root, "src/catalog.ts"), "utf8");
const audioAacLine = catalog.split("\n").find((line) => line.includes('"audio-aac"')) ?? "";
const srcDir = join(root, "src");
const srcFiles = readdirSync(srcDir, { recursive: true })
  .map((entry) => String(entry))
  .filter((entry) => /\.(ts|tsx|js|jsx)$/.test(entry));
const oldTitleHits = srcFiles.filter((entry) =>
  readFileSync(join(srcDir, entry), "utf8").includes("音频转 AAC"),
);
check(
  "音频卡片合并为单张「音频转码」",
  audioAacLine.includes('"音频转码"') && !catalog.includes('"audio-mp3"') && oldTitleHits.length === 0,
  `audio-aac 标题=${audioAacLine.includes('"音频转码"') ? "音频转码" : "不匹配"}，旧标题残留=${oldTitleHits.join(", ") || "无"}`,
);

// PDF 缺工具提示：面板内给出完整配置说明与重新检测入口。
const advancedPanels = readFileSync(join(root, "src/panels/AdvancedPanels.tsx"), "utf8");
const pdfHintTokens = ["pdftoppm", "Poppler", "PDF 工具目录", "PDFTOPPM_PATH", "TRANSMUTIO_PDF_TOOLS", "重新检测"];
const missingPdfTokens = pdfHintTokens.filter((token) => !advancedPanels.includes(token));
check(
  "PDF 缺工具提示含完整配置说明",
  missingPdfTokens.length === 0,
  missingPdfTokens.length ? `缺少 ${missingPdfTokens.join(", ")}` : "pdftoppm/Poppler/PDF 工具目录/PDFTOPPM_PATH/TRANSMUTIO_PDF_TOOLS/重新检测 均在面板中",
);

// 存储前缀迁移模块。
const storageModule = readFileSync(join(root, "src/lib/storage.ts"), "utf8");
check(
  "存储前缀迁移模块存在",
  storageModule.includes('LEGACY_PREFIX = "formatforge:"') &&
    storageModule.includes('STORAGE_PREFIX = "transmutio:"') &&
    storageModule.includes("MIGRATION_KEY"),
  "src/lib/storage.ts 定义 LEGACY_PREFIX / STORAGE_PREFIX / MIGRATION_KEY",
);

let failed = 0;
for (const item of results) {
  console.log(`${item.ok ? "PASS" : "FAIL"} ${item.name}: ${item.detail}`);
  if (!item.ok) failed += 1;
}
console.log(`\nformats check: ${results.length - failed}/${results.length} passed`);
process.exit(failed ? 1 : 0);
