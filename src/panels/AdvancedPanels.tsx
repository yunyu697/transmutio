import { useEffect, useMemo, useState } from "react";
import { formatBytes } from "@formatforge/design-tokens";
import { api, type AvifParams, type AvifResult, type CollageParams, type ExifFields, type ExifWriteResult, type FramesParams, type Job, type PdfToolsInfo, type SeqParams } from "../lib/api";
import { loadPanelPrefs, savePanelPrefs } from "../lib/panelPrefs";
import { DropZone, Field, FileTable, Group, OutputNaming, Segmented, SwitchRow, useMediaFiles, type NamingPrefs } from "./shared";
import { Icon } from "../components/Icon";

type CommonProps = {
  methodId: string;
  outputDir: string;
  ffmpegReady: boolean;
  onChangeOutputDir: (dir: string) => void;
  onEnqueued: (jobs: Job[]) => void;
  toast: (text: string, kind?: "ok" | "error" | "info") => void;
  onBack: () => void;
};

const chooseOutputDir = async (onChangeOutputDir: (dir: string) => void) => {
  const picked = await import("@tauri-apps/plugin-dialog").then((m) =>
    m.open({ directory: true, multiple: false }),
  );
  if (typeof picked === "string") onChangeOutputDir(picked);
};

function OutDirBar({
  outputDir,
  onChangeOutputDir,
}: {
  outputDir: string;
  onChangeOutputDir: (dir: string) => void;
}) {
  return (
    <div className="outdir">
      <Icon name="folder" size={14} />
      <code title={outputDir}>{outputDir || "未设置输出目录"}</code>
      <button className="btn small ghost" onClick={() => void chooseOutputDir(onChangeOutputDir)}>
        更改
      </button>
    </div>
  );
}

const DEFAULT_SEQ: SeqParams = {
  fps: 24,
  sizeMode: "source",
  width: 1920,
  height: 1080,
  encoder: "auto",
  quality: 23,
  bitrateKbps: 0,
};

const DEFAULT_FRAMES: FramesParams = { fps: 5, width: 0, format: "png", quality: 90 };

const DEFAULT_COLLAGE: CollageParams = {
  cols: 2,
  rows: 0,
  cellWidth: 800,
  cellHeight: 600,
  gap: 12,
  background: "#FFFFFF",
  fit: "contain",
  format: "png",
  quality: 90,
};

