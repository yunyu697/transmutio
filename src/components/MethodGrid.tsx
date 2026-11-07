import type { MethodDef } from "../catalog";
import { Icon } from "./Icon";

export function MethodGrid({
  title,
  subtitle,
  methods,
  onPick,
  onUnimplemented,
  disabledReasons,
  onDisabled,
}: {
  title: string;
  subtitle: string;
  methods: MethodDef[];
  onPick: (method: MethodDef) => void;
  onUnimplemented: (method: MethodDef) => void;
  /** 已实现但当前环境缺依赖的方法：id → 用户可读原因。 */
  disabledReasons?: Record<string, string>;
  onDisabled?: (method: MethodDef, reason: string) => void;
}) {
  return (
    <>
      <div className="page-head">
        <div>
          <h1>{title}</h1>
          <p>{subtitle}</p>
        </div>
        <div className="spacer" />
        <div className="muted" style={{ fontSize: 10.5, color: "var(--ff-fg3)" }}>
          本版实现：视频 · 音频 · 图片
        </div>
      </div>
      <div className="method-scroll">
        {methods.length ? (
          <div className="method-grid">
            {methods.map((method) => {
              const missing = disabledReasons?.[method.id];
              const usable = method.implemented && !missing;
              return (
                <button
                  key={method.id}
                  type="button"
                  className={`card ${usable ? "" : "disabled"}`}
                  onClick={() =>
                    usable
                      ? onPick(method)
                      : missing
                        ? onDisabled?.(method, missing)
                        : onUnimplemented(method)
                  }
                  title={usable ? method.desc : missing ?? `${method.title}：本版未实现`}
                >
                  <div className="card-top">
                    <span className="card-ico">
                      <Icon name={method.icon} size={16} />
                    </span>
                    <h3>{method.title}</h3>
                    <span className={`badge ${usable ? "ok" : "todo"}`}>
                      {usable ? "可用" : missing ? "需外部工具" : "未实现"}
                    </span>
                  </div>
                  <p>{method.desc}</p>
                  {missing ? (
                    <p style={{ color: "var(--ff-fg3)", marginTop: 6 }}>{missing}</p>
                  ) : null}
                </button>
              );
            })}
          </div>
        ) : (
          <div className="empty">
            <div className="big">
              <Icon name="search" size={22} />
            </div>
            <div>没有匹配的方法</div>
            <div style={{ fontSize: 10.5, opacity: 0.8 }}>试试「MP4」「MP3」「缩放」等关键词</div>
          </div>
        )}
        <div className="note" style={{ marginTop: 18 }}>
          未实现的方法保留入口，方便查看后续规划；点击会明确提示「未实现」，不会执行任何假转换。
        </div>
      </div>
    </>
  );
}
