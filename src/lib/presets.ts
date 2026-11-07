/**
 * 命名预设库（本地）：按方法粒度保存参数快照，供底部「保存为预设」与启动自动载入使用。
 *
 * 存储键：`transmutio:presets:v1`
 * 只保存面板参数中的数值 / 枚举 / 文本选项，**不保存绝对路径、目录与任何凭据**。
 */
import { findMethod } from "../catalog";
import { readNamespaced, writeNamespaced } from "./storage";

const STORE_KEY = "presets:v1";

export interface NamedPreset {
  id: string;
  methodId: string;
  name: string;
  params: Record<string, unknown>;
  naming?: { template: string; conflict: string };
  updatedAt: number;
}

interface PresetStore {
  presets: NamedPreset[];
  /** methodId → 上次使用的预设 id */
  last: Record<string, string>;
}

const EMPTY: PresetStore = { presets: [], last: {} };

/** 面板参数在 localStorage 里的键；与各面板 savePanelPrefs 的 key 保持一致。 */
const PANEL_KEY_OVERRIDES: Record<string, string> = {
  "video-merge": "video-video-merge",
  "video-mux": "video-video-mux",
  "video-gif": "video-video-gif",
  "image-seq-video": "seq:params",
  "video-frames": "frames:params",
  "image-collage": "collage:params",
  "image-exif": "exif:form",
  "image-avif": "avif:prefs",
};

const NAMING_KEY_OVERRIDES: Record<string, string> = {
  "image-seq-video": "seq:naming",
  "image-collage": "collage:naming",
};

/** 不进入预设的字段：路径、目录、凭据类一律剥离。 */
const FORBIDDEN = /(path|dir|token|secret|cookie|password|credential)/i;

export function panelKeyFor(methodId: string): string | null {
  if (PANEL_KEY_OVERRIDES[methodId]) return PANEL_KEY_OVERRIDES[methodId];
  const method = findMethod(methodId);
  if (!method?.panel) return null;
  if (method.panel === "sequence") return "seq:params";
  if (method.panel === "frames") return "frames:params";
  if (method.panel === "collage") return "collage:params";
  if (method.panel === "exif") return "exif:form";
  if (method.panel === "avif") return "avif:prefs";
  return `${method.panel}:${methodId}`;
}

export function namingKeyFor(methodId: string): string {
  return NAMING_KEY_OVERRIDES[methodId] ?? `naming:${methodId}`;
}

/** 读取某方法当前面板参数（用于保存快照与速览摘要）。 */
export function readMethodParams(methodId: string): Record<string, unknown> | null {
  const key = panelKeyFor(methodId);
  if (!key) return null;
  const raw = readNamespaced("panel:" + key);
  if (!raw) return null;
  try {
    const parsed: unknown = JSON.parse(raw);
    if (!parsed || typeof parsed !== "object" || Array.isArray(parsed)) return null;
    return parsed as Record<string, unknown>;
  } catch {
    return null;
  }
}

function sanitize(params: Record<string, unknown>): Record<string, unknown> {
  const out: Record<string, unknown> = {};
  for (const [key, value] of Object.entries(params)) {
    if (FORBIDDEN.test(key)) continue;
    if (typeof value === "string" || typeof value === "number" || typeof value === "boolean") {
      out[key] = value;
    }
  }
  return out;
}

function readStore(): PresetStore {
  const raw = readNamespaced(STORE_KEY);
  if (!raw) return { ...EMPTY, presets: [], last: {} };
  try {
    const parsed = JSON.parse(raw) as Partial<PresetStore>;
    return {
      presets: Array.isArray(parsed.presets) ? parsed.presets : [],
      last: parsed.last && typeof parsed.last === "object" ? parsed.last : {},
    };
  } catch {
    return { ...EMPTY, presets: [], last: {} };
  }
}

function writeStore(store: PresetStore): void {
  writeNamespaced(STORE_KEY, JSON.stringify(store));
}

let seq = 0;
function nextId(): string {
  seq += 1;
  return `p${Date.now().toString(36)}${seq.toString(36)}`;
}

export function listPresets(methodId: string): NamedPreset[] {
  return readStore()
    .presets.filter((preset) => preset.methodId === methodId)
    .sort((a, b) => b.updatedAt - a.updatedAt);
}

export function allPresets(): NamedPreset[] {
  return readStore().presets.slice().sort((a, b) => b.updatedAt - a.updatedAt);
}

/** 保存新预设；同一方法下重名视为覆盖。 */
export function createPreset(
  methodId: string,
  name: string,
  params: Record<string, unknown>,
  naming?: { template: string; conflict: string },
): NamedPreset {
  const store = readStore();
  const trimmed = name.trim();
  const existing = store.presets.find(
    (preset) => preset.methodId === methodId && preset.name === trimmed,
  );
  const preset: NamedPreset = {
    id: existing?.id ?? nextId(),
    methodId,
    name: trimmed,
    params: sanitize(params),
    naming,
    updatedAt: Date.now(),
  };
  store.presets = existing
    ? store.presets.map((item) => (item.id === existing.id ? preset : item))
    : [...store.presets, preset];
  store.last[methodId] = preset.id;
  writeStore(store);
  return preset;
}

export function renamePreset(id: string, name: string): void {
  const store = readStore();
  store.presets = store.presets.map((preset) =>
    preset.id === id ? { ...preset, name: name.trim(), updatedAt: Date.now() } : preset,
  );
  writeStore(store);
}

export function deletePreset(id: string): void {
  const store = readStore();
  const target = store.presets.find((preset) => preset.id === id);
  store.presets = store.presets.filter((preset) => preset.id !== id);
  if (target && store.last[target.methodId] === id) delete store.last[target.methodId];
  writeStore(store);
}

/** 写回某方法的面板参数与命名选项；调用方负责重建面板以读取新值。 */
export function applyPreset(preset: NamedPreset): void {
  const key = panelKeyFor(preset.methodId);
  if (!key) return;
  writeNamespaced("panel:" + key, JSON.stringify(preset.params));
  if (preset.naming) {
    writeNamespaced("panel:" + namingKeyFor(preset.methodId), JSON.stringify(preset.naming));
  }
  rememberLastPreset(preset.methodId, preset.id);
}

export function rememberLastPreset(methodId: string, id: string): void {
  const store = readStore();
  store.last[methodId] = id;
  writeStore(store);
}

export function lastPresetFor(methodId: string): NamedPreset | null {
  const store = readStore();
  const id = store.last[methodId];
  if (!id) return null;
  return store.presets.find((preset) => preset.id === id) ?? null;
}

/** 摘要行：从参数里挑 2–3 个可读键值，避免把整块 JSON 塞进卡片。 */
export function summarizeParams(params: Record<string, unknown> | null): string {
  if (!params) return "";
  const parts: string[] = [];
  for (const [key, value] of Object.entries(params)) {
    if (parts.length >= 3) break;
    if (value === "" || value === null || value === undefined) continue;
    if (typeof value === "object") continue;
    const text = typeof value === "boolean" ? (value ? "开" : "关") : String(value);
    if (text.length > 18) continue;
    parts.push(`${key}=${text}`);
  }
  return parts.join(" · ");
}