/** 图片序列 → 视频：帧率、尺寸统一策略、编码器（起始编号按文件名自动推导）。 */
export function SequencePanel({ outputDir, ffmpegReady, onChangeOutputDir, onEnqueued, toast, onBack }: CommonProps) {
  const { files, busy, notice, pick, remove, clear } = useMediaFiles("image");
  const [params, setParams] = useState<SeqParams>(() => loadPanelPrefs("seq:params", DEFAULT_SEQ));
  const [naming, setNaming] = useState<NamingPrefs>(() => loadPanelPrefs("seq:naming", { template: "", conflict: "suffix" }));
  const [starting, setStarting] = useState(false);
  useEffect(() => savePanelPrefs("seq:params", params), [params]);
  useEffect(() => savePanelPrefs("seq:naming", naming), [naming]);
  const set = <K extends keyof SeqParams>(key: K, value: SeqParams[K]) =>
    setParams((prev) => ({ ...prev, [key]: value }));

  const start = async () => {
    if (files.length < 1) return toast("请先添加图片序列", "error");
    if (!ffmpegReady) return toast("未找到 ffmpeg，无法开始转换", "error");
    setStarting(true);
    try {
      const paths = files.filter((file) => !file.error).map((file) => file.path);
      const jobs = await api.enqueue([
        {
          kind: "image-seq-video",
          input: paths[0],
          inputs: paths,
          outputDir,
          outputStem: files[0]?.name.replace(/\.\w+$/, "") + "-video",
          nameTemplate: naming.template.trim() || undefined,
          conflict: naming.conflict,
          params: { ...params },
        },
      ]);
      onEnqueued(jobs);
      toast(`已加入 ${jobs.length} 个任务`, "ok");
      clear();
    } catch (error) {
      toast(`入队失败：${error instanceof Error ? error.message : String(error)}`, "error");
    } finally {
      setStarting(false);
    }
  };

  return (
    <div className="workbench">
      <div className="wb-main">
        <div className="wb-head">
          <button className="btn small ghost" onClick={onBack}>← 方法列表</button>
          <div>
            <h1>图片序列转视频</h1>
            <p>文件名末尾必须带编号（如 frame_0001.png）；编号缺号会被明确拒绝，不会生成缺帧视频</p>
          </div>
          <button className="btn" style={{ marginLeft: "auto" }} onClick={pick} disabled={busy}>
            <Icon name="plus" size={14} />{busy ? "读取中…" : "添加图片"}
          </button>
        </div>
        <DropZone kind="image" busy={busy} onPick={pick} />
        {notice ? <div className="note warn" style={{ margin: "10px 20px 0" }}>{notice}</div> : null}
        <FileTable files={files} onRemove={remove} onClear={clear} />
      </div>
      <aside className="inspector">
        <div className="inspector-head"><Icon name="film" size={15} /><b>视频参数</b><span className="sub">{files.length} 张图片</span></div>
        <div className="inspector-body">
          <Group title="帧率与编号" icon="film">
            <Field label="帧率（fps）">
              <input className="input" type="number" min={1} max={60} value={params.fps}
                onChange={(event) => set("fps", Number(event.target.value))} />
            </Field>
            <div className="note">起始编号由所选文件名自动推导（首个文件的编号就是起始编号），不提供手动输入。若选择的文件编号不连续，任务会在入队阶段被拒绝并指出缺哪一张。</div>
          </Group>
          <Group title="尺寸统一" icon="resize">
            <Field label="策略">
              <Segmented
                value={params.sizeMode}
                onChange={(value) => set("sizeMode", value)}
                options={[
                  { value: "source", label: "要求同尺寸" },
                  { value: "first", label: "按首帧" },
                  { value: "max", label: "取最大" },
                  { value: "fixed", label: "自定义" },
                ]}
              />
            </Field>
            {params.sizeMode === "fixed" ? (
              <div className="row2">
                <Field label="宽">
                  <input className="input" type="number" min={1} value={params.width}
                    onChange={(event) => set("width", Number(event.target.value))} />
                </Field>
                <Field label="高">
                  <input className="input" type="number" min={1} value={params.height}
                    onChange={(event) => set("height", Number(event.target.value))} />
                </Field>
              </div>
            ) : null}
          </Group>
          <Group title="编码" icon="chip">
            <Field label="编码器">
              <Segmented
                value={params.encoder}
                onChange={(value) => set("encoder", value)}
                options={[
                  { value: "auto", label: "自动" },
                  { value: "libopenh264", label: "openH264" },
                  { value: "libx264", label: "x264" },
                ]}
              />
            </Field>
            <Field label={`画质（CRF/Q 值） ${params.quality}`}>
              <input className="slider" type="range" min={14} max={40} value={params.quality}
                onChange={(event) => set("quality", Number(event.target.value))} />
            </Field>
            <div className="note">「自动」会按当前 FFmpeg 实际可用的软件编码器选择：LGPL 包通常是 libopenh264，GPL 包优先 libx264。</div>
          </Group>
          <OutputNaming naming={naming} onChange={setNaming} />
        </div>
      </aside>
      <div className="bottombar">
        <OutDirBar outputDir={outputDir} onChangeOutputDir={onChangeOutputDir} />
        <div style={{ fontSize: 10.5, color: "var(--ff-fg3)" }}>
          {files.length ? `${files.length} 张 · ${formatBytes(files.reduce((sum, f) => sum + f.sizeBytes, 0))}` : "添加图片后显示统计"}
        </div>
        <button className="btn primary" onClick={start} disabled={starting || !files.length}>
          <Icon name="play" size={14} />{starting ? "正在入队…" : "开始生成"}
        </button>
      </div>
    </div>
  );
}

/** 视频 → 图片序列。 */
export function FramesPanel({ outputDir, ffmpegReady, onChangeOutputDir, onEnqueued, toast, onBack }: CommonProps) {
  const { files, busy, notice, pick, remove, clear } = useMediaFiles("video");
  const [params, setParams] = useState<FramesParams>(() => loadPanelPrefs("frames:params", DEFAULT_FRAMES));
  const [starting, setStarting] = useState(false);
  useEffect(() => savePanelPrefs("frames:params", params), [params]);
  const set = <K extends keyof FramesParams>(key: K, value: FramesParams[K]) =>
    setParams((prev) => ({ ...prev, [key]: value }));
  const video = files[0];

  const start = async () => {
    if (!video) return toast("请先添加一个视频", "error");
    if (!ffmpegReady) return toast("未找到 ffmpeg，无法开始转换", "error");
    setStarting(true);
    try {
      const jobs = await api.enqueue([
        {
          kind: "video-frames",
          input: video.path,
          outputDir,
          outputStem: video.name.replace(/\.\w+$/, ""),
          conflict: "suffix",
          params: { ...params },
        },
      ]);
      onEnqueued(jobs);
      toast(`已加入 ${jobs.length} 个任务`, "ok");
      clear();
    } catch (error) {
      toast(`入队失败：${error instanceof Error ? error.message : String(error)}`, "error");
    } finally {
      setStarting(false);
    }
  };

  return (
    <div className="workbench">
      <div className="wb-main">
        <div className="wb-head">
          <button className="btn small ghost" onClick={onBack}>← 方法列表</button>
          <div><h1>视频转图片序列</h1><p>按固定 fps 抽帧，输出 {video ? video.name.replace(/\.\w+$/, "") : "<源名>"}-0001.png 起</p></div>
          <button className="btn" style={{ marginLeft: "auto" }} onClick={pick} disabled={busy}>
            <Icon name="plus" size={14} />{busy ? "读取中…" : "添加视频"}
          </button>
        </div>
        <DropZone kind="video" busy={busy} onPick={pick} />
        {notice ? <div className="note warn" style={{ margin: "10px 20px 0" }}>{notice}</div> : null}
        <FileTable files={files.slice(0, 1)} onRemove={remove} onClear={clear} />
      </div>
      <aside className="inspector">
        <div className="inspector-head"><Icon name="camera" size={15} /><b>抽帧参数</b></div>
        <div className="inspector-body">
          <Group title="抽帧" icon="camera">
            <div className="row2">
              <Field label="每秒帧数">
                <input className="input" type="number" min={1} max={60} value={params.fps}
                  onChange={(event) => set("fps", Number(event.target.value))} />
              </Field>
              <Field label="宽度（0 = 原宽）">
                <input className="input" type="number" min={0} value={params.width}
                  onChange={(event) => set("width", Number(event.target.value))} />
              </Field>
            </div>
            <Field label="格式">
              <Segmented value={params.format} onChange={(value) => set("format", value)}
                options={[{ value: "png", label: "PNG" }, { value: "jpg", label: "JPG" }]} />
            </Field>
            {params.format === "jpg" ? (
              <Field label={`JPEG 质量 ${params.quality}`}>
                <input className="slider" type="range" min={1} max={100} value={params.quality}
                  onChange={(event) => set("quality", Number(event.target.value))} />
              </Field>
            ) : null}
            <div className="note">先写临时目录，全部成功后才移入输出目录；失败不会留下半套图片。</div>
          </Group>
        </div>
      </aside>
      <div className="bottombar">
        <OutDirBar outputDir={outputDir} onChangeOutputDir={onChangeOutputDir} />
        <div style={{ fontSize: 10.5, color: "var(--ff-fg3)" }}>{video ? `${video.width ?? "?"}x${video.height ?? "?"} · ${video.durationSec?.toFixed(1) ?? "?"}s` : "添加视频后显示信息"}</div>
        <button className="btn primary" onClick={start} disabled={starting || !video}>
          <Icon name="play" size={14} />{starting ? "正在入队…" : "开始抽帧"}
        </button>
      </div>
    </div>
  );
}

