import { Icon } from "./Icon";
import type { ThemeMode } from "@formatforge/design-tokens";

export function TopBar({
  theme,
  search,
  taskCount,
  tasksOpen,
  ffmpegReady,
  onSearch,
  onToggleTheme,
  onToggleTasks,
  onOpenSettings,
}: {
  theme: ThemeMode;
  search: string;
  taskCount: number;
  tasksOpen: boolean;
  ffmpegReady: boolean;
  onSearch: (value: string) => void;
  onToggleTheme: () => void;
  onToggleTasks: () => void;
  onOpenSettings: () => void;
}) {
  return (
    <header className="topbar">
      <div className="brand">
        <div className="brand-mark">T</div>
        <div className="brand-text">
          <b>Transmutio</b>
          <span>格式转换</span>
        </div>
      </div>

      <div className="search">
        <Icon name="search" size={14} />
        <input
          value={search}
          onChange={(event) => onSearch(event.target.value)}
          placeholder="搜索转换方法…"
          aria-label="搜索转换方法"
        />
        {search ? (
          <button className="iconbtn" style={{ width: 22, height: 22 }} onClick={() => onSearch("")} title="清空搜索">
            <Icon name="close" size={12} />
          </button>
        ) : null}
      </div>

      <div className="topbar-right">
        <span
          className="kv"
          style={{ alignItems: "center", gap: 6, marginRight: 4 }}
          title={ffmpegReady ? "已找到 ffmpeg" : "未找到 ffmpeg，请在设置中指定路径"}
        >
          <span className={`dot ${ffmpegReady ? "" : "bad"}`} style={{ width: 6, height: 6, borderRadius: "50%", background: ffmpegReady ? "var(--ff-ok)" : "var(--ff-danger)" }} />
          FFmpeg
        </span>
        <button
          className={`iconbtn ${tasksOpen ? "active" : ""}`}
          onClick={onToggleTasks}
          title="任务中心"
          aria-pressed={tasksOpen}
          style={{ position: "relative" }}
        >
          <Icon name="grid" size={16} />
          {taskCount > 0 ? (
            <span
              style={{
                position: "absolute",
                top: 3,
                right: 2,
                background: "var(--ff-accent)",
                color: "#04110f",
                borderRadius: 999,
                fontSize: 9,
                fontWeight: 700,
                padding: "1px 4px",
                lineHeight: 1.4,
              }}
            >
              {taskCount}
            </span>
          ) : null}
        </button>
        <button className="iconbtn" onClick={onToggleTheme} title={theme === "dark" ? "切换到浅色主题" : "切换到深色主题"}>
          <Icon name={theme === "dark" ? "sun" : "moon"} size={16} />
        </button>
        <button className="iconbtn" onClick={onOpenSettings} title="设置">
          <Icon name="settings" size={16} />
        </button>
      </div>
    </header>
  );
}
