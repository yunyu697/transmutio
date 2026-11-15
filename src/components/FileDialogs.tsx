/**
 * 列表相关弹窗：属性/详细信息（单选取详情，多选取汇总）与批量重命名。
 * 只负责展示与输入；磁盘操作由调用方的 controller 完成。
 */
import { useEffect, useMemo, useState } from "react";
import { formatBytes, formatDuration } from "@formatforge/design-tokens";
import type { BatchRenameResult, MediaInfo } from "../lib/api";
import { Icon } from "./Icon";

const KIND_LABEL: Record<MediaInfo["kind"], string> = {
  video: "视频",
  audio: "音频",
  image: "图片",
  other: "其它",
};

function Row({ label, value }: { label: string; value: string }) {
  return (
    <div className="prop-row">
      <span className="prop-key">{label}</span>
      <span className="prop-value" title={value}>
        {value}
      </span>
    </div>
  );
}

export function FilePropertiesDialog({ files, onClose }: { files: MediaInfo[]; onClose: () => void }) {
  const total = files.reduce((sum, file) => sum + file.sizeBytes, 0);
  const single = files.length === 1 ? files[0] : null;
  return (
    <div className="ds-modal-backdrop" role="presentation" onMouseDown={(event) => event.target === event.currentTarget && onClose()}>
      <div className="ds-modal" role="dialog" aria-modal="true" aria-label="属性">
        <header>
          <Icon name="info" size={16} />
          <b>{single ? "文件属性" : `${files.length} 个文件的属性`}</b>
          <button className="btn small ghost ds-modal-close" onClick={onClose} aria-label="关闭" autoFocus>
            <Icon name="close" size={14} />
          </button>
        </header>
        <div className="ds-modal-body">
          {single ? (
            <>
              <Row label="文件名" value={single.name} />
              <Row label="位置" value={single.path} />
              <Row label="类型" value={`${KIND_LABEL[single.kind]} · ${single.ext || "未知格式"}`} />
              <Row label="大小" value={formatBytes(single.sizeBytes)} />
              {single.kind === "image" ? (
                <Row label="尺寸" value={`${single.width ?? "?"} × ${single.height ?? "?"}`} />
              ) : (
                <>
                  <Row label="时长" value={formatDuration(single.durationSec)} />
                  <Row
                    label="尺寸"
                    value={single.width ? `${single.width} × ${single.height}` : "—"}
                  />
                  <Row label="帧率" value={single.fps ? `${single.fps.toFixed(2)} fps` : "—"} />
                </>
              )}
              <Row
                label="编码"
                value={[single.videoCodec, single.audioCodec].filter(Boolean).join(" / ") || "—"}
              />
              <Row label="码率" value={single.bitrate ? `${Math.round(single.bitrate / 1000)} kbps` : "—"} />
              {single.kind !== "video" ? (
                <Row
                  label="采样"
                  value={
                    single.sampleRate ? `${single.sampleRate} Hz · ${single.channels ?? "?"} 声道` : "—"
                  }
                />
              ) : null}
              {single.error ? <Row label="读取错误" value={single.error} /> : null}
            </>
          ) : (
            <>
              <Row label="项目数" value={`${files.length} 个`} />
              <Row label="总大小" value={formatBytes(total)} />
              <Row
                label="类型分布"
                value={Object.entries(
                  files.reduce<Record<string, number>>((acc, file) => {
                    const key = KIND_LABEL[file.kind];
                    acc[key] = (acc[key] ?? 0) + 1;
                    return acc;
                  }, {}),
                )
                  .map(([key, count]) => `${key} ${count}`)
                  .join(" · ")}
              />
              <div className="prop-list" role="list">
                {files.map((file) => (
                  <div className="prop-list-row" role="listitem" key={file.path} title={file.path}>
                    <span>{file.name}</span>
                    <span className="muted">{formatBytes(file.sizeBytes)}</span>
                  </div>
                ))}
              </div>
            </>
          )}
        </div>
        <footer>
          <button className="btn small" onClick={onClose}>
            关闭
          </button>
        </footer>
      </div>
    </div>
  );
}

