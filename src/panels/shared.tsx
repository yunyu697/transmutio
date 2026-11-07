import { useCallback, useEffect, useState, type ReactNode } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import { formatBytes, formatDuration } from "@formatforge/design-tokens";
import { api, type MediaInfo } from "../lib/api";
import { Icon } from "../components/Icon";
import {
  AUDIO_BITRATE_TIERS,
  CRF_MAX,
  CRF_MIN,
  CRF_TIERS,
  tierForCrf,
  tierForKbps,
  type BitrateTier,
} from "../lib/rateTiers";

export type FileKind = "video" | "audio" | "image";

const FILTERS: Record<FileKind, { name: string; extensions: string[] }[]> = {
  video: [{ name: "视频", extensions: ["mp4", "mkv", "mov", "avi", "webm", "flv", "wmv", "m4v", "ts", "mpg", "mpeg", "3gp"] }],
  audio: [{ name: "音频", extensions: ["mp3", "aac", "m4a", "wav", "flac", "ogg", "opus", "wma", "amr"] }],
  image: [{ name: "图片", extensions: ["jpg", "jpeg", "png", "webp", "bmp", "tif", "tiff", "gif", "avif"] }],
};

export function useMediaFiles(kind: FileKind | FileKind[]) {
  const kinds = Array.isArray(kind) ? kind : [kind];
  const [files, setFiles] = useState<MediaInfo[]>([]);
  const [busy, setBusy] = useState(false);
  const [notice, setNotice] = useState<string | null>(null);

  const addPaths = useCallback(
    async (paths: string[]) => {
      if (!paths.length) return;
      setBusy(true);
      try {
        const infos = await api.probeFiles(paths);
        const accepted: MediaInfo[] = [];
        let skipped = 0;
        for (const info of infos) {
          if (kinds.includes(info.kind as FileKind)) accepted.push(info);
          else skipped += 1;
        }
        setFiles((prev) => {
          const existing = new Set(prev.map((f) => f.path));
          return [...prev, ...accepted.filter((f) => !existing.has(f.path))];
        });
        if (skipped > 0) setNotice(`已跳过 ${skipped} 个不受支持的文件`);
        else setNotice(null);
      } catch (error) {
        setNotice(`读取文件信息失败：${error instanceof Error ? error.message : String(error)}`);
      } finally {
        setBusy(false);
      }
    },
    // kinds 由调用方字面量传入，数组等价于同一组过滤条件
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [kinds.join(",")],
  );

  const pick = useCallback(async () => {
    const filters = kinds.flatMap((k) => FILTERS[k]);
    const picked = await open({ multiple: true, directory: false, filters });
    if (!picked) return;
    const paths = Array.isArray(picked) ? picked : [picked];
    await addPaths(paths);
  }, [addPaths, kinds.join(",")]);

  const remove = useCallback((path: string) => {
    setFiles((prev) => prev.filter((f) => f.path !== path));
  }, []);

  const clear = useCallback(() => setFiles([]), []);

  useEffect(() => {
    let unlisten: (() => void) | undefined;
    let disposed = false;
    void (async () => {
      const webview = getCurrentWebview();
      const stop = await webview.onDragDropEvent((event) => {
        const payload = event.payload as { type: string; paths?: string[] };
        if (payload.type === "drop" && payload.paths?.length) {
          void addPaths(payload.paths);
        }
      });
      if (disposed) stop();
      else unlisten = stop;
    })();
    return () => {
      disposed = true;
      unlisten?.();
    };
  }, [addPaths]);

  return { files, busy, notice, addPaths, pick, remove, clear };
}

export function kindLabel(kind: FileKind | FileKind[]): string {
  const list = Array.isArray(kind) ? kind : [kind];
  return list.map((k) => (k === "video" ? "视频" : k === "audio" ? "音频" : "图片")).join(" / ");
}

export function DropZone({
  kind,
  busy,
  onPick,
}: {
  kind: FileKind | FileKind[];
  busy: boolean;
  onPick: () => void;
}) {
  return (
    <div className="dropzone">
      <Icon name="plus" size={16} />
      <span>
        拖入{kindLabel(kind)}文件，或
        <button className="btn small ghost" style={{ marginLeft: 6 }} onClick={onPick} disabled={busy}>
          {busy ? "读取中…" : "选择文件"}
        </button>
      </span>
      <span style={{ marginLeft: "auto", fontSize: 11.5, opacity: 0.7 }}>支持批量 · 不覆盖已有文件</span>
    </div>
  );
}

