import { findMethod, sections, type MethodDef, type SectionDef } from "../catalog";
import { Icon } from "./Icon";

/** 真实应用图标（打包用 Tauri 图标集，不新增文件）。 */
const appIconUrl = new URL("../../src-tauri/icons/128x128.png", import.meta.url).href;

const RECENT_LIMIT = 4;

export interface SideNavProps {
  activeSection: string;
  activeMethodId: string | null;
  collapsed: boolean;
  search: string;
  recentMethods: string[];
  outputDir: string;
  outputDirMode: "source" | "custom";
  taskCount: number;
  tasksOpen: boolean;
  theme: "dark" | "light";
  onSelectSection: (section: SectionDef) => void;
  onPickMethod: (method: MethodDef) => void;
  onHome: () => void;
  onSearch: (value: string) => void;
  onToggleCollapsed: () => void;
  onToggleTasks: () => void;
  onToggleTheme: () => void;
  onOpenSettings: () => void;
}

/**
 * 左栏：折叠态与展开态共用同一个 <nav> 与同一个折叠按钮。
 * 折叠按钮固定在导航容器左上角（top/left 各 8px），两种状态下坐标完全一致，
 * 只换图标方向与 aria-label，避免按钮在屏幕上跳位置。
 */
export function SideNav({
  activeSection,
  activeMethodId,
  collapsed,
  search,
  recentMethods,
  outputDir,
  outputDirMode,
  taskCount,
  tasksOpen,
  theme,
  onSelectSection,
  onPickMethod,
  onHome,
  onSearch,
  onToggleCollapsed,
  onToggleTasks,
  onToggleTheme,
  onOpenSettings,
}: SideNavProps) {
  const recent = recentMethods
    .map((id) => findMethod(id))
    .filter((method): method is MethodDef => Boolean(method))
    .slice(0, RECENT_LIMIT);

  const readyOf = (section: SectionDef) => section.methods.filter((method) => method.implemented).length;

  return (
    <nav className={`sidenav ${collapsed ? "rail" : ""}`} aria-label="转换分区">
      <div className="side-head">
        <button type="button" className="appmark" title="返回首页" aria-label="返回首页" onClick={onHome}>
          <img src={appIconUrl} alt="" />
        </button>
        {collapsed ? null : (
          <button type="button" className="side-brand-name" onClick={onHome} title="返回首页">
            Transmutio
          </button>
        )}
      </div>

      <button
        type="button"
        className="collapse"
        title={collapsed ? "展开侧栏" : "收起侧栏"}
        aria-label={collapsed ? "展开侧栏" : "收起侧栏"}
        aria-expanded={!collapsed}
        onClick={onToggleCollapsed}
      >
        <Icon name={collapsed ? "chevron-right" : "chevron-left"} size={14} />
      </button>

      {collapsed ? (
        <div className="rail-scroll">
          {sections.map((section) => (
            <button
              key={section.id}
              type="button"
              className={`railbtn ${activeSection === section.id ? "active" : ""}`}
              title={section.name}
              aria-label={section.name}
              onClick={() => onSelectSection(section)}
            >
              <Icon name={section.icon} size={17} />
            </button>
          ))}
        </div>
      ) : (
        <>
          <label className="searchbox">
            <Icon name="search" size={13} />
            <input
              value={search}
              onChange={(event) => onSearch(event.target.value)}
              placeholder="搜索转换方法…"
              aria-label="搜索转换方法"
            />
          </label>

          <div className="side-scroll">
            {recent.length ? (
              <div className="side-group">
                <div className="side-label">最近使用</div>
                {recent.map((method) => (
                  <button
                    key={method.id}
                    type="button"
                    className={`ritem ${activeMethodId === method.id ? "active" : ""}`}
                    onClick={() => onPickMethod(method)}
                  >
                    <Icon name={method.icon} size={15} />
                    <span className="ritem-name">{method.title}</span>
                  </button>
                ))}
              </div>
            ) : null}

            <div className="side-group">
              <div className="side-label">转换分区</div>
              {sections.map((section) => {
                const ready = readyOf(section);
                const open = activeSection === section.id;
                return (
                  <div key={section.id} className="nav-group">
                    <button
                      type="button"
                      className={`grow ${open ? "active" : ""}`}
                      onClick={() => onSelectSection(section)}
                      aria-expanded={open}
                    >
                      <Icon name={section.icon} size={16} />
                      <span>{section.name}</span>
                      <span className="count" title={`${ready} / ${section.methods.length} 已实现`}>
                        {ready > 0 ? ready : "—"}
                      </span>
                    </button>
                    {open ? (
                      <div className="subs">
                        {section.methods.map((method) => (
                          <button
                            key={method.id}
                            type="button"
                            className={`sub ${activeMethodId === method.id ? "active" : ""}`}
                            onClick={() => onPickMethod(method)}
                          >
                            {method.title}
                          </button>
                        ))}
                      </div>
                    ) : null}
                  </div>
                );
              })}
            </div>
          </div>
        </>
      )}

      {collapsed ? (
        <div className="rail-foot">
          <button
            type="button"
            className={`railbtn ${tasksOpen ? "active" : ""}`}
            title="任务中心"
            aria-label="任务中心"
            onClick={onToggleTasks}
          >
            <Icon name="queue" size={17} />
            {taskCount > 0 ? <span className="badge-dot">{taskCount}</span> : null}
          </button>
          <button type="button" className="railbtn" title="切换主题" aria-label="切换主题" onClick={onToggleTheme}>
            <Icon name={theme === "dark" ? "sun" : "moon"} size={17} />
          </button>
          <button type="button" className="railbtn" title="设置" aria-label="设置" onClick={onOpenSettings}>
            <Icon name="settings" size={17} />
          </button>
        </div>
      ) : (
        <div className="side-foot">
          <div className="outsum">
            <b>输出位置</b>
            <span className="path">{outputDirMode === "custom" && outputDir ? outputDir : "与源文件相同目录"}</span>
          </div>
          <div className="iconrow">
            <button
              type="button"
              className={`sibtn ${tasksOpen ? "active" : ""}`}
              title="任务中心"
              aria-label="任务中心"
              onClick={onToggleTasks}
            >
              <Icon name="queue" size={16} />
              {taskCount > 0 ? <span className="badge-dot">{taskCount}</span> : null}
            </button>
            <button type="button" className="sibtn" title="切换主题" aria-label="切换主题" onClick={onToggleTheme}>
              <Icon name={theme === "dark" ? "sun" : "moon"} size={16} />
            </button>
            <button type="button" className="sibtn" title="设置" aria-label="设置" onClick={onOpenSettings}>
              <Icon name="settings" size={16} />
            </button>
          </div>
        </div>
      )}
    </nav>
  );
}
