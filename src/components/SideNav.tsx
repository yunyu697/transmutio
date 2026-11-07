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

  if (collapsed) {
    return (
      <nav className="sidenav rail" aria-label="转换分区">
        <div className="rail-brand">
          <button type="button" className="appmark rail-mark" title="返回首页" aria-label="返回首页" onClick={onHome}>
            <img src={appIconUrl} alt="" />
          </button>
        </div>
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
          <span className="railsep" />
          <button
            type="button"
            className="railbtn"
            title="展开侧栏"
            aria-label="展开侧栏"
            onClick={onToggleCollapsed}
          >
            <Icon name="chevron-right" size={17} />
          </button>
        </div>
      </nav>
    );
  }

  return (
    <nav className="sidenav" aria-label="转换分区">
      <div className="side-top">
        <span className="appmark" title="Transmutio">
          <img src={appIconUrl} alt="" />
        </span>
        <button type="button" className="side-brand-name" onClick={onHome} title="返回首页">
          Transmutio
        </button>
        <button
          type="button"
          className="collapse"
          title="收起侧栏"
          aria-label="收起侧栏"
          onClick={onToggleCollapsed}
        >
          <Icon name="chevron-left" size={14} />
        </button>
      </div>

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
          <>
            <div className="side-label">最近使用</div>
            {recent.map((method) => (
              <button
                key={method.id}
                type="button"
                className={`ritem ${activeMethodId === method.id ? "active" : ""}`}
                onClick={() => onPickMethod(method)}
              >
                <Icon name={method.icon} size={14} />
                <span className="ritem-name">{method.title}</span>
              </button>
            ))}
          </>
        ) : null}

        <div className="side-label">转换分区</div>
        {sections.map((section) => {
          const ready = section.methods.filter((method) => method.implemented).length;
          const open = activeSection === section.id;
          return (
            <div key={section.id}>
              <button
                type="button"
                className={`grow ${open ? "active" : ""}`}
                onClick={() => onSelectSection(section)}
              >
                <Icon name={section.icon} size={15} />
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

      <div className="side-foot">
        <div className="outsum">
          <b>输出位置</b>
          <span className="path">
            {outputDirMode === "custom" && outputDir ? outputDir : "与源文件相同目录"}
          </span>
        </div>
        <div className="iconrow">
          <button
            type="button"
            className={`sibtn ${tasksOpen ? "active" : ""}`}
            title="任务中心"
            aria-label="任务中心"
            onClick={onToggleTasks}
          >
            <Icon name="queue" size={15} />
            {taskCount > 0 ? <span className="badge-dot">{taskCount}</span> : null}
          </button>
          <button type="button" className="sibtn" title="切换主题" aria-label="切换主题" onClick={onToggleTheme}>
            <Icon name={theme === "dark" ? "sun" : "moon"} size={15} />
          </button>
          <button type="button" className="sibtn" title="设置" aria-label="设置" onClick={onOpenSettings}>
            <Icon name="settings" size={15} />
          </button>
        </div>
      </div>
    </nav>
  );
}
