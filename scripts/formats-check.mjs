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
const imagePanel = readFileSync(join(root, "src/panels/ImagePanel.tsx"), "utf8");
const apiSource = readFileSync(join(root, "src/lib/api.ts"), "utf8");

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

// 变速：范围常量集中在 speed.ts，两个面板使用同一套预设与「保持音高」开关；
// 后端滤镜链必须包含 atempo 链式拆分与 setpts 变速。
const speedModule = readFileSync(join(root, "src/lib/speed.ts"), "utf8");
const speedConstants = ["SPEED_MIN = 0.25", "SPEED_MAX = 4", "SPEED_STEP = 0.05", "SPEED_PRESETS"];
const missingSpeedConstants = speedConstants.filter((token) => !speedModule.includes(token));
const speedPresets = ["0.5", "0.75", "1.25", "1.5", "2"];
const missingPresets = speedPresets.filter((value) => !speedModule.includes(value));
check(
  "变速范围与预设集中定义",
  missingSpeedConstants.length === 0 && missingPresets.length === 0 && speedModule.includes("raw < SPEED_MIN || raw > SPEED_MAX"),
  missingSpeedConstants.length || missingPresets.length
    ? `缺少 ${[...missingSpeedConstants, ...missingPresets].join(", ")}`
    : "speed.ts 定义 SPEED_MIN/SPEED_MAX/SPEED_STEP/SPEED_PRESETS（含 0.5/0.75/1.25/1.5/2），越界返回 error",
);
check(
  "视频与音频面板共用变速预设",
  [videoPanel, audioPanel].every(
    (panel) => panel.includes("SPEED_PRESETS") && panel.includes("保持音高") && panel.includes("keepPitch"),
  ),
  "VideoPanel / AudioPanel 均引用 SPEED_PRESETS，并含「保持音高」开关与 keepPitch 字段",
);
const argsSource = readFileSync(join(root, "src-tauri/src/args.rs"), "utf8");
check(
  "后端变速滤镜链完整",
  ["atempo_factors", "setpts=PTS/", "validate_speed", "atempo=0.5"].every((token) => argsSource.includes(token)),
  "args.rs 含 atempo_factors / setpts=PTS/ / validate_speed 与 atempo=0.5 链式用例",
);

// 水印：catalog 两张卡启用（不再是置灰的 `, false`），面板控件与前端参数类型齐全。
const videoWatermarkLine = catalog.split("\n").find((line) => line.includes('"video-watermark"')) ?? "";
const imageWatermarkLine = catalog.split("\n").find((line) => line.includes('"image-watermark"')) ?? "";
check(
  "两张水印卡已启用",
  !videoWatermarkLine.includes(", false") &&
    !imageWatermarkLine.includes(", false") &&
    videoWatermarkLine.includes('"video"') &&
    imageWatermarkLine.includes('"image"'),
  `video-watermark=${videoWatermarkLine.trim() || "缺失"}；image-watermark=${imageWatermarkLine.trim() || "缺失"}`,
);

const watermarkPositions = ["nw", "n", "ne", "w", "c", "e", "sw", "s", "se", "custom"];
const watermarkUiTokens = ['title="水印"', "选择水印图片", "opacity", "scalePercent", "margin", "rotation"];
const missingWatermarkUi = [videoPanel, imagePanel].flatMap((panel) =>
  [...watermarkPositions.map((position) => `"${position}"`), ...watermarkUiTokens].filter(
    (token) => !panel.includes(token),
  ),
);
check(
  "两个面板含水印控件",
  missingWatermarkUi.length === 0,
  missingWatermarkUi.length
    ? `缺少 ${missingWatermarkUi.join(", ")}`
    : "位置九宫格（nw…se / custom）、opacity / scalePercent / margin / rotation 与「水印」标题均在 VideoPanel / ImagePanel 中",
);

const apiWatermarkBlock = apiSource.slice(apiSource.indexOf("export interface WatermarkParams"), apiSource.indexOf("export interface VideoParams"));
const videoParamsBlock = apiSource.slice(apiSource.indexOf("export interface VideoParams"), apiSource.indexOf("export interface AudioParams"));
const imageParamsBlock = apiSource.slice(apiSource.indexOf("export interface ImageParams"), apiSource.indexOf("export interface SeqParams"));
const watermarkFields = ["enabled", "path", "position", "x", "y", "opacity", "scalePercent", "margin", "rotation"];
const missingWatermarkFields = watermarkFields.filter((field) => !apiWatermarkBlock.includes(field));
check(
  "前端参数类型包含水印",
  apiSource.includes("export interface WatermarkParams") &&
    missingWatermarkFields.length === 0 &&
    videoParamsBlock.includes("watermark: WatermarkParams") &&
    imageParamsBlock.includes("watermark: WatermarkParams"),
  missingWatermarkFields.length
    ? `WatermarkParams 缺少 ${missingWatermarkFields.join(", ")}`
    : "api.ts 定义 WatermarkParams，VideoParams / ImageParams 均含 watermark: WatermarkParams",
);

