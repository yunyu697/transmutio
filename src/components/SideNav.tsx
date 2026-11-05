import { sections, type SectionDef } from "../catalog";
import { Icon } from "./Icon";
import type { ToolsInfo } from "../lib/api";

export function SideNav({
  activeSection,
  tools,
  onSelect,
}: {
  activeSection: string;
  tools: ToolsInfo | null;
  onSelect: (section: SectionDef) => void;
}) {
  return (
    <nav className="sidenav">
      <div className="nav-title">转换分区</div>
      {sections.map((section) => {
        const ready = section.methods.filter((method) => method.implemented).length;
        return (
          <button
            key={section.id}
            className={`navitem ${activeSection === section.id ? "active" : ""}`}
            onClick={() => onSelect(section)}
            type="button"
          >
            <Icon name={section.icon} size={16} />
            <span className="nav-meta">
              <b>{section.name}</b>
              <small>{section.hint}</small>
            </span>
            <span className="count" title={`${ready} / ${section.methods.length} 已实现`}>
              {ready > 0 ? ready : "—"}
            </span>
          </button>
        );
      })}

      <div className="nav-footer">
        <div>
          {tools?.ffmpeg ? (
            <>
              <span style={{ color: "var(--ff-ok)" }}>●</span> FFmpeg {tools.ffmpegVersion ?? "已找到"}
            </>
          ) : (
            <>
              <span style={{ color: "var(--ff-danger)" }}>●</span> 未找到 FFmpeg
            </>
          )}
        </div>
        <div>
          {tools?.build.lgplOk ? "LGPL 构建 · 可分发" : tools?.build.gpl || tools?.build.nonfree ? "GPL/nonfree 构建 · 仅开发" : "构建许可未知"}
        </div>
        <div style={{ opacity: 0.8 }}>v0.1.0 MVP · {tools?.targetTriple ?? "—"}</div>
      </div>
    </nav>
  );
}
