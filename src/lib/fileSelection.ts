/**
 * 文件列表的选择模型（纯逻辑，无 DOM / React）。
 *
 * 约定：
 * - id 就是文件 path，列表按 path 去重；
 * - 所有范围计算都以「当前可见顺序」（order）为准，排序后自动跟随；
 * - anchor = Shift 范围起点，lead = 键盘焦点行，两者都是 path 而不是下标。
 */

export interface SelectionState {
  ids: ReadonlySet<string>;
  anchor: string | null;
  lead: string | null;
}

export function emptySelection(): SelectionState {
  return { ids: new Set(), anchor: null, lead: null };
}

function build(ids: Iterable<string>, anchor: string | null, lead: string | null): SelectionState {
  return { ids: new Set(ids), anchor, lead };
}

/** 只选中一项（普通点击）。 */
export function selectOnly(id: string): SelectionState {
  return build([id], id, id);
}

/** Ctrl 点击：已选则取消，未选则加入；焦点与锚点都落到被点项。 */
export function toggle(state: SelectionState, id: string): SelectionState {
  const ids = new Set(state.ids);
  if (ids.has(id)) ids.delete(id);
  else ids.add(id);
  return build(ids, id, id);
}

/**
 * Shift 点击 / Shift 方向键：从锚点到目标的连续范围。
 * `additive`（Ctrl+Shift）把范围并入已有选择，否则替换。
 * 锚点缺省时退化为当前焦点，再退化为目标本身。
 */
export function rangeTo(
  state: SelectionState,
  order: readonly string[],
  id: string,
  additive = false,
): SelectionState {
  const target = order.indexOf(id);
  if (target < 0) return state;
  const rawAnchor = state.anchor ?? state.lead ?? id;
  const anchorIndex = order.indexOf(rawAnchor);
  const from = anchorIndex < 0 ? target : anchorIndex;
  const start = Math.min(from, target);
  const end = Math.max(from, target);
  const ids = additive ? new Set(state.ids) : new Set<string>();
  for (let i = start; i <= end; i += 1) ids.add(order[i]);
  return build(ids, anchorIndex < 0 ? id : rawAnchor, id);
}

export function selectAll(order: readonly string[]): SelectionState {
  const lead = order.length ? order[order.length - 1] : null;
  return build(order, order[0] ?? null, lead);
}

export function clearSelection(): SelectionState {
  return emptySelection();
}

/** 焦点移动到相对位置的项；extend=true 时按 Shift 扩展范围。 */
export function moveFocus(
  state: SelectionState,
  order: readonly string[],
  delta: number,
  extend = false,
): SelectionState {
  if (!order.length) return state;
  const current = state.lead ?? state.anchor;
  const index = current ? order.indexOf(current) : -1;
  const base = index < 0 ? (delta > 0 ? -1 : order.length) : index;
  const next = Math.min(order.length - 1, Math.max(0, base + delta));
  const id = order[next];
  if (extend) {
    const anchored = state.anchor ?? state.lead ?? id;
    return rangeTo({ ...state, anchor: anchored }, order, id, false);
  }
  return selectOnly(id);
}

/** Home / End：跳到首尾；extend=true 时按 Shift 扩展范围。 */
export function focusEdge(
  state: SelectionState,
  order: readonly string[],
  edge: "start" | "end",
  extend = false,
): SelectionState {
  if (!order.length) return state;
  const id = edge === "start" ? order[0] : order[order.length - 1];
  if (!extend) return selectOnly(id);
  const anchored = state.anchor ?? state.lead ?? id;
  return rangeTo({ ...state, anchor: anchored }, order, id, false);
}

/**
 * 框选实时命中：additive（按住 Ctrl/Shift）时并入基础选择，否则替换。
 * 命中为空且非追加时等价于「点空白」——清空。
 */
export function applyMarquee(
  state: SelectionState,
  order: readonly string[],
  hits: readonly string[],
  additive = false,
): SelectionState {
  if (!hits.length && additive) return state;
  if (!hits.length) return emptySelection();
  const hitSet = new Set(hits);
  const lead = order.filter((id) => hitSet.has(id)).pop() ?? hits[hits.length - 1];
  if (!additive) return build(hits, hits[0], lead);
  const ids = new Set(state.ids);
  for (const id of hits) ids.add(id);
  return build(ids, state.anchor ?? hits[0], lead);
}

/** 文件被移除 / 重命名后，裁掉已不存在的 id 并修正锚点与焦点。 */
export function pruneSelection(state: SelectionState, order: readonly string[]): SelectionState {
  const visible = new Set(order);
  const ids = new Set<string>();
  for (const id of state.ids) if (visible.has(id)) ids.add(id);
  const anchor =
    state.anchor && visible.has(state.anchor)
      ? state.anchor
      : state.lead && visible.has(state.lead)
        ? state.lead
        : (order[0] ?? null);
  const lead =
    state.lead && visible.has(state.lead)
      ? state.lead
      : anchor && visible.has(anchor)
        ? anchor
        : (order[0] ?? null);
  if (ids.size === state.ids.size && anchor === state.anchor && lead === state.lead) return state;
  return build(ids, anchor, lead);
}

/** 重命名后把选择状态里的旧 path 替换成新 path。 */
export function renameInSelection(state: SelectionState, from: string, to: string): SelectionState {
  const ids = new Set<string>();
  for (const id of state.ids) ids.add(id === from ? to : id);
  return build(ids, state.anchor === from ? to : state.anchor, state.lead === from ? to : state.lead);
}

/** 按可见顺序输出选中项，供「整体拖动」和复制使用。 */
export function orderedSelection(order: readonly string[], ids: ReadonlySet<string>): string[] {
  return order.filter((id) => ids.has(id));
}

export function selectionCount(state: SelectionState): number {
  return state.ids.size;
}
