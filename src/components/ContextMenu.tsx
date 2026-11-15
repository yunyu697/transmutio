/**
 * 右键菜单：键盘（↑↓ / Home / End / Enter / Esc / 首字母跳转 / ←→ 进出子菜单）、
 * 点击外部关闭、贴边翻转。渲染到 body，避免被列表滚动容器裁掉。
 */
import { useEffect, useLayoutEffect, useRef, useState } from "react";
import { createPortal } from "react-dom";
import { Icon } from "./Icon";

export interface MenuItem {
  type: "item";
  id: string;
  label: string;
  /** 注解（例如「会改变合并顺序」），显示在标签右侧。 */
  hint?: string;
  shortcut?: string;
  icon?: string;
  disabled?: boolean;
  danger?: boolean;
  checked?: boolean;
  onSelect: () => void;
}

export interface MenuSubmenu {
  type: "submenu";
  id: string;
  label: string;
  /** 注解（例如「会改变合并顺序」）。 */
  hint?: string;
  icon?: string;
  items: MenuEntry[];
}

export interface MenuSeparator {
  type: "separator";
  id: string;
}

export type MenuEntry = MenuItem | MenuSubmenu | MenuSeparator;

export interface ContextMenuProps {
  x: number;
  y: number;
  entries: MenuEntry[];
  label?: string;
  /** restoreFocus = true 表示菜单是通过键盘关闭的，调用方可以把焦点还给列表。 */
  onClose: (restoreFocus?: boolean) => void;
}

const EDGE = 8;