/** 图片拼图。 */
export function CollagePanel({ outputDir, ffmpegReady, onChangeOutputDir, onEnqueued, toast, onBack }: CommonProps) {
  const { files, busy, notice, pick, remove, clear } = useMediaFiles("image");
  const [params, setParams] = useState<CollageParams>(() => loadPanelPrefs("collage:params", DEFAULT_COLLAGE));
  const [naming, setNaming] = useState<NamingPrefs>(() => loadPanelPrefs("collage:naming", { template: "", conflict: "suffix" }));
  const [starting, setStarting] = useState(false);
  useEffect(() => savePanelPrefs("collage:params", params), [params]);
  useEffect(() => savePanelPrefs("collage:naming", naming), [naming]);
  const set = <K extends keyof CollageParams>(key: K, value: CollageParams[K]) =>
    setParams((prev) => ({ ...prev, [key]: value }));
  const rows = params.rows || Math.max(1, Math.ceil(files.length / Math.max(1, params.cols)));
  const canvas = useMemo(
    () => ({
      w: params.cols * params.cellWidth + Math.max(0, params.cols - 1) * params.gap,
      h: rows * params.cellHeight + Math.max(0, rows - 1) * params.gap,
    }),
    [params.cols, params.cellWidth, params.gap, params.cellHeight, rows],
  );

  const start = async () => {
    if (!files.length) return toast("请先添加图片", "error");
    if (!ffmpegReady) return toast("未找到 ffmpeg，无法开始转换", "error");
    if (files.length > params.cols * rows) {
      return toast(`当前网格 ${params.cols}x${rows} 最多放 ${params.cols * rows} 张，已超出`, "error");
    }
    setStarting(true);
    try {
      const paths = files.filter((file) => !file.error).map((file) => file.path);
      const jobs = await api.enqueue([
        {
          kind: "image-collage",
          input: paths[0],
          inputs: paths,
          outputDir,
          outputStem: "collage",
          nameTemplate: naming.template.trim() || undefined,
          conflict: naming.conflict,
          params: { ...params, rows: params.rows },
        },
      ]);
      onEnqueued(jobs);
      toast(`已加入 ${jobs.length} 个任务`, "ok");
    } catch (error) {
      toast(`入队失败：${error instanceof Error ? error.message : String(error)}`, "error");
    } finally {
      setStarting(false);
    }
  };

  return (
    <div className="workbench">
      <div className="wb-main">
        <div className="wb-head">
          <button className="btn small ghost" onClick={onBack}>← 方法列表</button>
          <div><h1>图片拼图</h1><p>网格布局 · 单元格等比适配 · 间距与底色可调</p></div>
          <button className="btn" style={{ marginLeft: "auto" }} onClick={pick} disabled={busy}>
            <Icon name="plus" size={14} />{busy ? "读取中…" : "添加图片"}
          </button>
        </div>
        <DropZone kind="image" busy={busy} onPick={pick} />
        {notice ? <div className="note warn" style={{ margin: "10px 20px 0" }}>{notice}</div> : null}
        <FileTable files={files} onRemove={remove} onClear={clear} />
      </div>
      <aside className="inspector">
        <div className="inspector-head"><Icon name="grid" size={15} /><b>网格参数</b><span className="sub">{canvas.w}x{canvas.h} px</span></div>
        <div className="inspector-body">
          <Group title="布局" icon="grid">
            <div className="row2">
              <Field label="列数（1..8）">
                <input className="input" type="number" min={1} max={8} value={params.cols}
                  onChange={(event) => set("cols", Number(event.target.value))} />
              </Field>
              <Field label="行数（0 = 自动）">
                <input className="input" type="number" min={0} max={8} value={params.rows}
                  onChange={(event) => set("rows", Number(event.target.value))} />
              </Field>
            </div>
            <div className="row2">
              <Field label="单元格宽">
                <input className="input" type="number" min={1} value={params.cellWidth}
                  onChange={(event) => set("cellWidth", Number(event.target.value))} />
              </Field>
              <Field label="单元格高">
                <input className="input" type="number" min={1} value={params.cellHeight}
                  onChange={(event) => set("cellHeight", Number(event.target.value))} />
              </Field>
            </div>
            <div className="row2">
              <Field label="间距（px）">
                <input className="input" type="number" min={0} max={200} value={params.gap}
                  onChange={(event) => set("gap", Number(event.target.value))} />
              </Field>
              <Field label="底色">
                <input className="input" type="text" value={params.background}
                  onChange={(event) => set("background", event.target.value)} placeholder="#FFFFFF" />
              </Field>
            </div>
            <Field label="适配方式">
              <Segmented value={params.fit} onChange={(value) => set("fit", value)}
                options={[{ value: "contain", label: "完整显示" }, { value: "cover", label: "裁剪填满" }]} />
            </Field>
          </Group>
          <Group title="输出" icon="image">
            <Field label="格式">
              <Segmented value={params.format} onChange={(value) => set("format", value)}
                options={[{ value: "png", label: "PNG" }, { value: "jpg", label: "JPG" }, { value: "webp", label: "WebP" }]} />
            </Field>
            {params.format !== "png" ? (
              <Field label={`质量 ${params.quality}`}>
                <input className="slider" type="range" min={1} max={100} value={params.quality}
                  onChange={(event) => set("quality", Number(event.target.value))} />
              </Field>
            ) : null}
            <div className="note">画布 = 列数×单元格宽 + 间距；超出网格容量的图片会在入队阶段被拒绝。</div>
          </Group>
          <OutputNaming naming={naming} onChange={setNaming} />
        </div>
      </aside>
      <div className="bottombar">
        <OutDirBar outputDir={outputDir} onChangeOutputDir={onChangeOutputDir} />
        <div style={{ fontSize: 10.5, color: "var(--ff-fg3)" }}>{files.length} 张 · 画布 {canvas.w}x{canvas.h}</div>
        <button className="btn primary" onClick={start} disabled={starting || !files.length}>
          <Icon name="play" size={14} />{starting ? "正在入队…" : "生成拼图"}
        </button>
      </div>
    </div>
  );
}

