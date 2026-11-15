/**
 * 队列剪贴板：只搬列表项，不复制/移动磁盘文件。
 *
 * - 复制：记住路径，并把路径文本写入系统剪贴板；
 * - 剪切：同上，额外标记为「待移动」，列表里标灰；
 * - 粘贴：把项移到落点（没有落点则末尾），粘贴后清掉剪切标记。
 *
 * 状态放在模块级，跨面板切换仍然有效；React 侧用 useSyncExternalStore 订阅。
 */
export type QueueClipboardMode = "copy" | "cut";

export interface QueueClipboard {
  mode: QueueClipboardMode;
  paths: string[];
}

let state: QueueClipboard | null = null;
const listeners = new Set<() => void>();

function emit() {
  for (const listener of listeners) listener();
}

export function subscribeQueueClipboard(listener: () => void): () => void {
  listeners.add(listener);
  return () => listeners.delete(listener);
}

export function getQueueClipboard(): QueueClipboard | null {
  return state;
}

export function setQueueClipboard(mode: QueueClipboardMode, paths: readonly string[]): void {
  state = paths.length ? { mode, paths: [...paths] } : null;
  emit();
}

export function clearQueueClipboard(): void {
  if (!state) return;
  state = null;
  emit();
}

/** 系统剪贴板：写路径文本；失败时静默降级（内部剪贴板仍然可用）。 */
export async function writePathsToClipboard(paths: readonly string[]): Promise<boolean> {
  const text = paths.join("\n");
  try {
    if (navigator.clipboard?.writeText) {
      await navigator.clipboard.writeText(text);
      return true;
    }
  } catch {
    /* 权限或安全上下文问题，走下面的兜底 */
  }
  try {
    const area = document.createElement("textarea");
    area.value = text;
    area.setAttribute("readonly", "");
    area.style.position = "fixed";
    area.style.opacity = "0";
    document.body.appendChild(area);
    area.select();
    const ok = document.execCommand("copy");
    document.body.removeChild(area);
    return ok;
  } catch {
    return false;
  }
}

/** 从系统剪贴板文本导入路径（换行分隔）；读不到就返回空数组。 */
export async function readPathsFromClipboard(): Promise<string[]> {
  try {
    const text = await navigator.clipboard.readText();
    return text
      .split(/\r?\n/)
      .map((line) => line.trim().replace(/^"|"$/g, ""))
      .filter(Boolean);
  } catch {
    return [];
  }
}
