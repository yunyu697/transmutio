import { sections, type SectionDef } from "../catalog";
import { BrandMark } from "./BrandMark";
import { Icon } from "./Icon";

export interface SideNavProps {
  activeSection: string;
  activeMethodId: string | null;
  collapsed: boolean;
  search: string;
  taskCount: number;
  tasksOpen: boolean;
  /** 解析后的亮度：true = 深色，只用于太阳/月亮图标方向。 */
  dark: boolean;
  onSelectSection: (section: SectionDef) => void;
  /** 回到当前分区的方法列表（品牌名/图标点击）。 */
  onGoRoot: () => void;
  onSearch: (value: string) => void;
  onToggleCollapsed: () => void;
  onToggleTasks: () => void;
  onToggleTheme: () => void;
  onOpenSettings: () => void;
}

/**
 * 左栏：只有一个入口层级——分区。方法列表统一放在主区，
 * 不再有「最近使用」和分区子项两套重复入口。
 * 折叠按钮固定在左下角、靠右下沿：right/bottom 各 10px，与侧栏其它元素的内边距
 * 以及底部图标行的底边对齐。两种状态共用同一个按钮、同一组锚点偏移，
 * 只换图标方向与 aria-label，不换按钮也不跳位置。
 */
export function SideNav({
  activeSection,
  activeMethodId,
  collapsed,
  search,
  taskCount,
  tasksOpen,
  dark,
  onSelectSection,
  onGoRoot,
  onSearch,
  onToggleCollapsed,
  onToggleTasks,
  onToggleTheme,
  onOpenSettings,
}: SideNavProps) {
  const readyOf = (section: SectionDef) => section.methods.filter((method) => method.implemented).length;

  return (
    <nav className={`sidenav ${collapsed ? "rail" : ""}`} aria-label="转换分区">
      <div className="side-head">
        <button type="button" className="appmark" title="返回方法列表" aria-label="返回方法列表" onClick={onGoRoot}>
          <BrandMark />
        </button>
        {collapsed ? null : (
          <button type="button" className="side-brand-name" onClick={onGoRoot} title="返回方法列表">
            Transmutio
          </button>
        )}
      </div>

      {collapsed ? (
        <div className="rail-scroll">
          {sections.map((section) => (
            <button
              key={section.id}
              type="button"
              className={`railbtn ${activeSection === section.id ? "active" : ""}`}
              title={section.name}
              aria-label={section.name}
              aria-current={activeSection === section.id ? "true" : undefined}
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
            <div className="side-group">
              <div className="side-label">转换分区</div>
              {sections.map((section) => {
                const ready = readyOf(section);
                const open = activeSection === section.id;
                return (
                  <button
                    key={section.id}
                    type="button"
                    className={`grow ${open ? "active" : ""}`}
                    onClick={() => onSelectSection(section)}
                    aria-current={open ? "true" : undefined}
                    title={`${section.name} · ${section.hint}`}
                  >
                    <Icon name={section.icon} size={16} />
                    <span>{section.name}</span>
                    <span className="count" title={`${ready} / ${section.methods.length} 已实现`}>
                      {ready > 0 ? ready : "—"}
                    </span>
                  </button>
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
          <button
            type="button"
            className="railbtn"
            title={dark ? "切换到浅色模式" : "切换到深色模式"}
            aria-label={dark ? "切换到浅色模式" : "切换到深色模式"}
            onClick={onToggleTheme}
          >
            <Icon name={dark ? "sun" : "moon"} size={17} />
          </button>
          <button type="button" className="railbtn" title="设置" aria-label="设置" onClick={onOpenSettings}>
            <Icon name="settings" size={17} />
          </button>
        </div>
      ) : (
        <div className="side-foot">
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
            <button
              type="button"
              className="sibtn"
              title={dark ? "切换到浅色模式" : "切换到深色模式"}
              aria-label={dark ? "切换到浅色模式" : "切换到深色模式"}
              onClick={onToggleTheme}
            >
              <Icon name={dark ? "sun" : "moon"} size={16} />
            </button>
            <button type="button" className="sibtn" title="设置" aria-label="设置" onClick={onOpenSettings}>
              <Icon name="settings" size={16} />
            </button>
          </div>
        </div>
      )}

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
    </nav>
  );
}
