/**
 * 水印参数的默认值与归一化。后端 WatermarkParams 为 camelCase + default，
 * 这里保持同名字段；旧版本持久化数据缺少字段时补齐默认值，避免把 undefined 传给后端。
 */
import type { WatermarkParams } from "./api";

export const WATERMARK_DEFAULTS: WatermarkParams = {
  enabled: false,
  path: "",
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
    path: typeof raw.path === "string" ? raw.path : WATERMARK_DEFAULTS.path,
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