/** PDF 合并 / 拆分 / 转图片。 */
export function PdfPanel({
  methodId,
  outputDir,
  onChangeOutputDir,
  onEnqueued,
  toast,
  onBack,
  pdfTools,
  onRedetectPdfTools,
  onDownloadPoppler,
}: CommonProps & {
  pdfTools: PdfToolsInfo | null;
  onRedetectPdfTools?: () => Promise<PdfToolsInfo | null>;
  onDownloadPoppler?: () => Promise<void>;
}) {
  const [paths, setPaths] = useState<string[]>([]);
  const [mode, setMode] = useState<"per-page" | "ranges">(
    () => loadPanelPrefs<{ mode: "per-page" | "ranges" }>("pdf:mode", { mode: "per-page" }).mode,
  );
  const [ranges, setRanges] = useState<string>(() => loadPanelPrefs("pdf:ranges", { ranges: "1-3" }).ranges);
  const [format, setFormat] = useState<"png" | "jpg">(
    () => loadPanelPrefs<{ format: "png" | "jpg" }>("pdf:format", { format: "png" }).format,
  );
  const [dpi, setDpi] = useState<number>(() => loadPanelPrefs("pdf:dpi", { dpi: 96 }).dpi);
  const [naming, setNaming] = useState<NamingPrefs>(() => loadPanelPrefs("pdf:naming", { template: "", conflict: "suffix" }));
  const [starting, setStarting] = useState(false);
  const [redetecting, setRedetecting] = useState(false);

  useEffect(() => savePanelPrefs("pdf:mode", { mode }), [mode]);
  useEffect(() => savePanelPrefs("pdf:ranges", { ranges }), [ranges]);
  useEffect(() => savePanelPrefs("pdf:format", { format }), [format]);
  useEffect(() => savePanelPrefs("pdf:dpi", { dpi }), [dpi]);
  useEffect(() => savePanelPrefs("pdf:naming", naming), [naming]);

  const isMerge = methodId === "pdf-merge";
  const isSplit = methodId === "pdf-split";
  const isImage = methodId === "pdf-image";
  const toolMissing = isImage && pdfTools && !pdfTools.available;

  const [downloadPopplerBusy, setDownloadPopplerBusy] = useState(false);

  const downloadingPoppler = async () => {
    if (!onDownloadPoppler) return;
    setDownloadPopplerBusy(true);
    try {
      await onDownloadPoppler();
    } finally {
      setDownloadPopplerBusy(false);
    }
  };

  const redetect = async () => {
    if (!onRedetectPdfTools) return;
    setRedetecting(true);
    try {
      const next = await onRedetectPdfTools();
      if (next?.available) toast("已重新检测到 pdftoppm，现在可以转换了。", "ok");
      else toast(next?.reason ?? "仍未检测到 pdftoppm（Poppler），请按上方说明配置后重试。", "error");
    } catch (error) {
      toast(`重新检测失败：${error instanceof Error ? error.message : String(error)}`, "error");
    } finally {
      setRedetecting(false);
    }
  };

  const pick = async () => {
    const picked = await import("@tauri-apps/plugin-dialog").then((m) =>
      m.open({
        multiple: isMerge,
        directory: false,
        filters: [{ name: "PDF", extensions: ["pdf"] }],
      }),
    );
    if (!picked) return;
    const list: string[] = Array.isArray(picked) ? picked : [picked as string];
    setPaths(isMerge ? list : list.slice(0, 1));
  };

  const start = async () => {
    if (!paths.length) return toast("请先添加 PDF 文件", "error");
    if (isMerge && paths.length < 2) return toast("PDF 合并至少需要 2 个文件", "error");
    if (toolMissing) return toast(pdfTools?.reason ?? "缺少 pdftoppm", "error");
    if (isSplit && mode === "ranges") {
      const valid = /^\d+(-\d+)?(,\d+(-\d+)?)*$/.test(ranges.trim());
      if (!valid) return toast("页码范围格式非法，例如 1-3,5", "error");
    }
    setStarting(true);
    try {
      const kind = isMerge ? "pdf-merge" : isSplit ? "pdf-split" : "pdf-image";
      const params = isMerge
        ? {}
        : isSplit
          ? { mode, ranges: mode === "ranges" ? ranges.trim() : undefined }
          : { format, dpi };
      const jobs = await api.enqueue([
        {
          kind,
          input: paths[0],
          inputs: paths,
          outputDir,
          outputStem: paths[0]?.split(/[\\/]/).pop()?.replace(/\.pdf$/i, ""),
          nameTemplate: naming.template.trim() || undefined,
          conflict: naming.conflict,
          params,
        },
      ]);
      onEnqueued(jobs);
      toast(`已加入 ${jobs.length} 个任务`, "ok");
      setPaths([]);
    } catch (error) {
      toast(`入队失败：${error instanceof Error ? error.message : String(error)}`, "error");
    } finally {
      setStarting(false);
    }
  };

  return (
    <div className="workbench">
      <div className="wb-main">
        <div className="wb-head">
          <button className="btn small ghost" onClick={onBack}>← 方法列表</button>
          <div>
            <h1>{isMerge ? "PDF 合并" : isSplit ? "PDF 拆分" : "PDF 转图片"}</h1>
            <p>{isMerge ? "多份 PDF 按顺序拼接（本地处理，不依赖外部工具）" : isSplit ? "每页一个文件或指定页码范围" : "PNG / JPG，可指定 DPI（需要 pdftoppm）"}</p>
          </div>
          <button className="btn" style={{ marginLeft: "auto" }} onClick={pick}>
            <Icon name="plus" size={14} />添加 PDF
          </button>
        </div>
        {toolMissing ? (
          <div className="note warn" style={{ margin: "10px 20px 0" }}>
            <b>未检测到 pdftoppm（Poppler 套件）：</b>PDF 转图片暂时无法执行，合并 / 拆分不受影响。<br />
            配置方式（任选其一）：
            <ul style={{ margin: "6px 0 0 18px", padding: 0 }}>
              <li>在设置页的「PDF 工具目录」中填写 Poppler 的 <code>Library\bin</code> 目录（该目录需包含 <code>pdftoppm.exe</code>）。</li>
              <li>或设置环境变量 <code>PDFTOPPM_PATH=...\pdftoppm.exe</code>。</li>
            </ul>
            查找顺序：<code>PDFTOPPM_PATH</code> → 设置目录 → <code>TRANSMUTIO_PDF_TOOLS</code> → PATH。<br />
            {pdfTools?.reason ? <>检测详情：{pdfTools.reason}<br /></> : null}
            <button
              className="btn small"
              style={{ marginTop: 8 }}
              onClick={() => void redetect()}
              disabled={redetecting || !onRedetectPdfTools}
            >
              {redetecting ? "检测中…" : "重新检测"}
            </button>
            {onDownloadPoppler ? (
              <button
                className="btn small"
                style={{ marginTop: 8, marginLeft: 8 }}
                onClick={() => void downloadingPoppler()}
                disabled={downloadPopplerBusy}
              >
                {downloadPopplerBusy ? "下载中…" : "自动下载 Poppler（按内置 SHA-256 校验）"}
              </button>
            ) : null}
          </div>
        ) : null}
        <div style={{ padding: "14px 20px" }}>
          {paths.length ? (
            <div className="file-table">
              {paths.map((path, index) => (
                <div key={path} className="file-row" style={{ display: "flex", gap: 8, alignItems: "center", padding: "6px 0" }}>
                  <span className="muted" style={{ width: 24 }}>{index + 1}</span>
                  <code style={{ flex: 1, overflow: "hidden", textOverflow: "ellipsis" }} title={path}>{path.split(/[\\/]/).pop()}</code>
                  <button className="btn small ghost" onClick={() => setPaths((prev) => prev.filter((p) => p !== path))}>
                    移除
                  </button>
                </div>
              ))}
            </div>
          ) : (
            <div className="empty" style={{ padding: "30px 0" }}>尚未添加 PDF</div>
          )}
        </div>
      </div>
      <aside className="inspector">
        <div className="inspector-head"><Icon name="pdf" size={15} /><b>参数</b><span className="sub">{paths.length} 个文件</span></div>
        <div className="inspector-body">
          {isSplit ? (
            <Group title="拆分方式" icon="split">
              <Field label="模式">
                <Segmented value={mode} onChange={(value) => setMode(value)}
                  options={[{ value: "per-page", label: "每页一个文件" }, { value: "ranges", label: "指定页码范围" }]} />
              </Field>
              {mode === "ranges" ? (
                <Field label="页码范围">
                  <input className="input" type="text" value={ranges} onChange={(event) => setRanges(event.target.value)} placeholder="1-3,5,8-10" />
                </Field>
              ) : null}
              <div className="note">非法页码（0、倒序、超出总页数、重复）会明确报错，不会静默跳过。</div>
            </Group>
          ) : null}
          {isImage ? (
            <Group title="图片输出" icon="image">
              <Field label="格式">
                <Segmented value={format} onChange={(value) => setFormat(value)}
                  options={[{ value: "png", label: "PNG" }, { value: "jpg", label: "JPG" }]} />
              </Field>
              <Field label="DPI（36..600）">
                <input className="input" type="number" min={36} max={600} value={dpi} onChange={(event) => setDpi(Number(event.target.value))} />
              </Field>
              <div className="note">使用外部 pdftoppm 渲染；未安装时仍可进入本面板，按上方说明配置后点「重新检测」即可。</div>
            </Group>
          ) : null}
          <Group title="实现方式" icon="info">
            <div className="note">
              {isMerge || isSplit
                ? "合并 / 拆分使用内置 lopdf，在进程内完成，不需要装 qpdf 或 Ghostscript。"
                : "PDF 栅格化没有可靠的纯 Rust 方案，因此调用 Poppler 的 pdftoppm；查找顺序为 PDFTOPPM_PATH → 设置目录 → TRANSMUTIO_PDF_TOOLS → PATH。"}
            </div>
          </Group>
          <OutputNaming naming={naming} onChange={setNaming} />
        </div>
      </aside>
      <div className="bottombar">
        <OutDirBar outputDir={outputDir} onChangeOutputDir={onChangeOutputDir} />
        <div style={{ fontSize: 10.5, color: "var(--ff-fg3)" }}>{paths.length ? `${paths.length} 个 PDF` : "添加文件后显示统计"}</div>
        <button className="btn primary" onClick={start} disabled={starting || !paths.length || Boolean(toolMissing)}>
          <Icon name="play" size={14} />{starting ? "正在入队…" : isMerge ? "开始合并" : isSplit ? "开始拆分" : "开始转换"}
        </button>
      </div>
    </div>
  );
}

