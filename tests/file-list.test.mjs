import test from "node:test";
import assert from "node:assert/strict";
import {
  applyMarquee,
  clearSelection,
  emptySelection,
  focusEdge,
  moveFocus,
  orderedSelection,
  pruneSelection,
  rangeTo,
  renameInSelection,
  selectAll,
  selectOnly,
  toggle,
} from "../src/lib/fileSelection.ts";
import { moveBlock, naturalCompare, sortEntries } from "../src/lib/fileQueue.ts";
import {
  registerDropController,
  registerDropLayer,
  resetDropRegistry,
  resolveDropTarget,
  routeDrop,
} from "../src/lib/fileDropTarget.ts";

const order = ["a", "b", "c", "d", "e"];

test("普通点击只选中一项并把锚点放到该项", () => {
  const state = selectOnly("c");
  assert.deepEqual([...state.ids], ["c"]);
  assert.equal(state.anchor, "c");
  assert.equal(state.lead, "c");
});

test("Ctrl 点击可以切换选中状态", () => {
  let state = selectOnly("b");
  state = toggle(state, "d");
  assert.deepEqual([...state.ids].sort(), ["b", "d"]);
  state = toggle(state, "b");
  assert.deepEqual([...state.ids], ["d"]);
});

test("Shift 从锚点按可见顺序取范围，反向也成立", () => {
  const state = rangeTo(selectOnly("b"), order, "d");
  assert.deepEqual(orderedSelection(order, state.ids), ["b", "c", "d"]);
  const backward = rangeTo(state, order, "a");
  assert.deepEqual(orderedSelection(order, backward.ids), ["a", "b"]);
});

test("Shift 范围替换旧选择，Ctrl+Shift 增量并入", () => {
  let state = selectOnly("b");
  state = rangeTo(state, order, "d");
  assert.deepEqual(orderedSelection(order, state.ids), ["b", "c", "d"]);
  state = rangeTo(state, order, "e");
  // 锚点仍是 b，范围 b..e 替换掉旧范围
  assert.deepEqual(orderedSelection(order, state.ids), ["b", "c", "d", "e"]);

  let additive = selectOnly("a");
  additive = rangeTo(additive, order, "b");
  additive = rangeTo(additive, order, "d", true);
  // 锚点仍是 a，从 a 扩到 d 并与既有 a、b 合并
  assert.deepEqual(orderedSelection(order, additive.ids), ["a", "b", "c", "d"]);
});

test("方向键移动焦点，Shift 扩展范围", () => {
  let state = selectOnly("c");
  state = moveFocus(state, order, 1);
  assert.deepEqual([...state.ids], ["d"]);
  state = moveFocus(state, order, -1, true);
  assert.deepEqual(orderedSelection(order, state.ids), ["c", "d"]);
});

test("Home / End 跳首尾，Shift 时扩展", () => {
  let state = focusEdge(selectOnly("c"), order, "end");
  assert.deepEqual([...state.ids], ["e"]);
  state = focusEdge(selectOnly("c"), order, "start", true);
  assert.deepEqual(orderedSelection(order, state.ids), ["a", "b", "c"]);
});

test("框选替换 / 追加 / 空命中清空", () => {
  const base = selectOnly("a");
  const replaced = applyMarquee(base, order, ["c", "d"]);
  assert.deepEqual(orderedSelection(order, replaced.ids), ["c", "d"]);
  const additive = applyMarquee(base, order, ["c", "d"], true);
  assert.deepEqual(orderedSelection(order, additive.ids), ["a", "c", "d"]);
  assert.equal(applyMarquee(base, order, []).ids.size, 0);
  assert.equal(applyMarquee(base, order, [], true), base);
});

test("全选与清空", () => {
  const all = selectAll(order);
  assert.equal(all.ids.size, order.length);
  assert.equal(all.anchor, "a");
  assert.equal(all.lead, "e");
  assert.equal(clearSelection().ids.size, 0);
});

test("排序后范围与焦点按新的可见顺序走", () => {
  const sorted = ["e", "d", "c", "b", "a"];
  const state = rangeTo(selectOnly("d"), sorted, "b");
  assert.deepEqual(orderedSelection(sorted, state.ids), ["d", "c", "b"]);
  const moved = moveFocus(selectOnly("d"), sorted, 1);
  assert.deepEqual([...moved.ids], ["c"]);
});

