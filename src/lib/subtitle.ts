/**
 * 字幕参数的默认值与归一化。后端 SubtitleParams 为 camelCase + default，
 * 这里保持同名字段；旧版本持久化数据缺少字段时补齐默认值，并按后端范围夹取，
 * 避免把越界值或 undefined 传给后端。
 */
import type { SubtitleParams } from "./api";

export const SUBTITLE_DEFAULTS: SubtitleParams = {
  enabled: false,
  mode: "soft",
  path: "",
  extraPaths: [],
  language: "chi",
  defaultTrack: false,
  fontSizePct: 4.5,
  outline: 2,
  position: "bottom",
  color: "#FFFFFF",
  marginV: 24,
};

/** 后端只接受这四种字幕格式。 */
export const SUBTITLE_EXTENSIONS = ["srt", "ass", "ssa", "vtt"];

/** 内嵌字幕仅 mp4 / mov / mkv 容器承载；webm / ts / avi 会被后端拒绝。 */
export const SUBTITLE_SOFT_CONTAINERS = ["mp4", "mov", "mkv"];

const POSITIONS = ["bottom", "middle", "top"];
const COLOR_RE = /^#[0-9a-fA-F]{6}$/;

const clamp = (value: number, min: number, max: number) => Math.min(max, Math.max(min, value));

export function normalizeSubtitle(value: unknown): SubtitleParams {
  const raw = (value && typeof value === "object" ? value : {}) as Partial<SubtitleParams>;
  const num = (input: unknown, fallback: number) =>
    typeof input === "number" && Number.isFinite(input) ? input : fallback;
  const mode = raw.mode === "hard" ? "hard" : "soft";
  const mainPath = typeof raw.path === "string" ? raw.path.trim() : SUBTITLE_DEFAULTS.path;
  const extras =
    mode === "soft" && Array.isArray(raw.extraPaths)
      ? raw.extraPaths
          .filter((item): item is string => typeof item === "string")
          .map((item) => item.trim())
          .filter((item) => item && item !== mainPath && SUBTITLE_EXTENSIONS.includes(item.split(".").pop()?.toLowerCase() ?? ""))
          .filter((item, index, list) => list.indexOf(item) === index)
      : [];
  return {
    enabled: typeof raw.enabled === "boolean" ? raw.enabled : SUBTITLE_DEFAULTS.enabled,
    mode,
    path: mainPath,
    extraPaths: extras,
    language:
      typeof raw.language === "string" && raw.language.trim()
        ? raw.language.trim()
        : SUBTITLE_DEFAULTS.language,
    defaultTrack: typeof raw.defaultTrack === "boolean" ? raw.defaultTrack : SUBTITLE_DEFAULTS.defaultTrack,
    fontSizePct: clamp(Math.round(num(raw.fontSizePct, SUBTITLE_DEFAULTS.fontSizePct) * 2) / 2, 1, 20),
    outline: clamp(Math.round(num(raw.outline, SUBTITLE_DEFAULTS.outline)), 0, 6),
    position:
      typeof raw.position === "string" && POSITIONS.includes(raw.position)
        ? (raw.position as SubtitleParams["position"])
        : SUBTITLE_DEFAULTS.position,
    color:
      typeof raw.color === "string" && COLOR_RE.test(raw.color.trim())
        ? raw.color.trim().toUpperCase()
        : SUBTITLE_DEFAULTS.color,
    marginV: Math.max(0, Math.round(num(raw.marginV, SUBTITLE_DEFAULTS.marginV))),
  };
}

/** 选择字幕文件：仅 srt / ass / ssa / vtt（后端同样只接受这四种）。 */
export async function pickSubtitleFile(multiple = false): Promise<string[] | null> {
  const { open } = await import("@tauri-apps/plugin-dialog");
  const picked = await open({
    multiple,
    directory: false,
    filters: [{ name: "字幕文件", extensions: SUBTITLE_EXTENSIONS }],
  });
  if (!picked) return null;
  return Array.isArray(picked) ? picked : [picked];
}