export function FileTable({
  files,
  onRemove,
  onClear,
}: {
  files: MediaInfo[];
  onRemove: (path: string) => void;
  onClear: () => void;
}) {
  if (!files.length) {
    return (
      <div className="filetable">
        <div className="empty">
          <div className="big">
            <Icon name="film" size={22} />
          </div>
          <div>拖入文件，或点击「添加文件」。</div>
          <div style={{ fontSize: 12, opacity: 0.8 }}>文件只读取信息，不会被改动</div>
        </div>
      </div>
    );
  }
  return (
    <div className="filetable">
      <table>
        <thead>
          <tr>
            <th>文件</th>
            <th>类型</th>
            <th>时长 / 尺寸</th>
            <th>大小</th>
            <th />
            <th style={{ width: 56 }} />
          </tr>
        </thead>
        <tbody>
          {files.map((file) => (
            <tr key={file.path}>
              <td className="name" title={file.path}>
                {file.name}
              </td>
              <td className="muted">{file.videoCodec || file.audioCodec || file.ext}</td>
              <td className="muted">
                {file.kind === "image"
                  ? `${file.width ?? "?"}×${file.height ?? "?"}`
                  : `${formatDuration(file.durationSec)}${file.width ? ` · ${file.width}×${file.height}` : ""}`}
              </td>
              <td className="muted">{formatBytes(file.sizeBytes)}</td>
              <td className={file.error ? "err" : "muted"} title={file.error ?? ""}>
                {file.error ? file.error.slice(0, 42) : ""}
              </td>
              <td>
                <button className="btn small ghost" onClick={() => onRemove(file.path)}>
                  移除
                </button>
              </td>
            </tr>
          ))}
        </tbody>
      </table>
      <div style={{ display: "flex", gap: 8, padding: "8px 10px", borderTop: "1px solid var(--ff-line)" }}>
        <span className="muted" style={{ fontSize: 12 }}>
          共 {files.length} 个文件
        </span>
        <button className="btn small ghost" style={{ marginLeft: "auto" }} onClick={onClear}>
          清空列表
        </button>
      </div>
    </div>
  );
}

export function Group({ title, icon, children }: { title: string; icon: string; children: ReactNode }) {
  return (
    <section className="group">
      <header>
        <Icon name={icon} size={14} />
        {title}
      </header>
      <div className="group-body">{children}</div>
    </section>
  );
}

export function Field({ label, tag, children }: { label: string; tag?: string; children: ReactNode }) {
  return (
    <div className="field">
      <label>
        {label}
        {tag ? <span className={tag === "硬编" ? "tag hw" : "tag"}>{tag}</span> : null}
      </label>
      {children}
    </div>
  );
}

export function Segmented<T extends string>({
  value,
  options,
  onChange,
}: {
  value: T;
  options: { value: T; label: string; disabled?: boolean; title?: string }[];
  onChange: (value: T) => void;
}) {
  return (
    <div className="segmented">
      {options.map((option) => (
        <button
          key={option.value}
          className={option.value === value ? "on" : ""}
          onClick={() => !option.disabled && onChange(option.value)}
          disabled={option.disabled}
          title={option.title}
          type="button"
        >
          {option.label}
        </button>
      ))}
    </div>
  );
}

/**
 * 恒定质量档位控件：默认只显示「档位名 + 一句人话说明」，
 * CRF 数字收进「高级」展开区。内部仍以 CRF 数值保存与传递。
 */
export function QualityTierField({
  crf,
  onChange,
  label = "画质档位",
}: {
  crf: number;
  onChange: (crf: number) => void;
  label?: string;
}) {
  const tier = tierForCrf(crf);
  return (
    <>
      <Field label={label}>
        <select
          className="select"
          value={tier.id}
          onChange={(event) => {
            const next = CRF_TIERS.find((item) => item.id === event.target.value);
            if (next) onChange(next.defaultCrf);
          }}
        >
          {CRF_TIERS.map((item) => (
            <option key={item.id} value={item.id}>
              {item.name}
            </option>
          ))}
        </select>
      </Field>
      <div className="note">{tier.hint}</div>
      <details className="advanced">
        <summary>高级：手动微调 CRF</summary>
        <div className="advanced-body">
          <input
            className="input"
            type="number"
            min={CRF_MIN}
            max={CRF_MAX}
            value={crf}
            onChange={(event) => {
              const next = Number(event.target.value);
              if (Number.isFinite(next) && next > 0) {
                onChange(Math.min(CRF_MAX, Math.max(CRF_MIN, Math.round(next))));
              }
            }}
          />
          <span className="note">
            当前档位「{tier.name}」对应 CRF {tier.min}–{tier.max}；数值越小越清晰、体积越大。
          </span>
        </div>
      </details>
    </>
  );
}

