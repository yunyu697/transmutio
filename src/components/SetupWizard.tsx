import { useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import {
  api,
  type FfToolInspection,
  type FlavorInfo,
  type ToolsDownloadProgress,
  type ToolsPlan,
  type ToolsInfo,
} from "../lib/api";
import { formatBytes } from "@formatforge/design-tokens";
import type { Settings } from "../lib/settings";
import { Icon } from "./Icon";

const DEFAULT_SOURCE = "https://ffmpeg.org/download.html";

/**
 * 首次启动引导：缺失 FFmpeg、或当前构建与变体许可策略不符时展示。
 * 变体（LGPL / GPL）在编译期固定：文案、默认下载源、许可提示、可接受的构建都不同。
 * 明确不做：静默下载、代用户接受许可、引入 nonfree 组件。
 */
export function SetupWizard({
  tools,
  flavor,
  settings,
  probing,
  onPatch,
  onReprobe,
  onInstalled,
  toast,
  onDevOverride,
}: {
  tools: ToolsInfo | null;
  flavor: FlavorInfo | null;
  settings: Readonly<Settings>;
  probing: boolean;
  onPatch: (patch: Partial<Settings>) => void;
  onReprobe: () => void;
  onInstalled: (info: ToolsInfo) => void;
  toast: (text: string, kind?: "ok" | "error" | "info") => void;
  onDevOverride: () => void;
}) {
  const [inspection, setInspection] = useState<FfToolInspection | null>(null);
  const [expectedHash, setExpectedHash] = useState("");
  const [busy, setBusy] = useState<"pick" | "inspect" | "install" | null>(null);
  const [error, setError] = useState<string | null>(null);
  // 内置下载器：清单来自后端（编译期内置），进度经事件推送。
  const [plan, setPlan] = useState<ToolsPlan | null>(null);
  const [dlSource, setDlSource] = useState(settings.ffmpegDownloadSource);
  const [dlProgress, setDlProgress] = useState<ToolsDownloadProgress | null>(null);
  const [dlBusy, setDlBusy] = useState(false);
  const [dlError, setDlError] = useState<string | null>(null);

  useEffect(() => {
    void api
      .toolsPlan()
      .then(setPlan)
      .catch((e) => setDlError(`读取内置下载清单失败：${e instanceof Error ? e.message : String(e)}`));
  }, []);

  useEffect(() => {
    let unlisten: (() => void) | undefined;
    let disposed = false;
    void listen<ToolsDownloadProgress>("tools-download-progress", (event) => {
      setDlProgress(event.payload);
    })
      .then((fn) => {
        if (disposed) fn();
        else unlisten = fn;
      })
      .catch(() => {});
    return () => {
      disposed = true;
      unlisten?.();
    };
  }, []);

  const ffmpegEntry = plan?.entries.find((entry) => entry.kind === "ffmpeg") ?? null;
  const manualFfmpeg = plan?.manual.find((entry) => entry.kind === "ffmpeg") ?? null;

  const downloadFfmpeg = async () => {
    if (!ffmpegEntry || dlBusy) return;
    setDlBusy(true);
    setDlError(null);
    setDlProgress(null);
    try {
      const landed = await api.downloadTool(ffmpegEntry.packageId, dlSource || undefined);
      if (!landed.tools) throw new Error("下载完成但没有返回工具信息");
      toast(`已下载并校验 ${landed.version}，落位到 ${landed.dir}`, "ok");
      setInspection(null);
      onInstalled(landed.tools);
    } catch (e) {
      setDlError(e instanceof Error ? e.message : String(e));
    } finally {
      setDlBusy(false);
    }
  };

  const cancelDownload = async () => {
    try {
      await api.cancelToolDownload();
    } catch (e) {
      setDlError(e instanceof Error ? e.message : String(e));
    }
  };

  const isGplVariant = flavor?.id === "gpl";
  const effectiveAllowGpl = Boolean(isGplVariant || settings.allowGplEncoders);
  const sourceUrl = settings.ffmpegSourceUrl.trim() || flavor?.defaultSourceUrl || DEFAULT_SOURCE;

  const openSource = async () => {
    try {
      const { openUrl } = await import("@tauri-apps/plugin-opener");
      await openUrl(sourceUrl);
    } catch (e) {
      setError(`打开下载页失败：${e instanceof Error ? e.message : String(e)}`);
    }
  };

  const openPage = async (url: string) => {
    try {
      const { openUrl } = await import("@tauri-apps/plugin-opener");
      await openUrl(url);
    } catch (e) {
      setError(`打开下载页失败：${e instanceof Error ? e.message : String(e)}`);
    }
  };

  const pick = async () => {
    setBusy("pick");
    setError(null);
    try {
      const { open } = await import("@tauri-apps/plugin-dialog");
      const picked = await open({
        multiple: false,
        directory: false,
        title: "选择 ffmpeg 可执行文件（同目录下的 ffprobe 会自动配对）",
      });
      if (typeof picked !== "string") return;
      setBusy("inspect");
      const info = await api.inspectFfmpeg(picked);
      setInspection(info);
      if (!info.exists) setError("文件不存在，请重新选择");
      else if (info.nonfree) setError("该构建包含 nonfree 组件，任何变体都不接受");
      else if (info.gpl && !effectiveAllowGpl)
        setError("该构建为 GPL：当前是 LGPL 版，请改用 GPL 版（Transmutio GPL）");
      else if (!info.gpl && isGplVariant)
        setError("该构建为 LGPL：GPL 版需要 GPL 构建的 FFmpeg（含 libx264/libx265）");
    } catch (e) {
      setError(String(e instanceof Error ? e.message : e));
    } finally {
      setBusy(null);
    }
  };

  const install = async () => {
    if (!inspection) return;
    setBusy("install");
    setError(null);
    try {
      const info = await api.installFfmpeg({
        ffmpegPath: inspection.path,
        expectedSha256: expectedHash.trim() || undefined,
        allowGpl: isGplVariant ? true : settings.allowGplEncoders,
      });
      toast("FFmpeg 已校验并落位到应用数据目录", "ok");
      setInspection(null);
      onInstalled(info);
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setBusy(null);
    }
  };

  const gplBlocked = Boolean(tools && !tools.build.lgplOk);
  const lglBuildOnGplVariant = Boolean(isGplVariant && tools?.ffmpeg?.exists && tools.build.lgplOk);
  const found = tools?.ffmpeg ?? null;

  return (
    <main className="setup">
      <section className="setup-card">
        <div className="setup-head">
          <div className="setup-ico">
            <Icon name="film" size={22} />
          </div>
          <div>
            <h1>
              {isGplVariant
                ? found
                  ? "GPL 版：需要 GPL 构建的 FFmpeg"
                  : "第一次使用：先准备 GPL 构建的 FFmpeg"
                : found
                  ? "当前 FFmpeg 不符合 LGPL 许可路线"
                  : "第一次使用：先准备 FFmpeg"}
            </h1>
            <p>
              {isGplVariant ? (
                <>
                  转换由 FFmpeg 子进程完成。这是 <b>GPL 版</b>（{flavor?.productName ?? "Transmutio GPL"}），
                  只接受 <b>GPL 构建</b>的 FFmpeg（可用 libx264 / libx265 软件编码）。
                  不会在后台静默下载可执行文件。
                </>
              ) : (
                <>
                  转换由 FFmpeg 子进程完成。本应用默认只接受 <b>LGPL</b> 构建：不捆绑 GPL/nonfree
                  组件，也不会在后台静默下载可执行文件。
                </>
              )}
            </p>
          </div>
        </div>

        <div className={`note ${probing ? "" : found ? "warn" : "warn"}`}>
          {probing
            ? "正在探测本机 FFmpeg…"
            : found
              ? `已找到 ${found.path}（${tools?.ffmpegVersion ?? "版本未知"}）${
                  isGplVariant
                    ? lglBuildOnGplVariant
                      ? "，但这是 LGPL 构建，GPL 版不接受。"
                      : gplBlocked
                        ? "，但包含 nonfree 组件，任何变体都不接受。"
                        : ""
                    : gplBlocked
                      ? "，但包含 GPL/nonfree 组件，LGPL 版不接受。如需 x264/x265，请改用 GPL 版。"
                      : ""
                }`
              : "未找到可用的 ffmpeg / ffprobe。可以让应用按内置清单下载并校验，或手动选择本地文件。"}
        </div>

        <ol className="setup-steps">
          <li>
            <b>{isGplVariant ? "自动下载 GPL 构建（可选）" : "自动下载 LGPL 构建（可选）"}</b>
            <p>
              只从内置清单里登记的包下载：官方发布 + 多个加速镜像，失败按顺序回退并显示当前来源。
              无论哪个来源，落盘前都按<b>内置 SHA-256</b> 校验，镜像给的文件一律不采信；
              校验不通过会直接拒绝，不会留下半个文件。
            </p>
            {ffmpegEntry ? (
              <>
                <div className="setup-row">
                  <select
                    className="input"
                    value={dlSource}
                    disabled={dlBusy}
                    onChange={(e) => {
                      setDlSource(e.target.value);
                      onPatch({ ffmpegDownloadSource: e.target.value });
                    }}
                  >
                    <option value="">自动：官方优先，失败按顺序回退</option>
                    {ffmpegEntry.sources.map((source) => (
                      <option key={source.sourceId} value={source.sourceId}>
                        {source.label}
                        {source.mirror ? "（镜像，仍需哈希匹配）" : ""}
                      </option>
                    ))}
                  </select>
                  <button className="btn accent2" onClick={() => void downloadFfmpeg()} disabled={dlBusy}>
                    {dlBusy ? "下载中…" : `下载 ${ffmpegEntry.version}`}
                  </button>
                  {dlBusy ? (
                    <button className="btn ghost" onClick={() => void cancelDownload()}>
                      取消
                    </button>
                  ) : null}
                </div>
                <p className="setup-meta">
                  {ffmpegEntry.buildLabel} · {formatBytes(ffmpegEntry.size)} · SHA-256{" "}
                  {ffmpegEntry.sha256.slice(0, 12)}…
                </p>
                {dlProgress ? (
                  <p className="setup-meta">
                    {dlProgress.sourceLabel ? `[${dlProgress.sourceLabel}] ` : ""}
                    {dlProgress.message}
                    {dlProgress.total > 0
                      ? ` · ${Math.min(100, Math.round((dlProgress.received / dlProgress.total) * 100))}%`
                      : ""}
                  </p>
                ) : null}
                {dlError ? <div className="note danger">{dlError}</div> : null}
              </>
            ) : (
              <>
                <div className="note">
                  {manualFfmpeg?.reason ??
                    (plan ? "当前平台没有内置的可下载包。" : "正在读取内置下载清单…")}
                </div>
                {manualFfmpeg ? (
                  <button className="btn" onClick={() => void openPage(manualFfmpeg.url)}>
                    打开下载页
                  </button>
                ) : null}
              </>
            )}
          </li>

          <li>
            <b>{isGplVariant ? "获取 GPL 构建" : "获取 LGPL 构建"}</b>
            <p>
              {isGplVariant ? (
                <>
                  打开下载页后请选择明确标注 <b>GPL</b> 的构建（Windows 推荐 gyan.dev 的
                  <code>full</code> 或 <code>release</code> 构建，含 libx264 / libx265；Linux 用发行版的
                  ffmpeg 通常已是 GPL；macOS 需自行确认构建选项）。下载源可修改并会保存在本机设置里。
                </>
              ) : (
                <>
                  打开下载页后请选择明确标注 <b>LGPL</b> 的构建（Windows 可选 BtbN 的
                  <code>-lgpl</code> 资产；Linux 用发行版提供的 ffmpeg；macOS 需自行确认构建选项）。
                  下载源可修改并会保存在本机设置里。
                </>
              )}
            </p>
            <div className="setup-row">
              <input
                className="input"
                value={settings.ffmpegSourceUrl}
                placeholder={flavor?.defaultSourceUrl ?? DEFAULT_SOURCE}
                onChange={(e) => onPatch({ ffmpegSourceUrl: e.target.value })}
              />
              <button className="btn" onClick={() => void openSource()}>
                打开下载页
              </button>
            </div>
          </li>

          <li>
            <b>选择已下载的 ffmpeg 可执行文件</b>
            <p>
              只读取版本、构建配置、哈希与编码器列表来判断许可与可用性；不会执行转换，也不会修改源文件。
            </p>
            <div className="setup-row">
              <button className="btn primary" onClick={() => void pick()} disabled={busy !== null}>
                <Icon name="plus" size={14} />
                {busy === "pick" ? "选择中…" : busy === "inspect" ? "校验中…" : "选择 ffmpeg"}
              </button>
              {inspection ? (
                <span className="setup-meta">
                  {inspection.exists ? formatBytes(inspection.size) : "—"} · {inspection.version || "版本未知"}
                </span>
              ) : null}
            </div>

            {inspection?.exists ? (
              <div className="setup-inspect">
                <div className="setup-kv">
                  <span>许可</span>
                  <b className={inspection.lgplOk ? "" : "danger"}>
                    {inspection.nonfree ? "nonfree" : inspection.gpl ? "GPL" : "LGPL"}
                  </b>
                </div>
                <div className="setup-kv">
                  <span>H.264 编码器</span>
                  <b>{inspection.hasH264Encoder ? "有" : "无"}</b>
                </div>
                <div className="setup-kv">
                  <span>AAC 编码器</span>
                  <b>{inspection.hasAacEncoder ? "有" : "无"}</b>
                </div>
                <div className="setup-hash">
                  <span>SHA-256</span>
                  <code>{inspection.sha256 || "读取失败"}</code>
                </div>
                {inspection.notes.map((note) => (
                  <div key={note} className="note warn">
                    {note}
                  </div>
                ))}
                <div className="field">
                  <label htmlFor="expected-hash">
                    期望哈希（可选）<span className="tag">用于比对官方发布值</span>
                  </label>
                  <input
                    id="expected-hash"
                    className="input"
                    placeholder="粘贴发布页提供的 SHA-256；留空则只记录实际值"
                    value={expectedHash}
                    onChange={(e) => setExpectedHash(e.target.value)}
                  />
                </div>
                <button
                  className="btn accent2"
                  onClick={() => void install()}
                  disabled={
                    busy !== null ||
                    inspection.nonfree ||
                    (inspection.gpl && !effectiveAllowGpl) ||
                    (isGplVariant && !inspection.gpl)
                  }
                >
                  {busy === "install" ? "复制并复核…" : "校验通过，落位到应用数据目录"}
                </button>
                {isGplVariant && !inspection.gpl ? (
                  <div className="note danger">
                    GPL 版需要 GPL 构建的 FFmpeg（含 libx264 / libx265）；LGPL 构建不能安装在 GPL 版中。
                  </div>
                ) : !isGplVariant && inspection.gpl && !effectiveAllowGpl ? (
                  <div className="note danger">
                    GPL 构建被 LGPL 版拒绝。如需 x264/x265 软件编码，请改用 GPL 版（Transmutio GPL）。
                    {flavor?.devOverrideAvailable
                      ? "仅在本机开发时，可点击下方「允许 GPL」开发按钮。"
                      : null}
                  </div>
                ) : null}
              </div>
            ) : null}
          </li>

          {found && gplBlocked && !isGplVariant && (flavor?.devOverrideAvailable ?? true) ? (
            <li>
              <b>本机已有构建，仅作开发使用</b>
              <p>
                继续使用 {found.path} 会引入 GPL 组件。该选择只影响本机，不会被写入分发配置。
              </p>
              <button className="btn" onClick={onDevOverride}>
                仅本机开发：允许 GPL
              </button>
            </li>
          ) : null}
        </ol>

        <div className="note">
          {isGplVariant ? (
            <>
              合规说明：GPL 版安装包附带 <code>GPL-2.0.txt</code> / <code>GPL-3.0.txt</code>、
              <code>THIRD-PARTY-NOTICES-GPL.txt</code> 与源码获取说明。我们<b>不随包分发 FFmpeg 二进制</b>，
              FFmpeg 由你自行下载并落位；具体见安装目录下的 <code>licenses/</code>。
            </>
          ) : (
            <>
              合规说明：LGPL 版安装包附带 <code>LGPL-2.1.txt</code> / <code>LGPL-3.0.txt</code>、
              <code>THIRD-PARTY-NOTICES-LGPL.txt</code> 与源码获取说明。我们<b>不随包分发 FFmpeg 二进制</b>，
              FFmpeg 由你自行下载并落位；具体见安装目录下的 <code>licenses/</code>。
            </>
          )}
        </div>

        {error ? <div className="note danger">{error}</div> : null}

        <div className="setup-foot">
          <button className="btn ghost" onClick={onReprobe} disabled={probing || busy !== null}>
            重新检测
          </button>
          <span className="sub">
            不自动下载 · 不代替你接受许可 · 不引入 nonfree 编码器
          </span>
        </div>
      </section>
    </main>
  );
}
