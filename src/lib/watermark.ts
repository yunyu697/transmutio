/**
 * 水印参数的默认值与归一化。后端 WatermarkParams 为 camelCase + default，
 * 这里保持同名字段；旧版本持久化数据缺少字段时补齐默认值，避免把 undefined 传给后端。
 */
import type { WatermarkParams } from "./api";

export const WATERMARK_DEFAULTS: WatermarkParams = {
  enabled: false,
  mode: "image",
  path: "",
  text: "",
  fontPath: "",
  fontSizePercent: 4.5,
  fontColor: "#FFFFFF",
  borderWidth: 2,
  borderColor: "#000000",
  position: "se",
  x: 0,
  y: 0,
  opacity: 0.9,
  scalePercent: 15,
  margin: 24,
  rotation: 0,
};

const POSITIONS = ["nw", "n", "ne", "w", "c", "e", "sw", "s", "se", "custom"];

const clamp = (value: number, min: number, max: number) => Math.min(max, Math.max(min, value));

export function normalizeWatermark(value: unknown): WatermarkParams {
  const raw = (value && typeof value === "object" ? value : {}) as Partial<WatermarkParams>;
  const num = (input: unknown, fallback: number) =>
    typeof input === "number" && Number.isFinite(input) ? input : fallback;
  return {
    enabled: typeof raw.enabled === "boolean" ? raw.enabled : WATERMARK_DEFAULTS.enabled,
    mode: raw.mode === "text" ? "text" : "image",
    path: typeof raw.path === "string" ? raw.path : WATERMARK_DEFAULTS.path,
    text: typeof raw.text === "string" ? raw.text : WATERMARK_DEFAULTS.text,
    fontPath: typeof raw.fontPath === "string" ? raw.fontPath : WATERMARK_DEFAULTS.fontPath,
    fontSizePercent: clamp(num(raw.fontSizePercent, WATERMARK_DEFAULTS.fontSizePercent), 1, 20),
    fontColor:
      typeof raw.fontColor === "string" && /^#[0-9a-f]{6}$/i.test(raw.fontColor)
        ? raw.fontColor
        : WATERMARK_DEFAULTS.fontColor,
    borderWidth: clamp(Math.round(num(raw.borderWidth, WATERMARK_DEFAULTS.borderWidth)), 0, 20),
    borderColor:
      typeof raw.borderColor === "string" && /^#[0-9a-f]{6}$/i.test(raw.borderColor)
        ? raw.borderColor
        : WATERMARK_DEFAULTS.borderColor,
    position:
      typeof raw.position === "string" && POSITIONS.includes(raw.position)
        ? raw.position
        : WATERMARK_DEFAULTS.position,
    x: Math.round(num(raw.x, WATERMARK_DEFAULTS.x)),
    y: Math.round(num(raw.y, WATERMARK_DEFAULTS.y)),
    opacity: clamp(num(raw.opacity, WATERMARK_DEFAULTS.opacity), 0, 1),
    scalePercent: clamp(Math.round(num(raw.scalePercent, WATERMARK_DEFAULTS.scalePercent)), 5, 50),
    margin: Math.max(0, Math.round(num(raw.margin, WATERMARK_DEFAULTS.margin))),
    rotation: clamp(Math.round(num(raw.rotation, WATERMARK_DEFAULTS.rotation)), -180, 180),
  };
}

/** 选择水印图片：仅 png / jpg / jpeg / webp（后端同样只接受这四种）。 */
export async function pickWatermarkImage(): Promise<string | null> {
  const { open } = await import("@tauri-apps/plugin-dialog");
  const picked = await open({
    multiple: false,
    directory: false,
    filters: [{ name: "水印图片", extensions: ["png", "jpg", "jpeg", "webp"] }],
  });
  return typeof picked === "string" ? picked : null;
}

/** 选择字体文件：ttf / otf / ttc（使用系统字体，不随应用打包）。 */
export async function pickFontFile(): Promise<string | null> {
  const { open } = await import("@tauri-apps/plugin-dialog");
  const picked = await open({
    multiple: false,
    directory: false,
    filters: [{ name: "字体文件", extensions: ["ttf", "otf", "ttc"] }],
  });
  return typeof picked === "string" ? picked : null;
}