/** 自定义码率：直接填写 kbps（裸数字，不再套档位名）。 */
export function VideoBitrateField({
  kbps,
  onChange,
}: {
  kbps: number;
  onChange: (kbps: number) => void;
}) {
  return (
    <Field label="自定义码率（kbps）">
      <input
        className="input"
        type="number"
        min={300}
        step={100}
        value={kbps}
        onChange={(event) => onChange(Math.max(300, Number(event.target.value) || 0))}
      />
    </Field>
  );
}

export type AudioRateMode = "source" | "bitrate" | "";

const audioModeOf = (mode: AudioRateMode): AudioRateMode => (mode === "source" ? "source" : "bitrate");

/**
 * 音频码率控件：已删除「默认」选项，只剩两种语义——
 * 保持原始 = 用源音轨码率做目标，输出体积接近源；探测不到源码率时回落 128 kbps；
 * 固定档位 = 传选定 kbps（直接显示数字）。
 */
export function AudioBitrateField({
  mode,
  kbps,
  onChange,
  tiers = AUDIO_BITRATE_TIERS,
  allowSource = true,
}: {
  mode: AudioRateMode;
  kbps: number;
  onChange: (mode: AudioRateMode, kbps: number) => void;
  tiers?: BitrateTier[];
  /** 换轨等场景源码率不属于同一路流，可关闭「保持原始」。 */
  allowSource?: boolean;
}) {
  const current = allowSource ? audioModeOf(mode) : "bitrate";
  const tier = tierForKbps(tiers, kbps);
  const value = current === "source" ? "source" : (tier?.id ?? "custom");
  const hint =
    current === "source"
      ? "保持原始：沿用源文件的音频码率，输出体积接近源；读取不到源码率时按 128 kbps 编码。"
      : "固定档位：按所选码率编码，体积可预期。";
  return (
    <>
      <Field label="音频码率">
        <select
          className="select"
          value={value}
          onChange={(event) => {
            const next = event.target.value;
            if (next === "source") {
              onChange("source", kbps);
              return;
            }
            const picked = tiers.find((item) => item.id === next);
            if (picked) onChange("bitrate", picked.kbps);
          }}
        >
          {allowSource ? <option value="source">保持原始</option> : null}
          {tiers.map((item) => (
            <option key={item.id} value={item.id}>
              {item.name}
            </option>
          ))}
          {current === "bitrate" && !tier ? <option value="custom">自定义（{kbps} kbps）</option> : null}
        </select>
      </Field>
      <div className="note">{hint}</div>
    </>
  );
}

export function SwitchRow({
  label,
  hint,
  on,
  onChange,
  disabled,
}: {
  label: string;
  hint?: string;
  on: boolean;
  onChange: (value: boolean) => void;
  disabled?: boolean;
}) {
  return (
    <div className="switchrow">
      <div>
        <div style={{ fontSize: 13 }}>{label}</div>
        {hint ? <div className="hint">{hint}</div> : null}
      </div>
      <button
        type="button"
        className={`switch ${on ? "on" : ""}`}
        onClick={() => !disabled && onChange(!on)}
        disabled={disabled}
        aria-pressed={on}
        aria-label={label}
      />
    </div>
  );
}

export interface NamingPrefs {
  template: string;
  conflict: "suffix" | "skip" | "overwrite";
}

/// 输出命名模板 + 冲突策略。供所有面板复用，避免每个面板各写一份。
export function OutputNaming({
  naming,
  onChange,
}: {
  naming: NamingPrefs;
  onChange: (next: NamingPrefs) => void;
}) {
  return (
    <Group title="输出命名" icon="folder">
      <Field label="命名模板" tag="{name} {date} {index} {ext}">
        <input
          className="input"
          value={naming.template}
          placeholder="{name}"
          onChange={(e) => onChange({ ...naming, template: e.target.value })}
        />
      </Field>
      <Field label="同名冲突">
        <select
          className="select"
          value={naming.conflict}
          onChange={(e) => onChange({ ...naming, conflict: e.target.value as NamingPrefs["conflict"] })}
        >
          <option value="suffix">加序号（默认，安全）</option>
          <option value="skip">跳过（保留已有文件）</option>
          <option value="overwrite">覆盖（先备份旧文件）</option>
        </select>
      </Field>
      <div className="note">
        模板留空等于保持源文件名。占位符：{`{name}`} 源文件名、{`{date}`} 日期(YYYYMMDD)、{`{index}`} 批次内序号、{`{ext}`} 扩展名。
      </div>
      {naming.conflict === "overwrite" ? (
        <div className="note warn">覆盖是破坏性操作：会在替换前把旧文件挪到备份名，成功后删除备份。</div>
      ) : null}
    </Group>
  );
}
