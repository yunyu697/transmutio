/**
 * 文件列表的顺序模型（纯逻辑，无 DOM / React）。
 *
 * 列表顺序就是真实处理顺序（视频合并 / 音频拼接按下标入队），
 * 因此排序与拖动都直接改这一份顺序，不额外维护「视图顺序」。
 */

export type SortKey = "name" | "type" | "size" | "duration";
export type SortDir = "asc" | "desc";

/** 列表项需要的最小字段；MediaInfo 结构性兼容。 */
export interface SortableEntry {
  name: string;
  ext: string;
  sizeBytes: number;
  durationSec: number | null;
}

/** 自然排序：img2 排在 img10 之前。 */
export function naturalCompare(a: string, b: string): number {
  return a.localeCompare(b, undefined, { numeric: true, sensitivity: "base" });
}

/**
 * 把选中项整理成连续块，整体插入到 `targetIndex`（按原顺序的下标，0..len）。
 * 落点位于选中块内部时顺序保持不变。
 */
export function moveBlock(
  order: readonly string[],
  moving: ReadonlySet<string>,
  targetIndex: number,
): string[] {
  const block = order.filter((id) => moving.has(id));
  if (!block.length) return order.slice();
  const clamped = Math.max(0, Math.min(order.length, targetIndex));
  const remaining = order.filter((id) => !moving.has(id));
  let insertAt = 0;
  for (let i = 0; i < clamped; i += 1) if (!moving.has(order[i])) insertAt += 1;
  return [...remaining.slice(0, insertAt), ...block, ...remaining.slice(insertAt)];
}

/** 排序只改变列表本身的顺序；排序键缺失的值（时长）恒排最后。 */
export function sortEntries<T extends SortableEntry>(
  items: readonly T[],
  key: SortKey,
  dir: SortDir,
): T[] {
  const factor = dir === "asc" ? 1 : -1;
  return items.slice().sort((a, b) => {
    let primary = 0;
    if (key === "name") {
      primary = naturalCompare(a.name, b.name);
    } else if (key === "type") {
      primary = naturalCompare(a.ext, b.ext);
    } else if (key === "size") {
      primary = a.sizeBytes - b.sizeBytes;
    } else {
      if (a.durationSec == null && b.durationSec == null) primary = 0;
      else if (a.durationSec == null) return 1;
      else if (b.durationSec == null) return -1;
      else primary = a.durationSec - b.durationSec;
    }
    if (primary !== 0) return primary * factor;
    if (key === "name") return 0;
    return naturalCompare(a.name, b.name);
  });
}