function MenuPanel({
  entries,
  x,
  y,
  depth,
  label,
  onClose,
  onCloseAll,
}: {
  entries: MenuEntry[];
  x: number;
  y: number;
  depth: number;
  label?: string;
  onClose: (restoreFocus?: boolean) => void;
  onCloseAll: (restoreFocus?: boolean) => void;
}) {
  const panel = useRef<HTMLDivElement | null>(null);
  const [pos, setPos] = useState({ left: x, top: y });
  const [active, setActive] = useState(-1);
  const [openSub, setOpenSub] = useState<string | null>(null);
  const typeahead = useRef({ text: "", at: 0 });

  const actionable = entries
    .map((entry, index) => ({ entry, index }))
    .filter(({ entry }) => entry.type !== "separator");

  useLayoutEffect(() => {
    const node = panel.current;
    if (!node) return;
    const rect = node.getBoundingClientRect();
    let left = x;
    let top = y;
    if (left + rect.width > window.innerWidth - EDGE) left = Math.max(EDGE, x - rect.width);
    if (top + rect.height > window.innerHeight - EDGE) top = Math.max(EDGE, y - rect.height);
    setPos({ left, top });
    node.focus();
  }, [x, y]);

  useEffect(() => {
    setActive(actionable.length ? 0 : -1);
    // 只在条目集合变化时重置高亮
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [entries.length]);

  const move = (delta: number) => {
    if (!actionable.length) return;
    const current = actionable.findIndex(({ index }) => index === active);
    const next = (current + delta + actionable.length) % actionable.length;
    setActive(actionable[next].index);
    setOpenSub(null);
  };

  const jump = (char: string) => {
    const now = Date.now();
    const buffer = now - typeahead.current.at > 600 ? char : typeahead.current.text + char;
    typeahead.current = { text: buffer, at: now };
    const pool = actionable.filter(
      ({ entry }) => entry.type !== "item" || !entry.disabled,
    );
    if (!pool.length) return;
    const startIndex = pool.findIndex(({ index }) => index === active);
    for (let step = 1; step <= pool.length; step += 1) {
      const candidate = pool[(startIndex + step + pool.length) % pool.length];
      const text = candidate.entry.type === "separator" ? "" : candidate.entry.label;
      if (text.toLowerCase().startsWith(buffer.toLowerCase())) {
        setActive(candidate.index);
        setOpenSub(null);
        return;
      }
    }
  };

  const activate = (entry: MenuEntry) => {
    if (entry.type === "separator") return;
    if (entry.type === "submenu") {
      setOpenSub(entry.id);
      return;
    }
    if (entry.disabled) return;
    entry.onSelect();
    onCloseAll(true);
  };

  const onKeyDown = (event: React.KeyboardEvent) => {
    const entry = entries[active];
    switch (event.key) {
      case "ArrowDown":
        event.preventDefault();
        move(1);
        break;
      case "ArrowUp":
        event.preventDefault();
        move(-1);
        break;
      case "Home":
        event.preventDefault();
        if (actionable.length) setActive(actionable[0].index);
        break;
      case "End":
        event.preventDefault();
        if (actionable.length) setActive(actionable[actionable.length - 1].index);
        break;
      case "ArrowRight":
        if (entry?.type === "submenu") {
          event.preventDefault();
          setOpenSub(entry.id);
        }
        break;
      case "ArrowLeft":
        if (depth > 0) {
          event.preventDefault();
          event.stopPropagation();
          onClose(true);
        }
        break;
      case "Enter":
      case " ":
        event.preventDefault();
        if (entry) activate(entry);
        break;
      case "Escape":
        event.preventDefault();
        event.stopPropagation();
        onClose(true);
        break;
      case "Tab":
        event.preventDefault();
        onCloseAll(true);
        break;
      default:
        if (event.key.length === 1 && !event.ctrlKey && !event.metaKey && !event.altKey) {
          jump(event.key);
        }
    }
  };

  const submenuEntry = entries.find(
    (entry): entry is MenuSubmenu => entry.type === "submenu" && entry.id === openSub,
  );
  const submenuAnchor = submenuEntry
    ? panel.current?.querySelector<HTMLElement>(`[data-menu-id="${CSS.escape(submenuEntry.id)}"]`)
    : null;
  const anchorRect = submenuAnchor?.getBoundingClientRect();

  return (
    <div
      className="ctx-panel"
      data-ff-menu=""
      data-depth={depth}
      role="menu"
      aria-label={label ?? "文件操作"}
      style={{ left: pos.left, top: pos.top }}
      tabIndex={-1}
      ref={panel}
      onKeyDown={onKeyDown}
    >
      {entries.map((entry) => {
        if (entry.type === "separator") {
          return <div className="ctx-sep" role="separator" key={entry.id} />;
        }
        const isActive = active === entries.indexOf(entry);
        if (entry.type === "submenu") {
          return (
            <div
              key={entry.id}
              data-menu-id={entry.id}
              role="menuitem"
              aria-haspopup="menu"
              aria-expanded={openSub === entry.id}
              className={`ctx-item${isActive ? " active" : ""}`}
              onMouseEnter={() => {
                setActive(entries.indexOf(entry));
                setOpenSub(entry.id);
              }}
              onClick={() => setOpenSub(entry.id)}
            >
              {entry.icon ? <Icon name={entry.icon} size={14} /> : <span className="ctx-icon" />}
              <span className="ctx-label">{entry.label}</span>
              {entry.hint ? <span className="ctx-hint">{entry.hint}</span> : null}
              <Icon name="chevron-right" size={12} className="ctx-chev" />
            </div>
          );
        }
        return (
          <div
            key={entry.id}
            role="menuitem"
            aria-disabled={entry.disabled || undefined}
            className={`ctx-item${isActive ? " active" : ""}${entry.disabled ? " disabled" : ""}${
              entry.danger ? " danger" : ""
            }`}
            onMouseEnter={() => {
              setActive(entries.indexOf(entry));
              setOpenSub(null);
            }}
            onClick={() => activate(entry)}
          >
            {entry.icon ? <Icon name={entry.icon} size={14} /> : <span className="ctx-icon" />}
            <span className="ctx-label">{entry.label}</span>
            {entry.hint ? <span className="ctx-hint">{entry.hint}</span> : null}
            {entry.checked ? <Icon name="check" size={12} className="ctx-check" /> : null}
            {entry.shortcut ? <span className="ctx-shortcut">{entry.shortcut}</span> : null}
          </div>
        );
      })}
      {submenuEntry && anchorRect ? (
        <MenuPanel
          entries={submenuEntry.items}
          x={anchorRect.right - 2}
          y={anchorRect.top}
          depth={depth + 1}
          label={submenuEntry.label}
          onClose={() => {
            setOpenSub(null);
            panel.current?.focus();
          }}
          onCloseAll={onCloseAll}
        />
      ) : null}
    </div>
  );
}

export function ContextMenu({ x, y, entries, label, onClose }: ContextMenuProps) {
  useEffect(() => {
    const onDown = (event: MouseEvent) => {
      const target = event.target as HTMLElement | null;
      if (target?.closest("[data-ff-menu]")) return;
      onClose();
    };
    const onScroll = () => onClose();
    const onBlur = () => onClose();
    document.addEventListener("mousedown", onDown, true);
    document.addEventListener("scroll", onScroll, true);
    window.addEventListener("resize", onScroll);
    window.addEventListener("blur", onBlur);
    return () => {
      document.removeEventListener("mousedown", onDown, true);
      document.removeEventListener("scroll", onScroll, true);
      window.removeEventListener("resize", onScroll);
      window.removeEventListener("blur", onBlur);
    };
  }, [onClose]);

  return createPortal(
    <MenuPanel entries={entries} x={x} y={y} depth={0} label={label} onClose={onClose} onCloseAll={onClose} />,
    document.body,
  );
}