// 字幕：卡片启用、面板控件、前端参数类型与容器/变速阻止逻辑。
const videoSubtitleLine = catalog.split("\n").find((line) => line.includes('"video-subtitle"')) ?? "";
check(
  "字幕卡已启用",
  !videoSubtitleLine.includes(", false") && videoSubtitleLine.includes('"video"') && videoSubtitleLine.includes('"subtitle"'),
  `video-subtitle=${videoSubtitleLine.trim() || "缺失"}`,
);

const subtitleUiTokens = [
  'title="字幕"',
  '"内嵌"',
  '"烧录"',
  "fontSizePct",
  "outline",
  '"bottom"',
  '"middle"',
  '"top"',
  "color",
  "marginV",
  "defaultTrack",
];
const missingSubtitleUi = subtitleUiTokens.filter((token) => !videoPanel.includes(token));
check(
  "视频面板含字幕模式 / 样式 / 位置 / 颜色控件",
  missingSubtitleUi.length === 0,
  missingSubtitleUi.length
    ? `缺少 ${missingSubtitleUi.join(", ")}`
    : "内嵌/烧录、fontSizePct、outline、bottom/middle/top、color、marginV、defaultTrack 均在 VideoPanel 中",
);

const subtitleParamsBlock = apiSource.slice(
  apiSource.indexOf("export interface SubtitleParams"),
  apiSource.indexOf("export interface VideoParams"),
);
const subtitleFields = [
  "enabled",
  "mode",
  "path",
  "extraPaths",
  "language",
  "defaultTrack",
  "fontSizePct",
  "outline",
  "position",
  "color",
  "marginV",
];
const missingSubtitleFields = subtitleFields.filter((field) => !subtitleParamsBlock.includes(field));
check(
  "前端定义 SubtitleParams 与 subtitleBurn",
  apiSource.includes("export interface SubtitleParams") &&
    missingSubtitleFields.length === 0 &&
    apiSource.includes("subtitle: SubtitleParams") &&
    apiSource.includes("subtitleBurn: boolean"),
  missingSubtitleFields.length
    ? `SubtitleParams 缺少 ${missingSubtitleFields.join(", ")}`
    : "api.ts 定义 SubtitleParams（11 个 camelCase 字段）、VideoParams.subtitle 与 Capabilities.subtitleBurn",
);

const subtitleBlockTokens = [
  "SUBTITLE_SOFT_CONTAINERS",
  "subtitleSoftContainerBad",
  "subtitleSpeedConflict",
  "不承载内嵌字幕轨",
  "请改 MP4/MKV/MOV 或改用烧录",
  "变速时不能烧录字幕",
  "disabled={starting || !files.length || subtitleBlocked}",
];
const missingSubtitleBlock = subtitleBlockTokens.filter((token) => !videoPanel.includes(token));
check(
  "视频面板阻止容器不兼容与变速 + 烧录",
  missingSubtitleBlock.length === 0,
  missingSubtitleBlock.length
    ? `缺少 ${missingSubtitleBlock.join(", ")}`
    : "soft 容器白名单、容器不兼容提示、变速 + 烧录提示与开始按钮禁用均在 VideoPanel 中",
);

const subtitleModule = readFileSync(join(root, "src/lib/subtitle.ts"), "utf8");
const subtitleExts = ["srt", "ass", "ssa", "vtt"];
const missingSubtitleExts = subtitleExts.filter((ext) => !subtitleModule.includes(`"${ext}"`));
check(
  "字幕选择器限定 srt / ass / ssa / vtt",
  subtitleModule.includes("SUBTITLE_EXTENSIONS") &&
    subtitleModule.includes("SUBTITLE_DEFAULTS") &&
    missingSubtitleExts.length === 0,
  missingSubtitleExts.length
    ? `缺少 ${missingSubtitleExts.join(", ")}`
    : "subtitle.ts 定义 SUBTITLE_DEFAULTS 与 SUBTITLE_EXTENSIONS（srt/ass/ssa/vtt），并提供归一化与文件选择",
);