/** EXIF 元数据写入（只写副本，源文件不动）。 */
const DEFAULT_EXIF: ExifForm = {
  description: "",
  artist: "",
  copyright: "",
  software: "",
  dateLocal: "",
  make: "",
  model: "",
  lensModel: "",
  orientation: "0",
};

type ExifForm = {
  description: string;
  artist: string;
  copyright: string;
  software: string;
  dateLocal: string;
  make: string;
  model: string;
  lensModel: string;
  orientation: string;
};

/** datetime-local（2026-11-06T12:00）→ EXIF 固定格式（2026:11:06 12:00:00）。 */
const toExifDate = (local: string): string | undefined => {
  const m = local.match(/^(\d{4})-(\d{2})-(\d{2})T(\d{2}):(\d{2})/);
  if (!m) return undefined;
  return `${m[1]}:${m[2]}:${m[3]} ${m[4]}:${m[5]}:00`;
};

export function ExifPanel({ outputDir, onChangeOutputDir, toast, onBack }: CommonProps) {
  const { files, busy, notice, pick, remove, clear } = useMediaFiles("image");
  const [form, setForm] = useState<ExifForm>(() => loadPanelPrefs("exif:form", DEFAULT_EXIF));
  const [running, setRunning] = useState(false);
  const [results, setResults] = useState<ExifWriteResult[] | null>(null);
  useEffect(() => savePanelPrefs("exif:form", form), [form]);
  const set = <K extends keyof ExifForm>(key: K, value: ExifForm[K]) =>
    setForm((prev) => ({ ...prev, [key]: value }));

  const start = async () => {
    if (!files.length) return toast("请先添加图片", "error");
    const paths = files.filter((file) => !file.error).map((file) => file.path);
    const fields: ExifFields = {
      description: form.description.trim() || undefined,
      artist: form.artist.trim() || undefined,
      copyright: form.copyright.trim() || undefined,
      software: form.software.trim() || undefined,
      dateTimeOriginal: toExifDate(form.dateLocal),
      make: form.make.trim() || undefined,
      model: form.model.trim() || undefined,
      lensModel: form.lensModel.trim() || undefined,
      orientation: form.orientation === "0" ? undefined : Number(form.orientation),
    };
    setRunning(true);
    setResults(null);
    try {
      const written = await api.writeExif(paths, fields, outputDir.trim() || undefined);
      setResults(written);
      const ok = written.filter((item) => item.ok).length;
      toast(`写入完成：${ok}/${written.length} 成功`, ok === written.length ? "ok" : "error");
    } catch (error) {
      toast(`写入失败：${error instanceof Error ? error.message : String(error)}`, "error");
    } finally {
      setRunning(false);
    }
  };

  const orientationOptions = [
    { value: "0", label: "不写入" },
    { value: "1", label: "1 正常" },
    { value: "3", label: "3 180°" },
    { value: "6", label: "6 顺时针 90°" },
    { value: "8", label: "8 逆时针 90°" },
  ];

  return (
    <div className="workbench">
      <div className="wb-main">
        <div className="wb-head">
          <button className="btn small ghost" onClick={onBack}>← 方法列表</button>
          <div><h1>EXIF 元数据写入</h1><p>只写副本 · 源文件不动 · 写完回读校验</p></div>
          <button className="btn" style={{ marginLeft: "auto" }} onClick={pick} disabled={busy}>
            <Icon name="plus" size={14} />{busy ? "读取中…" : "添加图片"}
          </button>
        </div>
        <DropZone kind="image" busy={busy} onPick={pick} />
        {notice ? <div className="note warn" style={{ margin: "10px 20px 0" }}>{notice}</div> : null}
        <FileTable files={files} onRemove={remove} onClear={clear} />
        {results ? (
          <div style={{ padding: "0 20px 16px" }}>
            {results.map((item) => (
              <div key={item.src} className={`note ${item.ok ? "" : "warn"}`} style={{ marginTop: 8 }}>
                <b>{item.ok ? "OK" : "FAIL"}</b> · {item.src}
                {item.output ? <div className="sub">→ {item.output}</div> : null}
                {item.message ? <div className="sub">{item.message}</div> : null}
              </div>
            ))}
          </div>
        ) : null}
      </div>
      <aside className="inspector">
        <div className="inspector-head"><Icon name="info" size={15} /><b>元数据字段</b><span className="sub">空白字段不写入</span></div>
        <div className="inspector-body">
          <Group title="基本信息" icon="info">
            <Field label="描述">
              <input className="input" value={form.description} onChange={(event) => set("description", event.target.value)} placeholder="例如：现场照片" />
            </Field>
            <Field label="作者">
              <input className="input" value={form.artist} onChange={(event) => set("artist", event.target.value)} placeholder="例如：Tyler" />
            </Field>
            <Field label="版权">
              <input className="input" value={form.copyright} onChange={(event) => set("copyright", event.target.value)} placeholder="例如：© 2026 Transmutio" />
            </Field>
            <Field label="软件">
              <input className="input" value={form.software} onChange={(event) => set("software", event.target.value)} placeholder="例如：Transmutio 1.0.0" />
            </Field>
            <Field label="拍摄时间">
              <input className="input" type="datetime-local" value={form.dateLocal} onChange={(event) => set("dateLocal", event.target.value)} />
            </Field>
          </Group>
          <Group title="相机与方向" icon="image">
            <div className="row2">
              <Field label="厂商">
                <input className="input" value={form.make} onChange={(event) => set("make", event.target.value)} />
              </Field>
              <Field label="型号">
                <input className="input" value={form.model} onChange={(event) => set("model", event.target.value)} />
              </Field>
            </div>
            <Field label="镜头">
              <input className="input" value={form.lensModel} onChange={(event) => set("lensModel", event.target.value)} />
            </Field>
            <Field label="方向（EXIF Orientation）">
              <Segmented value={form.orientation} onChange={(value) => set("orientation", value)} options={orientationOptions} />
            </Field>
          </Group>
          <Group title="写在哪里" icon="folder">
            <div className="note">
              输出目录留空 = 与源文件同目录，同名冲突自动加 (1)、(2)。源文件不会被修改；
              失败时删除中间文件，绝不留下半套输出。支持 JPG / PNG / TIFF / WebP（仅无损或扩展格式）/ HEIC / AVIF / JXL。
            </div>
          </Group>
        </div>
      </aside>
      <div className="bottombar">
        <OutDirBar outputDir={outputDir} onChangeOutputDir={onChangeOutputDir} />
        <div style={{ fontSize: 10.5, color: "var(--ff-fg3)" }}>{files.length} 张 · 只写副本</div>
        <button className="btn primary" onClick={start} disabled={running || !files.length}>
          <Icon name="play" size={14} />{running ? "正在写入…" : "写入 EXIF"}
        </button>
      </div>
    </div>
  );
}