test("列表变化后裁掉不存在的选中项", () => {
  const state = applyMarquee(emptySelection(), order, ["b", "d"]);
  const pruned = pruneSelection(state, ["a", "b", "c"]);
  assert.deepEqual(orderedSelection(["a", "b", "c"], pruned.ids), ["b"]);
  assert.equal(pruned.lead, "b");
});

test("重命名后选择状态跟着新路径走", () => {
  const state = renameInSelection(applyMarquee(emptySelection(), order, ["b", "c"]), "b", "b2");
  assert.deepEqual([...state.ids].sort(), ["b2", "c"]);
  assert.equal(state.anchor, "b2");
});

test("自然排序把 img2 放在 img10 之前", () => {
  assert.ok(naturalCompare("img2.mp4", "img10.mp4") < 0);
});

test("拖动单行到开头 / 中间 / 末尾", () => {
  assert.deepEqual(moveBlock(order, new Set(["d"]), 0), ["d", "a", "b", "c", "e"]);
  assert.deepEqual(moveBlock(order, new Set(["d"]), 2), ["a", "b", "d", "c", "e"]);
  assert.deepEqual(moveBlock(order, new Set(["d"]), 5), ["a", "b", "c", "e", "d"]);
});

test("多选拖动保持选中项相对顺序并整体插入", () => {
  const selected = new Set(["b", "d"]);
  assert.deepEqual(moveBlock(order, selected, 5), ["a", "c", "e", "b", "d"]);
  assert.deepEqual(moveBlock(order, selected, 0), ["b", "d", "a", "c", "e"]);
  assert.deepEqual(moveBlock(order, selected, 3), ["a", "c", "b", "d", "e"]);
});

test("落在选中块内部视为顺序不变", () => {
  const selected = new Set(["b", "c"]);
  assert.deepEqual(moveBlock(order, selected, 2), order);
});

test("按名称、大小、时长排序，时长缺失恒排最后", () => {
  const files = [
    { name: "clip10.mp4", ext: "mp4", sizeBytes: 30, durationSec: 12 },
    { name: "clip2.mp4", ext: "mp4", sizeBytes: 10, durationSec: null },
    { name: "clip1.mp4", ext: "mp4", sizeBytes: 20, durationSec: 5 },
  ];
  assert.deepEqual(
    sortEntries(files, "name", "asc").map((file) => file.name),
    ["clip1.mp4", "clip2.mp4", "clip10.mp4"],
  );
  assert.deepEqual(
    sortEntries(files, "size", "asc").map((file) => file.sizeBytes),
    [10, 20, 30],
  );
  assert.deepEqual(
    sortEntries(files, "duration", "desc").map((file) => file.durationSec),
    [12, 5, null],
  );
  assert.deepEqual(
    sortEntries(files, "duration", "asc").map((file) => file.durationSec),
    [5, 12, null],
  );
});

test("按类型排序时同类型再用名称做次级排序", () => {
  const files = [
    { name: "b.wav", ext: "wav", sizeBytes: 1, durationSec: 1 },
    { name: "a.mp4", ext: "mp4", sizeBytes: 1, durationSec: 1 },
    { name: "a.wav", ext: "wav", sizeBytes: 1, durationSec: 1 },
  ];
  assert.deepEqual(
    sortEntries(files, "type", "asc").map((file) => file.name),
    ["a.mp4", "a.wav", "b.wav"],
  );
});

/* ------------------------- 外部拖放路由（多列表面板） ------------------------- */

/** 造一个可观测队列：记录每次 addPaths 的入参。 */
function makeQueue(id) {
  const calls = [];
  registerDropController({
    id,
    addPaths: (paths, at) => {
      calls.push({ paths, at });
    },
  });
  return calls;
}

/** 造一个矩形命中区：落在矩形内返回固定插入下标。 */
function rectLayer(id, controllerId, rect, index) {
  registerDropLayer({
    id,
    controllerId,
    hitTest: (x, y) =>
      x >= rect.left && x <= rect.right && y >= rect.top && y <= rect.bottom ? index : null,
  });
}

