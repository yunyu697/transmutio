import { useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { api, type PendingUpdate, type UpdateCandidate, type UpdateCheckResult, type UpdateProgress, type UpdateSourceAttempt, type UpdaterInfo } from "../lib/api";

function human(bytes: number) {
  if (!bytes) return "未知";
  if (bytes >= 1024 * 1024 * 1024) return `${(bytes / (1024 * 1024 * 1024)).toFixed(2)} GB`;
  if (bytes >= 1024 * 1024) return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
  return `${(bytes / 1024).toFixed(0)} KB`;
}

function Attempts({ attempts }: { attempts: UpdateSourceAttempt[] }) {
  if (!attempts.length) return null;
  const failed = attempts.filter((a) => !a.ok).length;
  return (
    <div className="note" style={{ marginTop: 6 }}>
      来源尝试（成功 {attempts.length - failed} / 失败 {failed}）：
      {attempts.map((a) => (
        <div key={a.id} style={{ fontSize: 12, marginTop: 2 }}>
          {a.ok ? "✓" : "✗"} {a.label} · {a.elapsedMs} ms{a.error ? ` · ${a.error}` : ""}
        </div>
      ))}
    </div>
  );
}

export function UpdatePanel({ toast }: { toast: (text: string, kind?: "ok" | "error" | "info") => void }) {
  const [info, setInfo] = useState<UpdaterInfo | null>(null);
  const [result, setResult] = useState<UpdateCheckResult | null>(null);
  const [pending, setPending] = useState<PendingUpdate | null>(null);
  const [checking, setChecking] = useState(false);
  const [busy, setBusy] = useState<"update" | "download" | "install" | null>(null);
  const [progress, setProgress] = useState<UpdateProgress | null>(null);
  const [attempts, setAttempts] = useState<UpdateSourceAttempt[]>([]);
  const [error, setError] = useState<string | null>(null);
  const [mirror, setMirror] = useState("github");

  useEffect(() => {
    void api.updateInfo().then(setInfo).catch(() => undefined);
    void api.updatePending().then(setPending).catch(() => undefined);
    let stop: (() => void) | undefined;
    listen<UpdateProgress>("update-progress", (event) => setProgress(event.payload))
      .then((fn) => {
        stop = fn;
      })
      .catch(() => undefined);
    return () => stop?.();
  }, []);

  async function doCheck() {
    setChecking(true);
    setError(null);
    setProgress(null);
    try {
      const checked = await api.updateCheck(mirror);
      setResult(checked);
      setAttempts(checked.attempts);
      // 已下载且校验通过的包与检查结果无关，始终保留「安装」。
      setPending(await api.updatePending().catch(() => null));
      if (!checked.available) toast(`当前已是最新版本（${checked.currentVersion}）`, "info");
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setChecking(false);
    }
  }

  async function doDownload(installAfter: boolean) {
    const candidate = result?.candidate;
    if (!candidate) return;
    setBusy(installAfter ? "update" : "download");
    setError(null);
    setProgress(null);
    try {
      const downloaded = await api.updateDownload(candidate, mirror);
      setPending(downloaded.pending);
      setAttempts(downloaded.attempts);
      if (installAfter) {
        if (!downloaded.installable) {
          setError(downloaded.installBlockReason ?? "安装被拒绝");
          return;
        }
        const outcome = await api.updateInstall();
        toast(outcome.message, "ok");
      } else {
        toast(`已下载并校验：${downloaded.pending.assetName}`, "ok");
      }
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
      setPending(await api.updatePending().catch(() => null));
    } finally {
      setBusy(null);
    }
  }

  async function doInstall() {
    if (!pending) return;
    setBusy("install");
    setError(null);
    try {
      const outcome = await api.updateInstall();
      toast(outcome.message, "ok");
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setBusy(null);
    }
  }

  const candidate: UpdateCandidate | null = result?.candidate ?? null;
  const sameVersionPending = !!(pending && candidate && pending.version === candidate.version);
  const showCandidate = !!candidate && !(sameVersionPending && pending.installable);

  return (
    <div>
      <div style={{ display: "flex", gap: 6, flexWrap: "wrap" }}>
        <button className="btn" disabled={checking || !!busy} onClick={() => void doCheck()}>
          {checking ? "检查中…" : "检查更新"}
        </button>
        <button className="btn" onClick={() => void api.openHomepage().catch(() => undefined)}>
          打开主页
        </button>
      </div>

      {result ? (
        <div className="note" style={{ marginTop: 6 }}>
          <select className="select" value={mirror} onChange={(event) => setMirror(event.target.value)}>
            {(info?.mirrors ?? []).map((m) => (
              <option key={m.id} value={m.id}>
                {m.label}
                {m.official ? "（直连）" : ""}
              </option>
            ))}
          </select>
          <span style={{ marginLeft: 8 }}>下载/检查时按「所选镜像 → 官方 → 其余镜像」逐个尝试。</span>
        </div>
      ) : null}

      {result && !result.available ? (
        <div className="kv" style={{ marginTop: 8 }}>
          <span>更新状态</span>
          <b>当前已是最新版本（{result.currentVersion}）</b>
        </div>
      ) : null}

      {showCandidate && candidate ? (
        <div style={{ marginTop: 8 }}>
          <div className="kv">
            <span>新版本</span>
            <b>{candidate.version}</b>
          </div>
          <div className="kv">
            <span>包大小</span>
            <b>{human(candidate.assetSize)}</b>
          </div>
          <div className="kv">
            <span>来源</span>
            <b>{candidate.sourceLabel}</b>
          </div>
          <div className="kv">
            <span>安装包</span>
            <b style={{ fontSize: 12 }}>{candidate.assetName}</b>
          </div>
          <div className="note">
            {result?.installModeLabel}
            {candidate.notes ? ` · ${candidate.notes.split("\n")[0].slice(0, 120)}` : ""}
          </div>
          <div style={{ display: "flex", gap: 6, flexWrap: "wrap", marginTop: 6 }}>
            <button className="btn" disabled={!!busy} onClick={() => void doDownload(true)}>
              {busy === "update" ? "更新中…" : "更新"}
            </button>
            <button className="btn" disabled={!!busy} onClick={() => void doDownload(false)}>
              {busy === "download" ? "下载中…" : "仅下载"}
            </button>
          </div>
        </div>
      ) : null}

      {progress && busy ? (
        <div style={{ marginTop: 8 }}>
          <div className="kv">
            <span>下载进度</span>
            <b>
              {progress.percent.toFixed(1)}% · {human(progress.received)} / {human(progress.total)}
            </b>
          </div>
          <div style={{ height: 6, background: "var(--border, #333)", borderRadius: 3, overflow: "hidden" }}>
            <div
              style={{
                height: "100%",
                width: `${Math.min(100, Math.max(0, progress.percent))}%`,
                background: "var(--accent, #4f8cff)",
                transition: "width .2s ease",
              }}
            />
          </div>
          <div className="note" style={{ marginTop: 4 }}>
            {progress.sourceLabel} · {progress.message}
          </div>
        </div>
      ) : null}

      {pending ? (
        <div style={{ marginTop: 10 }}>
          <div className="kv">
            <span>已下载更新</span>
            <b>{pending.version}</b>
          </div>
          <div className="kv">
            <span>安装包</span>
            <b style={{ fontSize: 12 }}>{pending.assetName}</b>
          </div>
          <div className="kv">
            <span>校验状态</span>
            <b>
              {pending.signatureVerified
                ? "大小 + SHA-256 + 签名通过"
                : pending.installable
                  ? "大小 + SHA-256 通过"
                  : "未通过签名校验"}
            </b>
          </div>
          <div className="note">{pending.path}</div>
          {pending.installBlockReason ? <div className="note">{pending.installBlockReason}</div> : null}
          <button
            className="btn"
            style={{ marginTop: 6 }}
            disabled={!!busy || !pending.installable}
            onClick={() => void doInstall()}
          >
            {busy === "install" ? "安装中…" : "安装"}
          </button>
        </div>
      ) : null}

      {error ? (
        <div className="note" style={{ marginTop: 8 }}>
          失败：{error}
        </div>
      ) : null}
      <Attempts attempts={attempts} />
      <div className="note" style={{ marginTop: 6 }}>
        {info?.signatureNote ?? "更新签名状态读取中…"}
      </div>
      {info ? (
        <div className="note">
          通道 {info.channel} · 当前 {info.currentVersion} · {info.platform}/{info.arch}
        </div>
      ) : null}
    </div>
  );
}
