/**
 * 文件列表视图：多选 / 框选 / 整行拖动排序 / 右键菜单 / 键盘 / 就地重命名。
 * 选择与顺序的纯逻辑分别放在 lib/fileSelection、lib/fileQueue，本文件只做交互与渲染。
 */
import {
  useCallback,
  useEffect,
  useId,
  useLayoutEffect,
  useMemo,
  useRef,
  useState,
  useSyncExternalStore,
  type KeyboardEvent as ReactKeyboardEvent,
  type MouseEvent as ReactMouseEvent,
  type PointerEvent as ReactPointerEvent,
} from "react";
import { formatBytes, formatDuration } from "@formatforge/design-tokens";
import type { MediaInfo } from "../lib/api";
import { api } from "../lib/api";
import type { MediaFilesController } from "../panels/shared";
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
  type SelectionState,
} from "../lib/fileSelection";
import { moveBlock, type SortDir, type SortKey } from "../lib/fileQueue";
import {
  clearQueueClipboard,
  getQueueClipboard,
  readPathsFromClipboard,
  setQueueClipboard,
  subscribeQueueClipboard,
  writePathsToClipboard,
} from "../lib/fileClipboard";
import { registerDropLayer } from "../lib/fileDropTarget";
import { ContextMenu, type MenuEntry } from "./ContextMenu";
import { BulkRenameDialog, FilePropertiesDialog } from "./FileDialogs";
import { Icon } from "./Icon";

type Density = "detailed" | "compact";
type Notify = (text: string, kind?: "ok" | "error" | "info") => void;

const DENSITY_KEY = "ff.filetable.density";
const DRAG_THRESHOLD = 4;
const MARQUEE_THRESHOLD = 3;
const AUTO_SCROLL_EDGE = 28;

const SORT_LABEL: Record<SortKey, string> = {
  name: "名称",
  type: "类型",
  size: "大小",
  duration: "时长",
};

interface MenuState {
  x: number;
  y: number;
  atIndex: number;
}

interface DragState {
  paths: string[];
  targetIndex: number;
  lineTop: number;
  x: number;
  y: number;
}

interface DragSession {
  startX: number;
  startY: number;
  x: number;
  y: number;
  paths: string[];
  collapsePath: string | null;
  started: boolean;
  cancelled: boolean;
}

interface MarqueeSession {
  startX: number;
  startY: number;
  base: SelectionState;
  additive: boolean;
  started: boolean;
}

function readDensity(): Density {
  try {
    return localStorage.getItem(DENSITY_KEY) === "compact" ? "compact" : "detailed";
  } catch {
    return "detailed";
  }
}

function typeLabel(file: MediaInfo): string {
  return file.videoCodec || file.audioCodec || file.ext || "—";
}

function rangeLabel(file: MediaInfo): string {
  if (file.kind === "image") return `${file.width ?? "?"}×${file.height ?? "?"}`;
  const size = file.width ? ` · ${file.width}×${file.height}` : "";
  return `${formatDuration(file.durationSec)}${size}`;
}