test("多列表：一次拖放只进坐标命中的那个列表", async () => {
  resetDropRegistry();
  const a = makeQueue("A");
  const b = makeQueue("B");
  const c = makeQueue("C");
  rectLayer("la", "A", { left: 0, right: 300, top: 0, bottom: 100 }, 2);
  rectLayer("lb", "B", { left: 0, right: 300, top: 200, bottom: 300 }, 1);
  rectLayer("lc", "C", { left: 0, right: 300, top: 400, bottom: 500 }, 0);

  assert.equal(await routeDrop(["/x.mp4"], { x: 100, y: 250 }), true);
  assert.equal(a.length, 0, "未命中的列表不应入队");
  assert.equal(c.length, 0, "未命中的列表不应入队");
  assert.equal(b.length, 1, "命中的列表应恰好入队一次");
  assert.deepEqual(b[0], { paths: ["/x.mp4"], at: 1 }, "按命中坐标算出的下标插入");
});

test("多列表：坐标不落在任何列表时落到默认队列并追加到末尾", async () => {
  resetDropRegistry();
  const a = makeQueue("A");
  const b = makeQueue("B");
  rectLayer("la", "A", { left: 0, right: 300, top: 0, bottom: 100 }, 0);
  rectLayer("lb", "B", { left: 0, right: 300, top: 200, bottom: 300 }, 0);

  await routeDrop(["/y.mp4"], { x: 900, y: 900 });
  assert.equal(b.length, 0);
  assert.equal(a.length, 1);
  assert.equal(a[0].at, undefined, "未命中时追加到末尾");
});

test("多列表：没有坐标信息时也落到默认队列", async () => {
  resetDropRegistry();
  const a = makeQueue("A");
  makeQueue("B");
  rectLayer("la", "A", { left: 0, right: 300, top: 0, bottom: 100 }, 5);

  await routeDrop(["/z.mp4"], null);
  assert.equal(a.length, 1);
  assert.equal(a[0].at, undefined);
});

test("多列表：重叠时后挂载的视图优先命中", async () => {
  resetDropRegistry();
  const a = makeQueue("A");
  const b = makeQueue("B");
  const rect = { left: 0, right: 300, top: 0, bottom: 100 };
  rectLayer("la", "A", rect, 0);
  rectLayer("lb", "B", rect, 4);

  const target = resolveDropTarget({ x: 10, y: 10 });
  assert.equal(target.controller.id, "B");
  assert.equal(target.index, 4);
  await routeDrop(["/w.mp4"], { x: 10, y: 10 });
  assert.equal(a.length, 0);
  assert.equal(b.length, 1);
});

test("多列表：视图卸载后同一位置回落到下层列表", async () => {
  resetDropRegistry();
  const a = makeQueue("A");
  const b = makeQueue("B");
  const rect = { left: 0, right: 300, top: 0, bottom: 100 };
  rectLayer("la", "A", rect, 0);
  const off = registerDropLayer({
    id: "lb",
    controllerId: "B",
    hitTest: (x, y) =>
      x >= rect.left && x <= rect.right && y >= rect.top && y <= rect.bottom ? 3 : null,
  });

  await routeDrop(["/1.mp4"], { x: 10, y: 10 });
  assert.equal(b.length, 1);
  off();
  await routeDrop(["/2.mp4"], { x: 10, y: 10 });
  assert.equal(a.length, 1, "上层卸载后应由下层接住");
  assert.equal(b.length, 1);
});

test("多列表：重复注册同一队列不会改变默认队列", async () => {
  resetDropRegistry();
  const a = makeQueue("A");
  const b = makeQueue("B");
  const a2 = [];
  registerDropController({
    id: "A",
    addPaths: (paths, at) => {
      a2.push({ paths, at });
    },
  });

  await routeDrop(["/r.mp4"], { x: 900, y: 900 });
  assert.equal(a.length, 0, "同 id 只更新，不应产生第二个 A");
  assert.equal(a2.length, 1);
  assert.equal(b.length, 0);
});

test("多列表：没有任何队列时 routeDrop 返回 false", async () => {
  resetDropRegistry();
  assert.equal(await routeDrop(["/n.mp4"], { x: 1, y: 1 }), false);
  assert.equal(resolveDropTarget({ x: 1, y: 1 }), null);
});