const DEFAULT_AVIF: AvifParams = { quality: 70, speed: 6 };

/** AVIF 编码（纯 Rust ravif，不依赖 FFmpeg 的 avif muxer）。 */
export function AvifPanel({ outputDir, onChangeOutputDir, toast, onBack }: CommonProps) {
  const { files, busy, notice, pick, remove, clear } = useMediaFiles("image");
  const [prefs, setPrefs] = useState<AvifParams>(() => loadPanelPrefs("avif:prefs", DEFAULT_AVIF));
  const [running, setRunning] = useState(false);
  const [results, setResults] = useState<AvifResult[] | null>(null);
  useEffect(() => savePanelPrefs("avif:prefs", prefs), [prefs]);

  const start = async () => {
    if (!files.length) return toast("请先添加图片", "error");
    const paths = files.filter((file) => !file.error).map((file) => file.path);
    setRunning(true);
    setResults(null);
    try {
      const encoded = await api.encodeAvif(paths, prefs, outputDir.trim() || undefined);
      setResults(encoded);
      const ok = encoded.filter((item) => item.ok).length;
      toast(`编码完成：${ok}/${encoded.length} 成功`, ok === encoded.length ? "ok" : "error");
    } catch (error) {
      toast(`编码失败：${error instanceof Error ? error.message : String(error)}`, "error");
    } finally {
      setRunning(false);
    }
  };

  return (
    <div className="workbench">
      <div className="wb-main">
        <div className="wb-head">
          <button className="btn small ghost" onClick={onBack}>← 方法列表</button>
          <div><h1>图片转 AVIF</h1><p>纯 Rust 编码 · 只写副本 · 输出校验 ftyp/avif 签名</p></div>
          <button className="btn" style={{ marginLeft: "auto" }} onClick={pick} disabled={busy}>
            <Icon name="plus" size={14} />{busy ? "读取中…" : "添加图片"}
          </button>
        </div>
        <DropZone kind="image" busy={busy} onPick={pick} />
        {notice ? <div className="note warn" style={{ margin: "10px 20px 0" }}>{notice}</div> : null}
        <FileTable files={files} onRemove={remove} onClear={clear} />
        {results ? (
          <div style={{ padding: "0 20px 16px" }}>
            {results.map((item) => (
              <div key={item.src} className={`note ${item.ok ? "" : "warn"}`} style={{ marginTop: 8 }}>
                <b>{item.ok ? "OK" : "FAIL"}</b> · {item.src}
                {item.ok ? <div className="sub">{item.width}×{item.height} · {formatBytes(item.bytes)}</div> : null}
                {item.output ? <div className="sub">→ {item.output}</div> : null}
                {item.message ? <div className="sub">{item.message}</div> : null}
              </div>
            ))}
          </div>
        ) : null}
      </div>
      <aside className="inspector">
        <div className="inspector-head"><Icon name="image" size={15} /><b>编码参数</b><span className="sub">速度越大越快、体积更大</span></div>
        <div className="inspector-body">
          <Group title="质量" icon="sparkle">
            <Field label={`质量 ${prefs.quality}`}>
              <input
                className="input"
                type="range"
                min={1}
                max={100}
                value={prefs.quality}
                onChange={(event) => setPrefs((prev) => ({ ...prev, quality: Number(event.target.value) }))}
              />
            </Field>
            <div className="note">1–100，越大越接近原图；70 左右适合网页图片。</div>
          </Group>
          <Group title="速度" icon="speed">
            <Field label={`速度 ${prefs.speed}`}>
              <input
                className="input"
                type="range"
                min={1}
                max={10}
                value={prefs.speed}
                onChange={(event) => setPrefs((prev) => ({ ...prev, speed: Number(event.target.value) }))}
              />
            </Field>
            <div className="note">1 最慢但压缩率最高，10 最快；批量大图建议 8。</div>
          </Group>
          <Group title="说明" icon="info">
            <div className="note">
              FFmpeg 的 AV1 编码器存在，但没有 AVIF 封装器（命令行没有对应的输出格式），因此这里用
              ravif（纯 Rust）直接编码，不依赖外部二进制。输入支持 JPG / PNG / WebP / BMP / TIFF / GIF。
            </div>
          </Group>
        </div>
      </aside>
      <div className="bottombar">
        <OutDirBar outputDir={outputDir} onChangeOutputDir={onChangeOutputDir} />
        <div style={{ fontSize: 10.5, color: "var(--ff-fg3)" }}>{files.length} 张 · 只写副本</div>
        <button className="btn primary" onClick={start} disabled={running || !files.length}>
          <Icon name="play" size={14} />{running ? "正在编码…" : "编码 AVIF"}
        </button>
      </div>
    </div>
  );
}
