import { useCallback, useEffect, useId, useRef, useState, type ReactNode } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import { api, type BatchRenameResult, type MediaInfo } from "../lib/api";
import { registerDropController, routeDrop } from "../lib/fileDropTarget";
import { moveBlock, sortEntries, type SortDir, type SortKey } from "../lib/fileQueue";
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

export interface MediaFilesController {
  /** 该队列在外部拖放路由表里的身份，供 FileTable 注册命中测试。 */
  dropId: string;
  files: MediaInfo[];
  busy: boolean;
  notice: string | null;
  /** 最初的添加顺序（恢复添加顺序用）。 */
  addedOrder: string[];
  /** 当前排序；null = 自定义顺序。 */
  sort: { key: SortKey; dir: SortDir } | null;
  /** 重复拖入的路径 + 触发令牌（供列表闪烁提示）。 */
  duplicatePaths: string[];
  duplicateToken: number;
  addPaths: (paths: string[], at?: number) => Promise<void>;
  pick: () => Promise<void>;
  remove: (path: string) => void;
  removeMany: (paths: string[]) => void;
  clear: () => void;
  move: (paths: string[], toIndex: number) => void;
  rename: (path: string, name: string) => Promise<{ ok: boolean; newPath?: string; error?: string }>;
  renameMany: (args: {
    paths: string[];
    template: string;
    start: number;
    pad: number;
    dryRun: boolean;
  }) => Promise<BatchRenameResult>;
  refresh: () => Promise<void>;
  sortBy: (key: SortKey, dir: SortDir) => void;
  restoreAddedOrder: () => void;
}

/**
 * 窗口级拖放桥：整个窗口只订阅一次系统拖放，再按坐标路由到命中的队列。
 * 之前每个 useMediaFiles 各订阅一次，一个面板挂多个列表时，一次拖放会把
 * 同一批文件同时塞进多个列表。
 */
let windowDropBridgeStarted = false;

function startWindowDropBridge() {
  if (windowDropBridgeStarted) return;
  windowDropBridgeStarted = true;
  void (async () => {
    try {
      const webview = getCurrentWebview();
      await webview.onDragDropEvent((event) => {
        const payload = event.payload as {
          type: string;
          paths?: string[];
          position?: { x: number; y: number };
        };
        if (payload.type !== "drop" || !payload.paths?.length) return;
        const ratio = window.devicePixelRatio || 1;
        const point =
          payload.position == null
            ? null
            : { x: payload.position.x / ratio, y: payload.position.y / ratio };
        void routeDrop(payload.paths, point);
      });
    } catch {
      // 订阅失败时允许下次挂载重试。
      windowDropBridgeStarted = false;
    }
  })();
}