export function FileTable({ list, onNotify }: { list: MediaFilesController; onNotify?: Notify }) {
  const { files } = list;
  const order = useMemo(() => files.map((file) => file.path), [files]);
  const orderRef = useRef(order);
  orderRef.current = order;

  const [selection, setSelection] = useState<SelectionState>(emptySelection);
  const selectionRef = useRef(selection);
  selectionRef.current = selection;

  const [menu, setMenu] = useState<MenuState | null>(null);
  const [renaming, setRenaming] = useState<{ path: string; value: string } | null>(null);
  const [drag, setDrag] = useState<DragState | null>(null);
  const [marqueeBox, setMarqueeBox] = useState<{ left: number; top: number; width: number; height: number } | null>(null);
  const [density, setDensity] = useState<Density>(readDensity);
  const [propertiesOpen, setPropertiesOpen] = useState(false);
  const [bulkOpen, setBulkOpen] = useState(false);
  const [flashes, setFlashes] = useState<Set<string>>(new Set());
  const [hint, setHint] = useState<string | null>(null);

  const clipboard = useSyncExternalStore(subscribeQueueClipboard, getQueueClipboard, getQueueClipboard);
  const cutPaths = useMemo(
    () => new Set(clipboard?.mode === "cut" ? clipboard.paths : []),
    [clipboard],
  );
  const selectedOrder = orderedSelection(order, selection.ids);
  const lead = selection.lead ?? selection.anchor;
  const focusFile = lead ? files.find((file) => file.path === lead) : undefined;
  const focusIndex = lead ? order.indexOf(lead) + 1 : 0;

  const scrollRef = useRef<HTMLDivElement | null>(null);
  const tableRef = useRef<HTMLTableElement | null>(null);
  const renameRef = useRef<HTMLInputElement | null>(null);
  const dragRef = useRef<DragSession | null>(null);
  const marqueeRef = useRef<MarqueeSession | null>(null);
  const pointerRef = useRef({ x: 0, y: 0 });
  const scrollLoop = useRef(0);
  const suppressDblClick = useRef(0);
  const flashTimer = useRef(0);

  const notify = useCallback<Notify>(
    (text, kind = "info") => {
      if (onNotify) onNotify(text, kind);
      else setHint(text);
    },
    [onNotify],
  );

  const flash = useCallback((paths: string[]) => {
    if (!paths.length) return;
    setFlashes(new Set(paths));
    window.clearTimeout(flashTimer.current);
    flashTimer.current = window.setTimeout(() => setFlashes(new Set()), 900);
  }, []);

  const rows = useCallback(
    () => Array.from(scrollRef.current?.querySelectorAll<HTMLElement>("tr[data-path]") ?? []),
    [],
  );

  /** 客户端 y 坐标 → 插入下标 + 指示线位置（相对滚动内容）。 */
  const dropAt = useCallback(
    (clientY: number): { index: number; lineTop: number } => {
      const scroll = scrollRef.current;
      const all = rows();
      if (!scroll || !all.length) return { index: 0, lineTop: 0 };
      const scrollRect = scroll.getBoundingClientRect();
      for (let index = 0; index < all.length; index += 1) {
        const rect = all[index].getBoundingClientRect();
        if (clientY < rect.top + rect.height / 2) {
          return { index, lineTop: rect.top - scrollRect.top + scroll.scrollTop };
        }
      }
      return {
        index: all.length,
        lineTop: all[all.length - 1].getBoundingClientRect().bottom - scrollRect.top + scroll.scrollTop,
      };
    },
    [rows],
  );

  useEffect(() => {
    if (!files.length) return;
    setSelection((prev) => pruneSelection(prev, order));
    // 只在可见集合变化时裁剪
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [order.join("\n")]);

  useEffect(() => {
    if (!list.duplicateToken) return;
    flash(list.duplicatePaths);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [list.duplicateToken]);

  useEffect(() => () => window.clearTimeout(flashTimer.current), []);

  // 外部拖入 → 落点插入：把本列表的命中测试登记到路由表，由窗口级桥按坐标分发。
  const dropLayerId = useId();
  useEffect(() => {
    return registerDropLayer({
      id: dropLayerId,
      controllerId: list.dropId,
      hitTest: (clientX, clientY) => {
        const scroll = scrollRef.current;
        if (!scroll) return null;
        const rect = scroll.getBoundingClientRect();
        if (clientX < rect.left || clientX > rect.right || clientY < rect.top || clientY > rect.bottom) {
          return null;
        }
        return dropAt(clientY).index;
      },
    });
  }, [dropLayerId, list.dropId, dropAt]);

  useLayoutEffect(() => {
    if (!renaming) return;
    const input = renameRef.current;
    if (!input) return;
    input.focus();
    const dot = renaming.value.lastIndexOf(".");
    const end = dot > 0 ? dot : renaming.value.length;
    input.setSelectionRange(0, end);
  }, [renaming?.path]);

  useEffect(() => {
    try {
      localStorage.setItem(DENSITY_KEY, density);
    } catch {
      /* 隐私模式下忽略 */
    }
  }, [density]);

  const closeMenu = useCallback((restoreFocus?: boolean) => {
    setMenu(null);
    if (!restoreFocus) return;
    // 只有焦点还停在 body（没有落到重命名输入框/弹窗）时才把焦点还给列表。
    requestAnimationFrame(() => {
      const active = document.activeElement;
      if (active && active !== document.body) return;
      scrollRef.current?.focus({ preventScroll: true });
    });
  }, []);

  const startRename = useCallback(
    (path: string) => {
      const file = files.find((item) => item.path === path);
      if (file) setRenaming({ path, value: file.name });
    },
    [files],
  );

  const commitRename = useCallback(async () => {
    const current = renaming;
    if (!current) return;
    const value = current.value.trim();
    const original = files.find((file) => file.path === current.path)?.name;
    setRenaming(null);
    if (!value || value === original) return;
    const result = await list.rename(current.path, value);
    if (result.ok && result.newPath) {
      setSelection((prev) => renameInSelection(prev, current.path, result.newPath!));
      notify("已重命名", "ok");
    } else {
      notify(`重命名失败：${result.error ?? "未知错误"}`, "error");
    }
  }, [renaming, files, list, notify]);

  const remapSelection = useCallback((plans: { from: string; to: string }[]) => {
    const map = new Map(plans.map((plan) => [plan.from, plan.to]));
    setSelection((prev) => {
      const ids = new Set<string>();
      for (const id of prev.ids) ids.add(map.get(id) ?? id);
      return {
        ids,
        anchor: prev.anchor ? (map.get(prev.anchor) ?? prev.anchor) : null,
        lead: prev.lead ? (map.get(prev.lead) ?? prev.lead) : null,
      };
    });
  }, []);

  const applyBulk = useCallback(
    async (args: { paths: string[]; template: string; start: number; pad: number; dryRun: boolean }) => {
      const result = await list.renameMany(args);
      if (result.applied) {
        remapSelection(result.plans);
        notify(`已重命名 ${result.plans.length} 个文件`, "ok");
      }
      return result;
    },
    [list, notify, remapSelection],
  );

  const importFromClipboard = useCallback(
    async (atIndex: number) => {
      const imported = await readPathsFromClipboard();
      if (!imported.length) {
        notify("剪贴板里没有可用的文件路径", "info");
        return;
      }
      await list.addPaths(imported, atIndex);
    },
    [list, notify],
  );

  const paste = useCallback(
    async (atIndex: number) => {
      const clip = getQueueClipboard();
      if (!clip) {
        await importFromClipboard(atIndex);
        return;
      }
      const existing = new Set(list.files.map((file) => file.path));
      const present = clip.paths.filter((path) => existing.has(path));
      const missing = clip.paths.filter((path) => !existing.has(path));
      if (clip.mode === "cut") {
        if (present.length) list.move(present, atIndex);
        if (missing.length) await list.addPaths(missing, atIndex);
        clearQueueClipboard();
        notify("已移动到目标位置", "ok");
        return;
      }
      if (!missing.length) {
        notify(`${present.length} 项已在列表中`, "info");
        flash(present);
        return;
      }
      await list.addPaths(missing, atIndex);
    },
    [list, notify, flash, importFromClipboard],
  );

  const openFiles = useCallback(
    async (paths: string[]) => {
      for (const path of paths.slice(0, 12)) {
        try {
          await api.openSourceFile(path);
        } catch (error) {
          notify(`打开失败：${error instanceof Error ? error.message : String(error)}`, "error");
          return;
        }
      }
    },
    [notify],
  );

  const revealFiles = useCallback(
    async (paths: string[]) => {
      for (const path of paths.slice(0, 6)) {
        try {
          await api.revealSourceFile(path);
        } catch (error) {
          notify(`定位失败：${error instanceof Error ? error.message : String(error)}`, "error");
          return;
        }
      }
    },
    [notify],
  );

  const copyPaths = useCallback(
    async (mode: "copy" | "cut" | "path") => {
      const paths = orderedSelection(orderRef.current, selectionRef.current.ids);
      if (!paths.length) return;
      setQueueClipboard(mode === "cut" ? "cut" : "copy", paths);
      const wrote = await writePathsToClipboard(paths);
      if (mode === "cut") notify(wrote ? "已剪切（待移动到新位置）" : "已剪切", "ok");
      else if (mode === "copy") notify(wrote ? "已复制路径到剪贴板" : "已复制", "ok");
      else notify(wrote ? `已复制 ${paths.length} 个路径` : "复制路径失败", wrote ? "ok" : "error");
    },
    [notify],
  );

  const sortBy = useCallback(
    (key: SortKey, dir: SortDir) => {
      list.sortBy(key, dir);
      notify("合并顺序已更新", "ok");
    },
    [list, notify],
  );

  const removePaths = useCallback(
    (paths: string[]) => {
      if (!paths.length) return;
      list.removeMany(paths);
      notify(`已从列表移除 ${paths.length} 项`, "ok");
    },
    [list, notify],
  );

  const openRowMenuAtLead = useCallback(() => {
    const current = selectionRef.current;
    const path = current.lead ?? current.anchor;
    if (!path) return;
    const row = scrollRef.current?.querySelector<HTMLElement>(`tr[data-path="${CSS.escape(path)}"]`);
    const scroll = scrollRef.current;
    if (!row || !scroll) return;
    const rect = row.getBoundingClientRect();
    const scrollRect = scroll.getBoundingClientRect();
    setMenu({
      x: rect.left + 72,
      y: Math.min(rect.bottom - 4, scrollRect.bottom - 8),
      atIndex: orderRef.current.indexOf(path),
    });
  }, []);

  /* ------------------------------- 行拖动 ------------------------------- */

  const endScrollLoop = useCallback(() => {
    if (scrollLoop.current) cancelAnimationFrame(scrollLoop.current);
    scrollLoop.current = 0;
  }, []);

  const startDrag = useCallback(
    (event: ReactPointerEvent, paths: string[], collapsePath: string | null) => {
      const session: DragSession = {
        startX: event.clientX,
        startY: event.clientY,
        x: event.clientX,
        y: event.clientY,
        paths,
        collapsePath,
        started: false,
        cancelled: false,
      };
      dragRef.current = session;
      pointerRef.current = { x: event.clientX, y: event.clientY };

      const update = () => {
        if (!session.started || session.cancelled) return;
        const { index, lineTop } = dropAt(session.y);
        setDrag({ paths: session.paths, targetIndex: index, lineTop, x: session.x, y: session.y });
      };

      const scrollTick = () => {
        const scroll = scrollRef.current;
        if (!scroll || !dragRef.current?.started || dragRef.current.cancelled) {
          scrollLoop.current = 0;
          return;
        }
        const rect = scroll.getBoundingClientRect();
        if (session.y < rect.top + AUTO_SCROLL_EDGE) {
          scroll.scrollTop -= Math.ceil((rect.top + AUTO_SCROLL_EDGE - session.y) / 3);
        } else if (session.y > rect.bottom - AUTO_SCROLL_EDGE) {
          scroll.scrollTop += Math.ceil((session.y - (rect.bottom - AUTO_SCROLL_EDGE)) / 3);
        }
        update();
        scrollLoop.current = requestAnimationFrame(scrollTick);
      };

      const onMove = (moveEvent: PointerEvent) => {
        session.x = moveEvent.clientX;
        session.y = moveEvent.clientY;
        pointerRef.current = { x: session.x, y: session.y };
        if (!session.started) {
          const distance = Math.hypot(session.x - session.startX, session.y - session.startY);
          if (distance < DRAG_THRESHOLD) return;
          session.started = true;
          document.body.classList.add("fl-dragging");
          scrollLoop.current = requestAnimationFrame(scrollTick);
        }
        update();
      };

      const finish = (cancelled: boolean) => {
        window.removeEventListener("pointermove", onMove);
        window.removeEventListener("pointerup", onUp);
        window.removeEventListener("pointercancel", onCancel);
        window.removeEventListener("keydown", onKey);
        endScrollLoop();
        document.body.classList.remove("fl-dragging");
        const wasStarted = session.started;
        dragRef.current = null;
        setDrag(null);
        if (cancelled) return;
        if (wasStarted) {
          suppressDblClick.current = Date.now();
          const { index } = dropAt(session.y);
          const nextOrder = moveBlock(orderRef.current, new Set(session.paths), index);
          if (nextOrder.join("\n") !== orderRef.current.join("\n")) {
            list.move(session.paths, index);
            flash(session.paths);
          }
        } else if (session.collapsePath) {
          setSelection(selectOnly(session.collapsePath));
        }
      };

      const onUp = () => finish(false);
      const onCancel = () => finish(true);
      const onKey = (keyEvent: KeyboardEvent) => {
        if (keyEvent.key === "Escape") {
          keyEvent.preventDefault();
          session.cancelled = true;
          finish(true);
        }
      };
      window.addEventListener("pointermove", onMove);
      window.addEventListener("pointerup", onUp);
      window.addEventListener("pointercancel", onCancel);
      window.addEventListener("keydown", onKey);
    },
    [dropAt, endScrollLoop, flash, list],
  );

  const onRowPointerDown = useCallback(
    (event: ReactPointerEvent, path: string) => {
      if (event.button !== 0 || renaming) return;
      if (menu) setMenu(null);
      scrollRef.current?.focus({ preventScroll: true });
      const ctrl = event.ctrlKey || event.metaKey;
      const shift = event.shiftKey;
      const current = selectionRef.current;
      let next: SelectionState;
      let collapsePath: string | null = null;
      if (ctrl && shift) next = rangeTo(current, orderRef.current, path, true);
      else if (ctrl) next = toggle(current, path);
      else if (shift) next = rangeTo(current, orderRef.current, path, false);
      else if (!current.ids.has(path)) next = selectOnly(path);
      else {
        next = { ids: current.ids, anchor: path, lead: path };
        if (current.ids.size > 1) collapsePath = path;
      }
      setSelection(next);
      if (!ctrl && !shift) {
        startDrag(event, orderedSelection(orderRef.current, next.ids), collapsePath);
      }
    },
    [menu, renaming, startDrag],
  );

  /* ------------------------------- 框选 ------------------------------- */

  const onBackgroundPointerDown = useCallback(
    (event: ReactPointerEvent) => {
      if (event.button !== 0) return;
      if ((event.target as HTMLElement).closest("tr[data-path], input, button, .fl-head, .fl-status")) return;
      const scroll = scrollRef.current;
      if (!scroll) return;
      if (menu) setMenu(null);
      scroll.focus({ preventScroll: true });
      const additive = event.ctrlKey || event.metaKey || event.shiftKey;
      const session: MarqueeSession = {
        startX: event.clientX,
        startY: event.clientY,
        base: additive ? selectionRef.current : emptySelection(),
        additive,
        started: false,
      };
      marqueeRef.current = session;

      const update = () => {
        const rect = scroll.getBoundingClientRect();
        const box = {
          left: Math.min(session.startX, pointerRef.current.x) - rect.left + scroll.scrollLeft,
          top: Math.min(session.startY, pointerRef.current.y) - rect.top + scroll.scrollTop,
          width: Math.abs(pointerRef.current.x - session.startX),
          height: Math.abs(pointerRef.current.y - session.startY),
        };
        setMarqueeBox(box);
        const hits: string[] = [];
        for (const row of rows()) {
          const rowRect = row.getBoundingClientRect();
          const point = row.dataset.path;
          if (!point) continue;
          if (
            rowRect.left < Math.max(session.startX, pointerRef.current.x) &&
            rowRect.right > Math.min(session.startX, pointerRef.current.x) &&
            rowRect.top < Math.max(session.startY, pointerRef.current.y) &&
            rowRect.bottom > Math.min(session.startY, pointerRef.current.y)
          ) {
            hits.push(point);
          }
        }
        setSelection(applyMarquee(session.base, orderRef.current, hits, session.additive));
      };

      const onMove = (moveEvent: PointerEvent) => {
        pointerRef.current = { x: moveEvent.clientX, y: moveEvent.clientY };
        if (!session.started) {
          const distance = Math.hypot(moveEvent.clientX - session.startX, moveEvent.clientY - session.startY);
          if (distance < MARQUEE_THRESHOLD) return;
          session.started = true;
          document.body.classList.add("fl-marquee");
        }
        update();
      };

      const finish = () => {
        window.removeEventListener("pointermove", onMove);
        window.removeEventListener("pointerup", finish);
        window.removeEventListener("pointercancel", finish);
        document.body.classList.remove("fl-marquee");
        marqueeRef.current = null;
        setMarqueeBox(null);
        if (!session.started && !session.additive) setSelection(emptySelection());
      };
      window.addEventListener("pointermove", onMove);
      window.addEventListener("pointerup", finish);
      window.addEventListener("pointercancel", finish);
    },
    [menu, rows],
  );

  /* ------------------------------- 右键菜单 ------------------------------- */

  const openMenuOnRow = useCallback((event: ReactMouseEvent, path: string) => {
    event.preventDefault();
    event.stopPropagation();
    scrollRef.current?.focus({ preventScroll: true });
    if (!selectionRef.current.ids.has(path)) setSelection(selectOnly(path));
    setMenu({ x: event.clientX, y: event.clientY, atIndex: orderRef.current.indexOf(path) });
  }, []);

  const openMenuOnBlank = useCallback((event: ReactMouseEvent) => {
    event.preventDefault();
    scrollRef.current?.focus({ preventScroll: true });
    setSelection(emptySelection());
    setMenu({ x: event.clientX, y: event.clientY, atIndex: dropAt(event.clientY).index });
  }, [dropAt]);

  const menuEntries = useMemo<MenuEntry[]>(() => {
    if (!menu) return [];
    const hasSelection = selection.ids.size > 0;
    const multi = selection.ids.size > 1;
    const sortEntriesList: MenuEntry[] = (["name", "type", "size", "duration"] as SortKey[]).map((key) => ({
      type: "item",
      id: `sort-${key}`,
      label: SORT_LABEL[key],
      checked: list.sort?.key === key,
      onSelect: () => sortBy(key, list.sort?.key === key && list.sort.dir === "asc" ? "desc" : "asc"),
    }));
    const sortMenu: MenuEntry = {
      type: "submenu",
      id: "sort",
      label: "排序方式",
      hint: "会改变合并顺序",
      items: [
        ...sortEntriesList,
        { type: "separator", id: "sort-sep" },
        {
          type: "item",
          id: "sort-asc",
          label: "升序",
          checked: list.sort?.dir === "asc",
          onSelect: () => sortBy(list.sort?.key ?? "name", "asc"),
        },
        {
          type: "item",
          id: "sort-desc",
          label: "降序",
          checked: list.sort?.dir === "desc",
          onSelect: () => sortBy(list.sort?.key ?? "name", "desc"),
        },
        { type: "separator", id: "sort-sep2" },
        {
          type: "item",
          id: "sort-restore",
          label: "恢复添加顺序",
          onSelect: () => {
            list.restoreAddedOrder();
            notify("已恢复添加顺序", "ok");
          },
        },
      ],
    };
    const viewMenu: MenuEntry = {
      type: "submenu",
      id: "view",
      label: "显示方式",
      items: [
        {
          type: "item",
          id: "view-detailed",
          label: "详细列表",
          checked: density === "detailed",
          onSelect: () => setDensity("detailed"),
        },
        {
          type: "item",
          id: "view-compact",
          label: "紧凑列表",
          checked: density === "compact",
          onSelect: () => setDensity("compact"),
        },
      ],
    };

    if (!hasSelection) {
      return [
        {
          type: "item",
          id: "paste",
          label: "粘贴",
          shortcut: "Ctrl+V",
          icon: "clipboard",
          onSelect: () => void paste(menu.atIndex),
        },
        {
          type: "item",
          id: "import-paths",
          label: "从剪贴板导入路径",
          icon: "clipboard",
          onSelect: () => void importFromClipboard(menu.atIndex),
        },
        { type: "separator", id: "s1" },
        {
          type: "item",
          id: "select-all",
          label: "全选",
          shortcut: "Ctrl+A",
          disabled: !files.length,
          onSelect: () => setSelection(selectAll(orderRef.current)),
        },
        {
          type: "item",
          id: "deselect",
          label: "取消选择",
          shortcut: "Esc",
          disabled: true,
          onSelect: () => setSelection(clearSelection()),
        },
        { type: "separator", id: "s2" },
        sortMenu,
        viewMenu,
        { type: "separator", id: "s3" },
        {
          type: "item",
          id: "refresh",
          label: "刷新",
          shortcut: "F5",
          icon: "refresh",
          disabled: !files.length || list.busy,
          onSelect: () => {
            void list.refresh();
            notify("已刷新文件信息", "ok");
          },
        },
      ];
    }

    const entries: MenuEntry[] = [
      {
        type: "item",
        id: "open",
        label: "打开",
        shortcut: "Enter",
        icon: "play",
        onSelect: () => void openFiles(selectedOrder),
      },
      {
        type: "item",
        id: "reveal",
        label: "在文件管理器中显示",
        icon: "folder",
        onSelect: () => void revealFiles(selectedOrder),
      },
      { type: "separator", id: "r1" },
    ];
    if (multi) {
      entries.push({
        type: "item",
        id: "bulk-rename",
        label: "批量重命名…",
        icon: "pencil",
        onSelect: () => setBulkOpen(true),
      });
    } else {
      entries.push({
        type: "item",
        id: "rename",
        label: "重命名",
        shortcut: "F2",
        icon: "pencil",
        onSelect: () => startRename(selectedOrder[0]),
      });
    }
    entries.push(
      { type: "separator", id: "r2" },
      {
        type: "item",
        id: "copy",
        label: "复制",
        shortcut: "Ctrl+C",
        icon: "copy",
        onSelect: () => void copyPaths("copy"),
      },
      {
        type: "item",
        id: "cut",
        label: "剪切",
        shortcut: "Ctrl+X",
        icon: "scissors",
        onSelect: () => void copyPaths("cut"),
      },
      {
        type: "item",
        id: "copy-path",
        label: "复制路径",
        icon: "link",
        onSelect: () => void copyPaths("path"),
      },
      { type: "separator", id: "r3" },
      {
        type: "item",
        id: "remove",
        label: multi ? `移除选中的 ${selection.ids.size} 项` : "从列表移除",
        shortcut: "Delete",
        icon: "trash",
        danger: true,
        onSelect: () => removePaths(selectedOrder),
      },
      { type: "separator", id: "r4" },
      {
        type: "item",
        id: "props",
        label: "属性",
        shortcut: "Alt+Enter",
        icon: "info",
        onSelect: () => setPropertiesOpen(true),
      },
    );
    return entries;
  }, [
    menu,
    selection.ids.size,
    list,
    density,
    files.length,
    selectedOrder,
    paste,
    importFromClipboard,
    openFiles,
    revealFiles,
    startRename,
    copyPaths,
    removePaths,
    sortBy,
    notify,
  ]);

  /* ------------------------------- 键盘 ------------------------------- */

  const onKeyDown = useCallback(
    (event: ReactKeyboardEvent) => {
      const target = event.target as HTMLElement;
      if (target.closest("input, textarea, select")) return;
      if (menu) return;
      const mod = event.ctrlKey || event.metaKey;
      const key = event.key;
      const current = selectionRef.current;
      const visible = orderRef.current;
      const leadPath = current.lead ?? current.anchor;
      const paths = orderedSelection(visible, current.ids);

      if (mod && key.toLowerCase() === "a") {
        event.preventDefault();
        setSelection(selectAll(visible));
        return;
      }
      if (mod && key.toLowerCase() === "c") {
        event.preventDefault();
        void copyPaths("copy");
        return;
      }
      if (mod && key.toLowerCase() === "x") {
        event.preventDefault();
        void copyPaths("cut");
        return;
      }
      if (mod && key.toLowerCase() === "v") {
        event.preventDefault();
        void paste(leadPath ? visible.indexOf(leadPath) : visible.length);
        return;
      }
      if (key === "Escape") {
        event.preventDefault();
        setSelection(clearSelection());
        return;
      }
      if (key === "ArrowDown" || key === "ArrowUp") {
        event.preventDefault();
        setSelection(moveFocus(current, visible, key === "ArrowDown" ? 1 : -1, event.shiftKey));
        return;
      }
      if (key === "Home" || key === "End") {
        event.preventDefault();
        setSelection(focusEdge(current, visible, key === "Home" ? "start" : "end", event.shiftKey));
        return;
      }
      if (key === "Enter" && !event.altKey) {
        event.preventDefault();
        void openFiles(paths);
        return;
      }
      if (key === "Enter" && event.altKey) {
        event.preventDefault();
        if (paths.length) setPropertiesOpen(true);
        return;
      }
      if (key === "F2") {
        event.preventDefault();
        if (paths.length > 1) setBulkOpen(true);
        else if (paths.length === 1) startRename(paths[0]);
        return;
      }
      if (key === "Delete") {
        event.preventDefault();
        removePaths(paths);
        return;
      }
      if (key === "F5") {
        event.preventDefault();
        void list.refresh();
        return;
      }
      if (key === "ContextMenu" || (key === "F10" && event.shiftKey)) {
        event.preventDefault();
        openRowMenuAtLead();
      }
    },
    [menu, copyPaths, paste, openFiles, startRename, removePaths, list, openRowMenuAtLead],
  );

  /* ------------------------------- 渲染 ------------------------------- */

  const head = (
    <div className="fl-head">
      <span className="fl-sort-state">
        <Icon name="sort" size={13} />
        {list.sort ? `按${SORT_LABEL[list.sort.key]} · ${list.sort.dir === "asc" ? "升序" : "降序"}` : "自定义顺序"}
      </span>
      <span className="fl-head-hint">拖动整行可调整顺序</span>
      <button
        className="btn small ghost fl-restore"
        onClick={() => {
          list.restoreAddedOrder();
          notify("已恢复添加顺序", "ok");
        }}
        disabled={!files.length}
      >
        恢复添加顺序
      </button>
    </div>
  );

  const status = (
    <div className="fl-status">
      <div className="fl-focus" title={focusFile?.path ?? ""}>
        {focusFile ? (
          <>
            <b className="fl-focus-name">{focusFile.name}</b>
            <span className="fl-focus-path">{focusFile.path}</span>
            <span className="fl-focus-meta">
              {focusFile.kind === "image"
                ? `${focusFile.width ?? "?"}×${focusFile.height ?? "?"} · ${focusFile.ext}`
                : `${formatDuration(focusFile.durationSec)} · ${focusFile.ext}`}
              {` · ${formatBytes(focusFile.sizeBytes)}`}
            </span>
            <span className="fl-focus-index">
              #{focusIndex}/{files.length}
            </span>
          </>
        ) : (
          <span className="muted">未选中文件</span>
        )}
      </div>
      <div className="fl-counts">
        {hint ? <span className="fl-hint">{hint}</span> : null}
        {list.notice ? <span className="fl-hint">{list.notice}</span> : null}
        <span>
          共 {files.length} 个
          {selection.ids.size ? ` · 已选 ${selection.ids.size}` : ""}
        </span>
      </div>
    </div>
  );

  return (
    <div className={`filetable density-${density}`}>
      {head}
      <div
        className="fl-scroll"
        ref={scrollRef}
        tabIndex={0}
        role="presentation"
        onKeyDown={onKeyDown}
        onPointerDown={onBackgroundPointerDown}
        onContextMenu={openMenuOnBlank}
      >
        {files.length ? (
          <table ref={tableRef} role="grid" aria-multiselectable="true">
            <thead>
              <tr>
                <th className="fl-seq">#</th>
                <th>文件</th>
                <th>类型</th>
                <th>时长 / 尺寸</th>
                <th>大小</th>
                <th />
                <th className="fl-actions" />
              </tr>
            </thead>
            <tbody>
              {files.map((file, index) => {
                const selected = selection.ids.has(file.path);
                const isLead = lead === file.path;
                return (
                  <tr
                    key={file.path}
                    data-path={file.path}
                    role="row"
                    aria-selected={selected}
                    className={[
                      selected ? "sel" : "",
                      isLead ? "lead" : "",
                      cutPaths.has(file.path) ? "cut" : "",
                      flashes.has(file.path) ? "flash" : "",
                    ]
                      .filter(Boolean)
                      .join(" ")}
                    onPointerDown={(event) => onRowPointerDown(event, file.path)}
                    onDoubleClick={() => {
                      if (Date.now() - suppressDblClick.current < 350) return;
                      void openFiles([file.path]);
                    }}
                    onContextMenu={(event) => openMenuOnRow(event, file.path)}
                  >
                    <td className="fl-seq">{index + 1}</td>
                    <td className="name" title={file.path}>
                      {renaming?.path === file.path ? (
                        <input
                          ref={renameRef}
                          className="fl-rename"
                          value={renaming.value}
                          onChange={(event) => setRenaming({ path: file.path, value: event.target.value })}
                          onPointerDown={(event) => event.stopPropagation()}
                          onClick={(event) => event.stopPropagation()}
                          onDoubleClick={(event) => event.stopPropagation()}
                          onKeyDown={(event) => {
                            event.stopPropagation();
                            if (event.key === "Enter") {
                              event.preventDefault();
                              void commitRename();
                            } else if (event.key === "Escape") {
                              event.preventDefault();
                              setRenaming(null);
                            }
                          }}
                          onBlur={() => setRenaming(null)}
                        />
                      ) : (
                        <span className="fl-name">{file.name}</span>
                      )}
                    </td>
                    <td className="muted">{typeLabel(file)}</td>
                    <td className="muted">{rangeLabel(file)}</td>
                    <td className="muted">{formatBytes(file.sizeBytes)}</td>
                    <td className={file.error ? "err" : "muted"} title={file.error ?? ""}>
                      {file.error ? file.error.slice(0, 42) : ""}
                    </td>
                    <td>
                      <button
                        className="fl-row-remove"
                        title="从列表移除"
                        onPointerDown={(event) => event.stopPropagation()}
                        onClick={(event) => {
                          event.stopPropagation();
                          removePaths([file.path]);
                        }}
                      >
                        <Icon name="close" size={12} />
                      </button>
                    </td>
                  </tr>
                );
              })}
            </tbody>
          </table>
        ) : (
          <div className="empty">
            <div className="big">
              <Icon name="film" size={22} />
            </div>
            <div>拖入文件，或点击「添加文件」。</div>
            <div style={{ fontSize: "var(--ds-type-role-body)", color: "var(--ds-color-text-muted)" }}>文件不会被复制；仅「重命名」会改动磁盘上的文件名</div>
          </div>
        )}
        {marqueeBox ? (
          <div
            className="fl-marquee"
            style={{
              left: marqueeBox.left,
              top: marqueeBox.top,
              width: marqueeBox.width,
              height: marqueeBox.height,
            }}
          />
        ) : null}
        {drag ? <div className="fl-drop-line" style={{ top: drag.lineTop }} /> : null}
      </div>
      {status}
      {drag ? (
        <div className="fl-ghost" style={{ left: drag.x + 14, top: drag.y + 12 }}>
          <div className="fl-ghost-title">
            {drag.paths.length > 1 ? `移动 ${drag.paths.length} 个文件` : "移动 1 个文件"}
          </div>
          {drag.paths.slice(0, 3).map((path) => (
            <div className="fl-ghost-row" key={path}>
              {path.split(/[\\/]/).pop()}
            </div>
          ))}
          {drag.paths.length > 3 ? <div className="fl-ghost-row">…</div> : null}
        </div>
      ) : null}
      {menu ? (
        <ContextMenu x={menu.x} y={menu.y} entries={menuEntries} onClose={closeMenu} label="文件列表菜单" />
      ) : null}
      {propertiesOpen ? (
        <FilePropertiesDialog
          files={selection.ids.size ? files.filter((file) => selection.ids.has(file.path)) : files}
          onClose={() => setPropertiesOpen(false)}
        />
      ) : null}
      {bulkOpen ? (
        <BulkRenameDialog
          paths={selectedOrder}
          onPreview={list.renameMany}
          onApply={applyBulk}
          onClose={() => setBulkOpen(false)}
        />
      ) : null}
    </div>
  );
}