export function BulkRenameDialog({
  paths,
  onPreview,
  onApply,
  onClose,
}: {
  paths: string[];
  onPreview: (args: { paths: string[]; template: string; start: number; pad: number; dryRun: boolean }) => Promise<BatchRenameResult>;
  onApply: (args: { paths: string[]; template: string; start: number; pad: number; dryRun: boolean }) => Promise<BatchRenameResult>;
  onClose: () => void;
}) {
  const [template, setTemplate] = useState("{name}-{n}");
  const [start, setStart] = useState(1);
  const [pad, setPad] = useState(2);
  const [preview, setPreview] = useState<BatchRenameResult | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const conflicts = useMemo(
    () => (preview?.plans ?? []).filter((plan) => plan.conflict).length,
    [preview],
  );

  useEffect(() => {
    let cancelled = false;
    const timer = window.setTimeout(() => {
      setBusy(true);
      onPreview({ paths, template, start, pad, dryRun: true })
        .then((result) => {
          if (!cancelled) {
            setPreview(result);
            setError(null);
          }
        })
        .catch((reason) => {
          if (!cancelled) {
            setPreview(null);
            setError(reason instanceof Error ? reason.message : String(reason));
          }
        })
        .finally(() => {
          if (!cancelled) setBusy(false);
        });
    }, 220);
    return () => {
      cancelled = true;
      window.clearTimeout(timer);
    };
  }, [paths, template, start, pad, onPreview]);

  const apply = async () => {
    setBusy(true);
    try {
      const result = await onApply({ paths, template, start, pad, dryRun: false });
      if (result.applied) onClose();
      else {
        setPreview(result);
        setError("仍有冲突，未执行改名");
      }
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : String(reason));
    } finally {
      setBusy(false);
    }
  };

  return (
    <div className="ds-modal-backdrop" role="presentation" onMouseDown={(event) => event.target === event.currentTarget && onClose()}>
      <div className="ds-modal wide" role="dialog" aria-modal="true" aria-label="批量重命名">
        <header>
          <Icon name="pencil" size={16} />
          <b>批量重命名 {paths.length} 个文件</b>
          <button className="btn small ghost ds-modal-close" onClick={onClose} aria-label="关闭">
            <Icon name="close" size={14} />
          </button>
        </header>
        <div className="ds-modal-body">
          <div className="field">
            <label>命名模板</label>
            <input
              className="input"
              value={template}
              onChange={(event) => setTemplate(event.target.value)}
              placeholder="{name}-{n}"
              autoFocus
            />
            <div className="note">
              占位符：{`{name}`} 原文件名（不含扩展名）、{`{n}`} 序号；扩展名保持不变。
            </div>
          </div>
          <div className="row2">
            <div className="field">
              <label>起始序号</label>
              <input
                className="input"
                type="number"
                min={0}
                value={start}
                onChange={(event) => setStart(Math.max(0, Number(event.target.value) || 0))}
              />
            </div>
            <div className="field">
              <label>补零位数</label>
              <input
                className="input"
                type="number"
                min={0}
                max={6}
                value={pad}
                onChange={(event) => setPad(Math.max(0, Math.min(6, Number(event.target.value) || 0)))}
              />
            </div>
          </div>
          {error ? <div className="note danger">{error}</div> : null}
          {preview ? (
            <>
              <div className="note">
                {busy ? "正在预检…" : `预览 ${preview.plans.length} 项`}
                {conflicts ? ` · ${conflicts} 项冲突，修正模板后才能执行` : " · 无冲突"}
              </div>
              <div className="bulk-preview" role="list">
                {preview.plans.map((plan) => (
                  <div
                    className={`bulk-preview-row${plan.conflict ? " conflict" : ""}`}
                    role="listitem"
                    key={plan.from}
                  >
                    <span className="from" title={plan.from}>
                      {plan.from.split(/[\\/]/).pop()}
                    </span>
                    <Icon name="chevron-right" size={12} />
                    <span className="to">{plan.name}</span>
                    {plan.conflict ? <span className="why">{plan.conflict}</span> : null}
                  </div>
                ))}
              </div>
            </>
          ) : null}
        </div>
        <footer>
          <button className="btn small" onClick={onClose}>
            取消
          </button>
          <button
            className="btn small primary"
            onClick={apply}
            disabled={busy || !preview || conflicts > 0 || !template.trim()}
          >
            {busy ? "处理中…" : "执行改名"}
          </button>
        </footer>
      </div>
    </div>
  );
}