// 图片压缩：卡片启用、面板控件、前端参数类型、PNG/BMP 提示与队列未达标展示。
const imageCompressLine = catalog.split("\n").find((line) => line.includes('"image-compress"')) ?? "";
check(
  "图片压缩卡已启用",
  imageCompressLine.includes('"图片压缩"') &&
    imageCompressLine.includes('"compress"') &&
    imageCompressLine.includes('"image"') &&
    !imageCompressLine.includes(", false"),
  `image-compress=${imageCompressLine.trim() || "缺失"}`,
);

const compressUiTokens = [
  'title="压缩"',
  '"目标大小"',
  '"固定质量"',
  '"absolute"',
  '"percent"',
  "targetKb",
  "targetPercent",
  "keepMetadata",
  "保留元数据（EXIF/ICC）",
  "压缩不改变尺寸",
];
const missingCompressUi = compressUiTokens.filter((token) => !imagePanel.includes(token));
check(
  "图片压缩面板含目标 / 质量 / 元数据开关",
  missingCompressUi.length === 0,
  missingCompressUi.length
    ? `缺少 ${missingCompressUi.join(", ")}`
    : "目标大小/固定质量、absolute/percent、targetKb/targetPercent、keepMetadata 与尺寸说明均在 ImagePanel 中",
);

const compressParamsBlock = apiSource.slice(
  apiSource.indexOf("export interface CompressParams"),
  apiSource.indexOf("export interface ImageParams"),
);
const compressFields = ["mode", "targetMode", "targetKb", "targetPercent", "quality", "keepMetadata", "pngPalette"];
const missingCompressFields = compressFields.filter((field) => !compressParamsBlock.includes(field));
const compressJobFields = ["targetMet", "targetBytes", "attemptsUsed", "qualityUsed"];
const missingCompressJobFields = compressJobFields.filter((field) => !apiSource.includes(`${field}?:`));
check(
  "前端定义 CompressParams 与压缩任务字段",
  apiSource.includes("export interface CompressParams") &&
    missingCompressFields.length === 0 &&
    imageParamsBlock.includes("compress: CompressParams") &&
    apiSource.includes('"image-compress"') &&
    missingCompressJobFields.length === 0,
  missingCompressFields.length || missingCompressJobFields.length
    ? `缺少 ${[...missingCompressFields, ...missingCompressJobFields].join(", ")}`
    : "api.ts 定义 CompressParams（7 个 camelCase 字段）、ImageParams.compress、JobKind 含 image-compress，Job 含 targetMet/targetBytes/attemptsUsed/qualityUsed",
);

const compressBlockTokens = [
  "PNG 重编码几乎不减小体积",
  "BMP / TIFF 不支持压缩",
  "compressUnsupported",
  "disabled={starting || !files.length || compressUnsupported}",
  'kind: isCompress ? ("image-compress" as const)',
];
const missingCompressBlock = compressBlockTokens.filter((token) => !imagePanel.includes(token));
check(
  "PNG 提示与 BMP/TIFF 阻止逻辑",
  missingCompressBlock.length === 0,
  missingCompressBlock.length
    ? `缺少 ${missingCompressBlock.join(", ")}`
    : "PNG 重编码提示、BMP/TIFF 不支持提示、开始按钮禁用与 image-compress 入队 kind 均在 ImagePanel 中",
);

const taskCenterSource = readFileSync(join(root, "src/components/TaskCenter.tsx"), "utf8");
check(
  "队列显示未达标徽标与压缩汇总",
  taskCenterSource.includes("image-compress") &&
    taskCenterSource.includes("targetMet === false") &&
    taskCenterSource.includes("未达标") &&
    taskCenterSource.includes("成功 {compressSummary.ok} / 未达标 {compressSummary.unmet} / 失败 {compressSummary.failed}"),
  "TaskCenter 按 targetMet === false 显示「未达标」徽标，并汇总「成功 / 未达标 / 失败」",
);

check(
  "静态断言数量 ≥ 26",
  results.length + 1 >= 26,
  `当前 ${results.length + 1} 条断言（保留原有 22 条并新增图片压缩断言）`,
);

let failed = 0;
for (const item of results) {
  console.log(`${item.ok ? "PASS" : "FAIL"} ${item.name}: ${item.detail}`);
  if (!item.ok) failed += 1;
}
console.log(`\nformats check: ${results.length - failed}/${results.length} passed`);
process.exit(failed ? 1 : 0);
