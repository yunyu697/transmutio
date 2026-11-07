import { findMethod, sections, type MethodDef, type SectionDef } from "../catalog";
import { readMethodParams, summarizeParams } from "../lib/presets";
import { Icon } from "./Icon";

/** 首页：顶部常用入口 + 分区速览卡（方案 A：信息卡，不做大留白）。 */
export function HomeView({
  recentMethods,
  onPickSection,
  onPickMethod,
}: {
  recentMethods: string[];
  onPickSection: (section: SectionDef) => void;
  onPickMethod: (method: MethodDef) => void;
}) {
  const quickMethods = ["video-mp4", "audio-aac", "image-convert"]
    .map((id) => findMethod(id))
    .filter((method): method is MethodDef => Boolean(method));

  const lastMethodOf = (section: SectionDef): MethodDef => {
    for (const id of recentMethods) {
      const method = findMethod(id);
      if (method?.sectionId === section.id) return method;
    }
    return section.methods.find((method) => method.quick) ?? section.methods[0];
  };

  return (
    <>
      <div className="page-head">
        <div>
          <h1>开始转换</h1>
          <p>选择分区或常用入口；左侧可展开分区内全部方法</p>
        </div>
      </div>

      <div className="quick">
        {quickMethods.map((method) => (
          <button key={method.id} type="button" className="qcard" onClick={() => onPickMethod(method)}>
            <span className="qico">
              <Icon name={method.icon} size={15} />
            </span>
            <b>{method.title}</b>
            <small>{method.desc}</small>
          </button>
        ))}
      </div>

      <div className="overviews">
        {sections.map((section) => {
          const quick = section.methods.filter((method) => method.quick).slice(0, 6);
          const last = lastMethodOf(section);
          const summary = summarizeParams(readMethodParams(last.id));
          const ready = section.methods.filter((method) => method.implemented).length;
          return (
            <div key={section.id} className="ov">
              <button type="button" className="ovtop" onClick={() => onPickSection(section)}>
                <Icon name={section.icon} size={15} />
                {section.name}
                <span className="n">{ready} 项</span>
              </button>
              <div className="chips">
                {quick.map((method) => (
                  <button key={method.id} type="button" className="chip" onClick={() => onPickMethod(method)}>
                    {method.title}
                  </button>
                ))}
              </div>
              <div className="ovmetaline">
                <b>常用格式</b>
                {section.formats}
              </div>
              <div className="ovmetaline">
                <b>适用场景</b>
                {section.scenes}
              </div>
              <div className="ovmetaline">
                <b>上次</b>
                {summary ? `${last.title} · ${summary}` : `${last.title} · 还没有使用记录`}
                <button type="button" className="go" onClick={() => onPickMethod(last)}>
                  继续 ›
                </button>
              </div>
            </div>
          );
        })}
      </div>

      <div className="fmtstrip">
        <span>输出位置跟随设置</span>
        <span>同名文件默认加序号，不覆盖</span>
        <span>全部转换在本机完成</span>
      </div>
    </>
  );
}
