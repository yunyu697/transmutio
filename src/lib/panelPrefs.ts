/// 面板参数持久化：按「面板:方法」隔离，避免不同方法的默认值互相污染。
import { readNamespaced, writeNamespaced } from "./storage";

const PREFIX = "panel:";

export function loadPanelPrefs<T extends object>(key: string, defaults: T): T {
  const raw = readNamespaced(PREFIX + key);
  if (!raw) return defaults;
  try {
    const parsed: unknown = JSON.parse(raw);
    if (!parsed || typeof parsed !== "object" || Array.isArray(parsed)) return defaults;
    return { ...defaults, ...(parsed as Partial<T>) };
  } catch {
    return defaults;
  }
}

export function savePanelPrefs<T extends object>(key: string, value: T): void {
  try {
    writeNamespaced(PREFIX + key, JSON.stringify(value));
  } catch {
    // 隐私模式 / 配额不足时静默降级为「不持久化」，不影响本次转换。
  }
}