export function useMediaFiles(kind: FileKind | FileKind[]): MediaFilesController {
  const kinds = Array.isArray(kind) ? kind : [kind];
  const dropId = useId();
  const [files, setFiles] = useState<MediaInfo[]>([]);
  const [busy, setBusy] = useState(false);
  const [notice, setNotice] = useState<string | null>(null);
  const [addedOrder, setAddedOrder] = useState<string[]>([]);
  const [sort, setSort] = useState<{ key: SortKey; dir: SortDir } | null>(null);
  const [duplicatePaths, setDuplicatePaths] = useState<string[]>([]);
  const [duplicateToken, setDuplicateToken] = useState(0);
  // 同步镜像：读取最新列表时不依赖上一次渲染的闭包。
  const filesRef = useRef<MediaInfo[]>([]);
  const addedRef = useRef<string[]>([]);

  const commitFiles = useCallback((next: MediaInfo[]) => {
    filesRef.current = next;
    setFiles(next);
  }, []);

  const commitAdded = useCallback((next: string[]) => {
    addedRef.current = next;
    setAddedOrder(next);
  }, []);

  const addPaths = useCallback(
    async (paths: string[], at?: number) => {
      if (!paths.length) return;
      setBusy(true);
      try {
        const infos = await api.probeFiles(paths);
        const seen = new Set(filesRef.current.map((f) => f.path));
        const fresh: MediaInfo[] = [];
        const duplicates: string[] = [];
        let skipped = 0;
        for (const info of infos) {
          if (!kinds.includes(info.kind as FileKind)) skipped += 1;
          else if (seen.has(info.path)) duplicates.push(info.path);
          else {
            seen.add(info.path);
            fresh.push(info);
          }
        }
        if (fresh.length) {
          const current = filesRef.current;
          const index = at == null ? current.length : Math.max(0, Math.min(current.length, at));
          commitFiles([...current.slice(0, index), ...fresh, ...current.slice(index)]);
          commitAdded([...addedRef.current, ...fresh.map((f) => f.path)]);
          setSort(null);
        }
        const notes: string[] = [];
        if (skipped > 0) notes.push(`已跳过 ${skipped} 个不受支持的文件`);
        if (duplicates.length) notes.push(`${duplicates.length} 个文件已在列表中`);
        setNotice(notes.length ? notes.join("；") : null);
        if (duplicates.length) {
          setDuplicatePaths(duplicates);
          setDuplicateToken((token) => token + 1);
        }
      } catch (error) {
        setNotice(`读取文件信息失败：${error instanceof Error ? error.message : String(error)}`);
      } finally {
        setBusy(false);
      }
    },
    // kinds 由调用方字面量传入，数组等价于同一组过滤条件
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [kinds.join(","), commitFiles, commitAdded],
  );

  const pick = useCallback(async () => {
    const filters = kinds.flatMap((k) => FILTERS[k]);
    const picked = await open({ multiple: true, directory: false, filters });
    if (!picked) return;
    const paths = Array.isArray(picked) ? picked : [picked];
    await addPaths(paths);
  }, [addPaths, kinds.join(",")]);

  const remove = useCallback(
    (path: string) => {
      commitFiles(filesRef.current.filter((f) => f.path !== path));
      commitAdded(addedRef.current.filter((p) => p !== path));
    },
    [commitFiles, commitAdded],
  );

  const removeMany = useCallback(
    (paths: string[]) => {
      const drop = new Set(paths);
      commitFiles(filesRef.current.filter((f) => !drop.has(f.path)));
      commitAdded(addedRef.current.filter((p) => !drop.has(p)));
    },
    [commitFiles, commitAdded],
  );

  const clear = useCallback(() => {
    commitFiles([]);
    commitAdded([]);
    setSort(null);
    setDuplicatePaths([]);
  }, [commitFiles, commitAdded]);

  /** 拖动排序：选中项整理成连续块整体插入。 */
  const move = useCallback(
    (paths: string[], toIndex: number) => {
      const current = filesRef.current;
      if (!current.length) return;
      const byPath = new Map(current.map((f) => [f.path, f]));
      const order = moveBlock(current.map((f) => f.path), new Set(paths), toIndex);
      commitFiles(order.map((path) => byPath.get(path)!).filter(Boolean));
      setSort(null);
    },
    [commitFiles],
  );

  /** 真改磁盘文件名：成功后把该行路径替换掉并重新探测；失败时列表保持原样。 */
  const rename = useCallback(
    async (path: string, name: string) => {
      try {
        const result = await api.renameSourceFile(path, name);
        let info: MediaInfo | undefined;
        try {
          info = (await api.probeFiles([result.to]))[0];
        } catch {
          // 探测失败不影响改名结果：先按新路径改名，元数据下轮刷新再补。
        }
        const fallback: MediaInfo | undefined = filesRef.current.find((f) => f.path === path);
        commitFiles(
          filesRef.current.map((f) =>
            f.path !== path
              ? f
              : (info ?? {
                  ...(fallback ?? f),
                  path: result.to,
                  name: result.name,
                  ext: result.name.split(".").pop()?.toLowerCase() ?? f.ext,
                }),
          ),
        );
        commitAdded(addedRef.current.map((p) => (p === path ? result.to : p)));
        return { ok: true, newPath: result.to };
      } catch (error) {
        return { ok: false, error: error instanceof Error ? error.message : String(error) };
      }
    },
    [commitFiles, commitAdded],
  );

  const renameMany = useCallback(
    async (args: { paths: string[]; template: string; start: number; pad: number; dryRun: boolean }) => {
      const result = await api.batchRenameSourceFiles(args);
      if (!result.applied) return result;
      const mapping = new Map(result.plans.map((plan) => [plan.from, plan.to]));
      let infos: MediaInfo[] = [];
      try {
        infos = await api.probeFiles(result.plans.map((plan) => plan.to));
      } catch {
        // 探测失败时保留旧元数据，只换路径与名字。
      }
      const byPath = new Map(infos.map((info) => [info.path, info]));
      commitFiles(
        filesRef.current.map((f) => {
          const next = mapping.get(f.path);
          if (!next) return f;
          const info = byPath.get(next);
          const name = next.split(/[\\/]/).pop() ?? f.name;
          return info ?? { ...f, path: next, name, ext: name.split(".").pop()?.toLowerCase() ?? f.ext };
        }),
      );
      commitAdded(addedRef.current.map((p) => mapping.get(p) ?? p));
      return result;
    },
    [commitFiles, commitAdded],
  );

  /** 刷新：重新探测当前列表里的所有源文件（只更新元数据，不改顺序）。 */
  const refresh = useCallback(async () => {
    const current = filesRef.current;
    if (!current.length) return;
    setBusy(true);
    try {
      const infos = await api.probeFiles(current.map((f) => f.path));
      const byPath = new Map(infos.map((info) => [info.path, info]));
      commitFiles(current.map((f) => byPath.get(f.path) ?? f));
      setNotice(null);
    } catch (error) {
      setNotice(`刷新失败：${error instanceof Error ? error.message : String(error)}`);
    } finally {
      setBusy(false);
    }
  }, [commitFiles]);

  const sortBy = useCallback(
    (key: SortKey, dir: SortDir) => {
      commitFiles(sortEntries(filesRef.current, key, dir));
      setSort({ key, dir });
    },
    [commitFiles],
  );

  const restoreAddedOrder = useCallback(() => {
    const rank = new Map(addedRef.current.map((path, index) => [path, index]));
    commitFiles(
      [...filesRef.current].sort(
        (a, b) => (rank.get(a.path) ?? Number.MAX_SAFE_INTEGER) - (rank.get(b.path) ?? Number.MAX_SAFE_INTEGER),
      ),
    );
    setSort(null);
  }, [commitFiles]);

  useEffect(() => {
    return registerDropController({ id: dropId, addPaths });
  }, [dropId, addPaths]);

  useEffect(() => {
    startWindowDropBridge();
  }, []);

  return {
    dropId,
    files,
    busy,
    notice,
    addedOrder,
    sort,
    duplicatePaths,
    duplicateToken,
    addPaths,
    pick,
    remove,
    removeMany,
    clear,
    move,
    rename,
    renameMany,
    refresh,
    sortBy,
    restoreAddedOrder,
  };
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

/** 列表视图实现见 components/FileTable；这里保留导出，面板调用点不用改。 */
export { FileTable } from "../components/FileTable";

/**
 * 输出位置：放在开始按钮上方，可就地更改。
 * 传 `"@source"` 表示与源文件同目录；此时隐藏「使用源文件夹」，
 * 自定义目录时给出一键切回源目录的入口（不需要重新手选）。
 */
export function OutputDirBar({
  outputDir,
  onChange,
}: {
  outputDir: string;
  onChange: (dir: string) => void;
}) {
  const isSource = !outputDir || outputDir === "@source";
  return (
    <div className="outdir">
      <Icon name="folder" size={14} />
      <span className="outdir-label">输出到</span>
      <code title={isSource ? "与源文件相同目录" : outputDir}>
        {isSource ? "与源文件相同目录" : outputDir}
      </code>
      <button
        className="btn small ghost"
        onClick={async () => {
          const picked = await open({ directory: true, multiple: false });
          if (typeof picked === "string") onChange(picked);
        }}
      >
        更改
      </button>
      {isSource ? null : (
        <button className="btn small ghost" onClick={() => onChange("@source")}>
          使用源文件夹
        </button>
      )}
    </div>
  );
}

/**
 * 参数分组。`collapsed` 用于「次要参数」：默认只露一行标题，
 * 点开才展开内容（原生 details，展开状态不需要额外状态管理）。
 */
export function Group({
  title,
  icon,
  children,
  collapsed,
}: {
  title: string;
  icon: string;
  children: ReactNode;
  collapsed?: boolean;
}) {
  const head = (
    <>
      <Icon name={icon} size={14} />
      {title}
    </>
  );
  if (collapsed) {
    return (
      <details className="group fold">
        <summary>
          {head}
          <Icon name="chevron-right" size={13} className="chev" />
        </summary>
        <div className="group-body">{children}</div>
      </details>
    );
  }
  return (
    <section className="group">
      <header>{head}</header>
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
