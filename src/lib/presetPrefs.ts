/**
 * 预设与本地面板参数的桥接：导出时只收集白名单字段（与后端 preset.rs 的字段
 * 白名单一致），导入时按面板写回 localStorage。路径、输出目录与凭据永不进入预设。
 */
import { readNamespaced, writeNamespaced } from "./storage";

const PREFIX = "panel:";

const FIELDS: Record<string, string[]> = {
  video: [
    "container", "codec", "encoder", "rateMode", "quality", "bitrateKbps", "resolution", "fps", "crop",
    "rotation", "audioMode", "audioBitrateKbps", "audioRateMode", "hwDecode", "faststart",
  ],
  audio: ["format", "bitrateKbps", "rateMode", "sampleRate", "channels", "normalize"],
  image: [
    "format", "quality", "maxEdge", "rotation", "autoOrient", "cropMode", "cropRatio",
    "cropX", "cropY", "cropW", "cropH",
  ],
  merge: ["mode", "resolution", "fps", "audioMode", "codec", "quality", "bitrateKbps"],
  mux: ["mode", "audioMode", "audioBitrateKbps", "audioRateMode"],
  gif: ["fps", "width", "startSec", "durationSec", "optimize"],
  seq: ["fps", "sizeMode", "width", "height", "encoder", "quality", "bitrateKbps"],
  frames: ["fps", "width", "format", "quality"],
  collage: ["cols", "rows", "cellWidth", "cellHeight", "gap", "background", "fit", "format", "quality"],
};

/** 每类面板导出时读取的本地键；同一面板类型有多个入口时取第一个作为代表。 */
const SOURCE_KEYS: Record<string, string> = {
  video: "video:video-mp4",
  audio: "audio:audio-aac",
  image: "image:image-convert",
  merge: "video-video-merge",
  mux: "video-video-mux",
  gif: "video-video-gif",
  seq: "seq:params",
  frames: "frames:params",
  collage: "collage:params",
};

/** 导入时写回的目标：面板参数键 + 需要同步命名策略的方法。 */
const APPLY_TARGETS: Record<string, { panelKeys: string[]; namingMethods: string[] }> = {
  video: {
    panelKeys: ["video:video-mp4", "video:video-compress", "video:video-crop"],
    namingMethods: ["video-mp4", "video-compress", "video-crop"],
  },
  audio: {
    panelKeys: ["audio:audio-mp3", "audio:audio-aac", "audio:audio-volume", "audio:audio-merge"],
    namingMethods: ["audio-mp3", "audio-aac", "audio-volume", "audio-merge", "audio-trim"],
  },
  image: {
    panelKeys: ["image:image-convert", "image:image-resize", "image:image-crop"],
    namingMethods: ["image-convert", "image-resize", "image-crop"],
  },
  merge: { panelKeys: ["video-video-merge"], namingMethods: ["video-merge"] },
  mux: { panelKeys: ["video-video-mux"], namingMethods: ["video-mux"] },
  gif: { panelKeys: ["video-video-gif"], namingMethods: ["video-gif"] },
  seq: { panelKeys: ["seq:params"], namingMethods: [] },
  frames: { panelKeys: ["frames:params"], namingMethods: [] },
  collage: { panelKeys: ["collage:params"], namingMethods: ["image-collage"] },
};

function readObject(key: string): Record<string, unknown> | null {
  try {
    const raw = readNamespaced(PREFIX + key);
    if (!raw) return null;
    const parsed: unknown = JSON.parse(raw);
    if (!parsed || typeof parsed !== "object" || Array.isArray(parsed)) return null;
    return parsed as Record<string, unknown>;
  } catch {
    return null;
  }
}

function pickFields(source: Record<string, unknown>, allowed: string[]): Record<string, unknown> {
  const result: Record<string, unknown> = {};
  for (const field of allowed) {
    if (field in source) result[field] = source[field];
  }
  return result;
}

function writeObject(key: string, value: Record<string, unknown>): void {
  try {
    writeNamespaced(PREFIX + key, JSON.stringify(value));
  } catch {
    // 配额不足时导出/导入仍然成功，只是本次参数不持久化。
  }
}

export interface CollectedPreset {
  panels: Record<string, unknown>;
  output: Record<string, unknown>;
}

/** 收集当前保存的面板参数与输出策略；没有保存过的面板不写入预设。 */
export function collectPreset(afterAction: string): CollectedPreset {
  const panels: Record<string, unknown> = {};
  for (const [name, key] of Object.entries(SOURCE_KEYS)) {
    const saved = readObject(key);
    if (saved) panels[name] = pickFields(saved, FIELDS[name]);
  }

  // 输出策略来自面板的「输出命名」；取第一个保存过命名的入口作为代表。
  let naming: { template?: unknown; conflict?: unknown } = {};
  for (const method of ["video-mp4", "image-convert", "audio-mp3", "video-merge"]) {
    const saved = readObject(`naming:${method}`);
    if (saved) {
      naming = saved;
      break;
    }
  }
  const output: Record<string, unknown> = {
    conflict: typeof naming.conflict === "string" ? naming.conflict : "suffix",
  };
  if (typeof naming.template === "string") output.nameTemplate = naming.template;
  if (afterAction) output.afterAction = afterAction;
  return { panels, output };
}

export type AfterAction = "none" | "open-folder" | "shutdown";

const AFTER_ACTIONS: AfterAction[] = ["none", "open-folder", "shutdown"];

export interface AppliedPreset {
  /** 写回成功、需要重新打开面板才生效的类别。 */
  applied: string[];
  /** 预设里的「批次完成后」动作；未提供或非法时为 null（保持当前设置）。 */
  afterAction: AfterAction | null;
}

/** 把校验通过的预设写回本地面板参数；返回写入的面板名称，便于提示。 */
export function applyPreset(preset: {
  panels: Record<string, unknown>;
  output: Record<string, unknown>;
}): AppliedPreset {
  const applied: string[] = [];
  for (const [name, value] of Object.entries(preset.panels)) {
    if (!value || typeof value !== "object" || Array.isArray(value)) continue;
    const fields = pickFields(value as Record<string, unknown>, FIELDS[name] ?? []);
    const target = APPLY_TARGETS[name];
    if (!target) continue;
    for (const key of target.panelKeys) writeObject(key, fields);
    applied.push(name);
  }

  const conflict = typeof preset.output.conflict === "string" ? preset.output.conflict : "suffix";
  const template = typeof preset.output.nameTemplate === "string" ? preset.output.nameTemplate : "";
  const namingMethods = Object.values(APPLY_TARGETS).flatMap((target) => target.namingMethods);
  for (const method of namingMethods) {
    writeObject(`naming:${method}`, { template, conflict });
  }
  const rawAction = preset.output.afterAction;
  const afterAction =
    typeof rawAction === "string" && (AFTER_ACTIONS as string[]).includes(rawAction)
      ? (rawAction as AfterAction)
      : null;
  return { applied, afterAction };
}
