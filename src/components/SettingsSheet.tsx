import { useState } from "react";
import { api, type Capabilities, type FlavorInfo, type ToolsInfo } from "../lib/api";
import type { Settings } from "../lib/settings";
import { Icon } from "./Icon";
import { Field, Group, Segmented, SwitchRow } from "../panels/shared";
import { UpdatePanel } from "./UpdatePanel";
import { applyPreset, collectPreset } from "../lib/presetPrefs";

export function SettingsSheet({
  open,
  settings,
  flavor,
  tools,
  caps,
  probing,
  onClose,
  onPatch,
  onApplyPath,
  onReprobe,
  onExportDiagnostics,
  toast,
}: {
  open: boolean;
  settings: Settings;
  flavor: FlavorInfo | null;
  tools: ToolsInfo | null;
  caps: Capabilities | null;
  probing: boolean;
  onClose: () => void;
  onPatch: (patch: Partial<Settings>) => void;
  onApplyPath: (path: string) => void;
  onReprobe: () => void;
  onExportDiagnostics: () => Promise<string>;
  toast: (text: string, kind?: "ok" | "error" | "info") => void;
}) {
  const [ffmpegPath, setFfmpegPath] = useState(settings.ffmpegPath);
  if (!open) return null;

  return (
    <div className="drawer-backdrop" onClick={onClose}>
      <div className="drawer" onClick={(event) => event.stopPropagation()}>
        <header>
          <Icon name="settings" size={16} />
          <b>设置与能力</b>
          <button className="iconbtn" style={{ marginLeft: "auto" }} onClick={onClose} title="关闭">
            <Icon name="close" size={14} />
          </button>
        </header>

        <div className="drawer-body">
          <Group title="外观与语言" icon="sun">
            <Field label="主题">
              <Segmented
                value={settings.theme}
                onChange={(value) => onPatch({ theme: value })}
                options={[
                  { value: "dark", label: "深色（默认）" },
                  { value: "light", label: "浅色" },
                ]}
              />
            </Field>
            <div className="note">界面语言：本版仅提供简体中文。en-US 词典仅为未完成骨架，暂不提供切换。</div>
          </Group>

          <Group title="任务与输出" icon="folder">
            <Field label="同时运行的任务数">
              <select
                className="select"
                value={settings.concurrency}
                onChange={(event) => onPatch({ concurrency: Number(event.target.value) })}
              >
                {[1, 2, 3, 4, 5, 6, 7, 8].map((value) => (
                  <option key={value} value={value}>
                    {value}（默认 4）
                  </option>
                ))}
              </select>
            </Field>
            <Field label="输出位置">
              <div className="radio-group">
                <label className="radio">
                  <input
                    type="radio"
                    name="output-mode"
                    checked={settings.outputDirMode === "source"}
                    onChange={() => onPatch({ outputDirMode: "source" })}
                  />
                  与源文件相同目录
                </label>
                <label className="radio">
                  <input
                    type="radio"
                    name="output-mode"
                    checked={settings.outputDirMode === "custom"}
                    onChange={() => onPatch({ outputDirMode: "custom" })}
                  />
                  自定义目录
                </label>
              </div>
              <div style={{ display: "flex", gap: 6, marginTop: 6 }}>
                <input
                  className="input"
                  value={settings.outputDir}
                  disabled={settings.outputDirMode !== "custom"}
                  onChange={(event) => onPatch({ outputDir: event.target.value })}
                  placeholder={
                    settings.outputDirMode === "custom"
                      ? "选择或填写输出目录"
                      : "当前写回每个源文件所在目录"
                  }
                />
                <button
                  className="btn"
                  disabled={settings.outputDirMode !== "custom"}
                  onClick={async () => {
                    const picked = await import("@tauri-apps/plugin-dialog").then((m) =>
                      m.open({ directory: true, multiple: false }),
                    );
                    if (typeof picked === "string") onPatch({ outputDir: picked });
                  }}
                >
                  选择
                </button>
              </div>
            </Field>
            <Field label="批次完成后">
              <select
                className="select"
                value={settings.afterAction}
                onChange={(event) => {
                  const value = event.target.value as Settings["afterAction"];
                  if (value === "shutdown" && !window.confirm("关机是系统级操作：确定允许？运行时还需设置环境变量 FORMATFORGE_ALLOW_SHUTDOWN=1 才会真正执行。")) {
                    return;
                  }
                  onPatch({ afterAction: value });
                }}
              >
                <option value="none">什么都不做</option>
                <option value="open-folder">自动打开输出目录</option>
                <option value="shutdown">关机（默认只构造命令，不执行）</option>
              </select>
            </Field>
            <div className="note">
              每个面板默认「加序号」：同名时生成「文件名 (1).ext」，绝不覆盖已有文件。跳过 / 覆盖需在面板的「输出命名」里显式选择。
            </div>
            {settings.afterAction === "shutdown" ? (
              <div className="note warn">
                关机动作默认被环境变量保护：只有当进程环境里存在 FORMATFORGE_ALLOW_SHUTDOWN=1 时才会真正下发 `shutdown /s /t 60`；否则只记录不执行。
              </div>
            ) : null}
          </Group>

          <Group title="FFmpeg" icon="chip">
            <Field label="可执行文件路径（留空自动查找）">
              <div style={{ display: "flex", gap: 6 }}>
                <input
                  className="input"
                  value={ffmpegPath}
                  onChange={(event) => setFfmpegPath(event.target.value)}
                  placeholder="例如 C:\ffmpeg\bin\ffmpeg.exe"
                />
                <button className="btn" onClick={() => onApplyPath(ffmpegPath)}>
                  应用并探测
                </button>
              </div>
            </Field>
            <Field label={flavor?.id === "gpl" ? "GPL 构建下载源（引导页使用）" : "LGPL 构建下载源（引导页使用）"}>
              <input
                className="input"
                value={settings.ffmpegSourceUrl}
                onChange={(event) => onPatch({ ffmpegSourceUrl: event.target.value })}
                placeholder={flavor?.defaultSourceUrl ?? "https://ffmpeg.org/download.html"}
              />
            </Field>
            {tools ? (
              <>
                <div className="kv">
                  <span>来源</span>
                  <b>{tools.ffmpeg?.source ?? "未找到"}</b>
                </div>
                <div className="kv">
                  <span>路径</span>
                  <b>{tools.ffmpeg?.path ?? "—"}</b>
                </div>
                <div className="kv">
                  <span>版本</span>
                  <b>{tools.ffmpegVersion ?? "—"}</b>
                </div>
                <div className="kv">
                  <span>平台</span>
                  <b>
                    {tools.platform} / {tools.arch}（{tools.targetTriple}）
                  </b>
                </div>
                <div className="note">{tools.searchNote}</div>
                {tools.build.nonfree ? (
                  <div className="note danger">
                    当前 FFmpeg 含 nonfree 组件，任何变体都不接受，也无法用于分发。
                  </div>
                ) : tools.build.gpl ? (
                  flavor?.id === "gpl" ? (
                    <div className="note">构建许可：GPL，符合 GPL 版（Transmutio GPL），可用 libx264 / libx265。</div>
                  ) : (
                    <div className="note danger">
                      GPL 构建：LGPL 版不接受；如需 x264/x265 软件编码，请改用 GPL 版（Transmutio GPL）。
                    </div>
                  )
                ) : flavor?.id === "gpl" ? (
                  <div className="note danger">
                    当前是 GPL 版：需要 GPL 构建的 FFmpeg（含 libx264 / libx265），LGPL 构建不可用。
                  </div>
                ) : (
                  <div className="note">构建许可：LGPL，可随 LGPL 安装包分发（无 libx264 / libx265）。</div>
                )}
                {tools.warnings.map((warning) => (
                  <div key={warning} className="note warn">
                    {warning}
                  </div>
                ))}
              </>
            ) : (
              <div className="note warn">尚未解析 FFmpeg 位置。</div>
            )}
          </Group>

          <Group title="硬件能力探测" icon="gpu">
            <div style={{ display: "flex", alignItems: "center", gap: 8 }}>
              <span style={{ fontSize: 10.5, color: "var(--ff-fg3)" }}>
                {caps ? `探测于 ${new Date(caps.probedAt).toLocaleString("zh-CN")} · ${caps.cacheHit ? "缓存命中" : "重新探测"}` : "尚未探测"}
              </span>
              <button className="btn small" style={{ marginLeft: "auto" }} onClick={onReprobe} disabled={probing}>
                <Icon name="retry" size={12} />
                {probing ? "探测中…" : "重新探测"}
              </button>
            </div>
            {caps ? (
              <>
                {caps.videoEncoders.map((encoder) => (
                  <div key={encoder.id} className="cap-row">
                    <span className={`enc-dot ${encoder.available && encoder.allowed ? "ok" : "bad"}`} />
                    <span className="nm">
                      {encoder.label}
                      <small>
                        {encoder.id} · {encoder.hardware ? "硬件" : "软件"} · {encoder.license}
                        {encoder.reason ? ` · ${encoder.reason}` : " · 试编通过"}
                      </small>
                    </span>
                  </div>
                ))}
                {caps.audioEncoders.map((encoder) => (
                  <div key={`a-${encoder.id}`} className="cap-row">
                    <span className={`enc-dot ${encoder.available && encoder.allowed ? "ok" : "bad"}`} />
                    <span className="nm">
                      {encoder.label}
                      <small>
                        {encoder.id} · {encoder.license}
                        {encoder.reason ? ` · ${encoder.reason}` : " · 试编通过"}
                      </small>
                    </span>
                  </div>
                ))}
                {caps.decodeAccels.map((accel) => (
                  <div key={`d-${accel.id}`} className="cap-row">
                    <span className={`enc-dot ${accel.available ? "ok" : "bad"}`} />
                    <span className="nm">
                      解码 {accel.label}
                      <small>{accel.available ? "真实样本验证通过" : accel.reason ?? "不可用"}</small>
                    </span>
                  </div>
                ))}
                <div className="kv">
                  <span>指纹</span>
                  <b>{caps.fingerprint}</b>
                </div>
                {caps.notes.map((note) => (
                  <div key={note} className="note">
                    {note}
                  </div>
                ))}
              </>
            ) : (
              <div className="note">启动后会自动探测；也可手动触发。</div>
            )}
          </Group>

          <Group title="许可策略" icon="alert">
            {flavor?.requireGplBuild ? (
              <>
                <div className="kv">
                  <span>GPL 编码器</span>
                  <b>编译期固定：始终允许</b>
                </div>
                <div className="note">
                  这是 GPL 版（Transmutio GPL）：allowGpl 在编译期固定为 true，设置项或环境变量都不能关闭；
                  只接受 GPL 构建的 FFmpeg（含 libx264 / libx265）。更新通道 {flavor.updateChannel}。
                </div>
              </>
            ) : flavor?.devOverrideAvailable ? (
              <>
                <SwitchRow
                  label="允许 GPL / nonfree 编码器"
                  hint="默认关闭。开启后仅在开发验证时可用 GPL 编码器（libx264/libx265）与 nonfree 组件，不得随安装包分发。"
                  on={settings.allowGplEncoders}
                  onChange={(value) => onPatch({ allowGplEncoders: value })}
                />
                {settings.allowGplEncoders ? (
                  <div className="note danger">
                    已开启 GPL / nonfree：当前能力表会放行 GPL 编码器。发布构建请保持关闭，并替换为 LGPL FFmpeg。
                  </div>
                ) : null}
              </>
            ) : (
              <>
                <div className="kv">
                  <span>GPL 编码器</span>
                  <b>发布版固定拒绝</b>
                </div>
                <div className="note">
                  这是 LGPL 发布版：编译期固定拒绝 GPL / nonfree 构建，没有可切换的开关。
                  如需 x264 / x265 软件编码，请改用 GPL 版（Transmutio GPL）。更新通道 {flavor?.updateChannel}。
                </div>
              </>
            )}
          </Group>

          <Group title="预设导入 / 导出" icon="document">
            <div style={{ display: "flex", gap: 6 }}>
              <button
                className="btn"
                onClick={async () => {
                  const path = await import("@tauri-apps/plugin-dialog").then((m) =>
                    m.save({
                      defaultPath: "transmutio-preset.json",
                      filters: [{ name: "Transmutio 预设", extensions: ["json"] }],
                    }),
                  );
                  if (!path) return;
                  try {
                    const { panels, output } = collectPreset(settings.afterAction);
                    const preset = await api.exportPreset(path, panels, output);
                    toast(
                      `预设已导出：${path}（${Object.keys(preset.panels).length} 个面板，不含路径与凭据）`,
                      "ok",
                    );
                  } catch (error) {
                    toast(`导出预设失败：${error instanceof Error ? error.message : String(error)}`, "error");
                  }
                }}
              >
                导出预设
              </button>
              <button
                className="btn"
                onClick={async () => {
                  const path = await import("@tauri-apps/plugin-dialog").then((m) =>
                    m.open({
                      multiple: false,
                      directory: false,
                      filters: [{ name: "Transmutio 预设", extensions: ["json"] }],
                    }),
                  );
                  if (typeof path !== "string") return;
                  try {
                    const preset = await api.importPreset(path);
                    const { applied, afterAction } = applyPreset(preset);
                    if (afterAction) onPatch({ afterAction });
                    const parts: string[] = [];
                    if (applied.length) {
                      parts.push(`已导入 ${applied.length} 个面板参数（${applied.join(" / ")}），重新打开对应面板后生效`);
                    }
                    if (afterAction) parts.push("批次完成后的动作已同步");
                    toast(
                      parts.length ? `预设导入完成：${parts.join("；")}` : "预设校验通过，但没有可应用的参数",
                      "ok",
                    );
                  } catch (error) {
                    toast(`导入失败：${error instanceof Error ? error.message : String(error)}`, "error");
                  }
                }}
              >
                导入预设
              </button>
            </div>
            <div className="note">
              预设包含面板参数、输出命名策略与「批次完成后」动作，不含本机路径、输出目录或凭据。导入时会校验版本、字段白名单与字段类型，
              非法字段会逐条报错，不会被静默忽略。
            </div>
          </Group>

          <Group title="诊断与更新" icon="info">
            <UpdatePanel toast={toast} />
            <button
              className="btn"
              style={{ marginTop: 8 }}
              onClick={async () => {
                try {
                  const path = await onExportDiagnostics();
                  toast(`诊断日志已保存：${path}`, "ok");
                } catch (error) {
                  toast(`保存诊断日志失败：${error instanceof Error ? error.message : String(error)}`, "error");
                }
              }}
            >
              导出诊断日志
            </button>
          </Group>

          <Group title="关于" icon="info">
            <div className="kv">
              <span>版本</span>
              <b>Transmutio 0.1.0 MVP</b>
            </div>
            <div className="kv">
              <span>发行变体</span>
              <b>{flavor?.productName ?? "未知"}（{flavor?.id?.toUpperCase() ?? "—"}）</b>
            </div>
            <div className="kv">
              <span>许可</span>
              <b>{flavor?.licenseLabel ?? "—"}</b>
            </div>
            <div className="kv">
              <span>更新通道</span>
              <b>{flavor?.updateChannel ?? "—"}</b>
            </div>
            <div className="kv">
              <span>应用标识</span>
              <b>{flavor?.appIdentifier ?? "—"}</b>
            </div>
            <div className="kv">
              <span>FFmpeg 构建配置</span>
              <b>
                {tools
                  ? `${tools.build.version || "—"} · ${tools.build.nonfree ? "nonfree" : tools.build.gpl ? "gpl" : "lgpl"}`
                  : "—"}
              </b>
            </div>
            <div className="kv">
              <span>技术栈</span>
              <b>Tauri 2 · React 19 · TypeScript</b>
            </div>
            <div className="kv">
              <span>架构</span>
              <b>{tools ? `${tools.platform}/${tools.arch}` : "—"}</b>
            </div>
          </Group>
        </div>
      </div>
    </div>
  );
}
