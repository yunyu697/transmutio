import { useEffect, useMemo, useState } from "react";
import { formatBytes, formatDuration } from "@formatforge/design-tokens";
import { api, type ImageParams, type Job } from "../lib/api";
import { DropZone, Field, FileTable, Group, OutputNaming, Segmented, SwitchRow, useMediaFiles, type NamingPrefs } from "./shared";
import { loadPanelPrefs, savePanelPrefs } from "../lib/panelPrefs";
import { Icon } from "../components/Icon";

const DEFAULT_PARAMS: ImageParams = {
  format: "jpg",
  quality: 82,
  maxEdge: 0,
  rotation: "0",
  autoOrient: true,
  cropMode: "none",
  cropRatio: "1:1",
  cropX: 0,
  cropY: 0,
  cropW: 0,
  cropH: 0,
};

const EDGE_PRESETS = [
  { value: 0, label: "保持原尺寸" },
  { value: 3840, label: "3840" },
  { value: 1920, label: "1920" },
  { value: 1280, label: "1280" },
  { value: 800, label: "800" },
];

export function ImagePanel({
  methodId,
  outputDir,
  ffmpegReady,
  onChangeOutputDir,
  onEnqueued,
  toast,
  onBack,
}: {
  methodId: string;
  outputDir: string;
  ffmpegReady: boolean;
  onChangeOutputDir: (dir: string) => void;
  onEnqueued: (jobs: Job[]) => void;
  toast: (text: string, kind?: "ok" | "error" | "info") => void;
  onBack: () => void;
}) {
  const { files, busy, notice, pick, remove, clear } = useMediaFiles("image");
  const defaults: ImageParams = {
    ...DEFAULT_PARAMS,
    format: methodId === "image-resize" ? "png" : "jpg",
    cropMode: methodId === "image-crop" ? "ratio" : "none",
  };
  // 参数持久化：EXIF 自动校正、手动旋转、质量、尺寸等重启后恢复。
  const [params, setParams] = useState<ImageParams>(() => loadPanelPrefs(`image:${methodId}`, defaults));
  const [naming, setNaming] = useState<NamingPrefs>(() => loadPanelPrefs(`naming:${methodId}`, { template: "", conflict: "suffix" }));
  const [starting, setStarting] = useState(false);
  useEffect(() => savePanelPrefs(`image:${methodId}`, params), [methodId, params]);
  useEffect(() => savePanelPrefs(`naming:${methodId}`, naming), [methodId, naming]);
  const [customEdge, setCustomEdge] = useState("");

  const set = <K extends keyof ImageParams>(key: K, value: ImageParams[K]) =>
    setParams((prev) => ({ ...prev, [key]: value }));

  const totals = useMemo(
    () => ({
      count: files.length,
      bytes: files.reduce((sum, file) => sum + file.sizeBytes, 0),
      duration: files.reduce((sum, file) => sum + (file.durationSec ?? 0), 0),
    }),
    [files],
  );

  const qualityRelevant = params.format === "jpg" || params.format === "webp";

  const start = async () => {
    if (!files.length) {
      toast("请先添加至少一张图片", "error");
      return;
    }
    if (!ffmpegReady) {
      toast("未找到 ffmpeg，无法开始转换", "error");
      return;
    }
    setStarting(true);
    try {
      const specs = files
        .filter((file) => !file.error)
        .map((file) => ({
          kind: "image" as const,
          input: file.path,
          outputDir,
          params: { ...params },
          nameTemplate: naming.template.trim() || undefined,
          conflict: naming.conflict,
        }));
      if (!specs.length) {
        toast("列表中的文件都不可用（可能为空或损坏）", "error");
        return;
      }
      const jobs = await api.enqueue(specs);
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
          <button className="btn small ghost" onClick={onBack} title="返回方法列表">
            ← 方法列表
          </button>
          <div>
            <h1>{methodId === "image-resize" ? "图片批量缩放" : methodId === "image-crop" ? "图片批量裁剪" : "图片批量转换"}</h1>
            <p>等比缩放不拉伸 · 永不覆盖原图 · 输出目录与源目录相同时自动加序号</p>
          </div>
          <button className="btn" style={{ marginLeft: "auto" }} onClick={pick} disabled={busy}>
            <Icon name="plus" size={14} />
            {busy ? "读取中…" : "添加图片"}
          </button>
        </div>
        <DropZone kind="image" busy={busy} onPick={pick} />
        {notice ? (
          <div className="note warn" style={{ margin: "10px 20px 0" }}>
            {notice}
          </div>
        ) : null}
        <FileTable files={files} onRemove={remove} onClear={clear} />
      </div>

      <aside className="inspector">
        <div className="inspector-head">
          <Icon name="settings" size={15} />
          <b>输出参数</b>
          <span className="sub">{files.length} 张图片</span>
        </div>
        <div className="inspector-body">
          <Group title="目标格式" icon="image">
            <Field label="格式">
              <Segmented
                value={params.format}
                onChange={(value) => set("format", value)}
                options={[
                  { value: "jpg", label: "JPG" },
                  { value: "png", label: "PNG" },
                  { value: "webp", label: "WebP" },
                  { value: "bmp", label: "BMP" },
                  { value: "tiff", label: "TIFF" },
                ]}
              />
            </Field>
            {qualityRelevant ? (
              <Field label={`质量 ${params.quality}（仅 JPG / WebP 生效）`}>
                <input
                  className="slider"
                  type="range"
                  min={1}
                  max={100}
                  value={params.quality}
                  onChange={(event) => set("quality", Number(event.target.value))}
                />
              </Field>
            ) : (
              <div className="note">{params.format.toUpperCase()} 为无损 / 无损压缩格式，质量滑块不生效。</div>
            )}
          </Group>

          <Group title="裁剪" icon="crop">
            <Field label="模式" tag="裁剪在旋转与缩放之前">
              <Segmented
                value={params.cropMode}
                onChange={(value) => set("cropMode", value)}
                options={[
                  { value: "none", label: "不裁剪" },
                  { value: "ratio", label: "按比例" },
                  { value: "custom", label: "自定义矩形" },
                ]}
              />
            </Field>
            {params.cropMode === "ratio" ? (
              <Field label="比例（居中裁剪最大区域）">
                <Segmented
                  value={params.cropRatio}
                  onChange={(value) => set("cropRatio", value)}
                  options={[
                    { value: "1:1", label: "1:1" },
                    { value: "16:9", label: "16:9" },
                    { value: "9:16", label: "9:16" },
                    { value: "4:3", label: "4:3" },
                  ]}
                />
              </Field>
            ) : null}
            {params.cropMode === "custom" ? (
              <>
                <div className="row2">
                  <Field label="起点 X（px）">
                    <input
                      className="input"
                      type="number"
                      min={0}
                      step={1}
                      value={params.cropX}
                      onChange={(event) => set("cropX", Math.max(0, Math.round(Number(event.target.value))))}
                    />
                  </Field>
                  <Field label="起点 Y（px）">
                    <input
                      className="input"
                      type="number"
                      min={0}
                      step={1}
                      value={params.cropY}
                      onChange={(event) => set("cropY", Math.max(0, Math.round(Number(event.target.value))))}
                    />
                  </Field>
                </div>
                <div className="row2">
                  <Field label="宽（px）">
                    <input
                      className="input"
                      type="number"
                      min={1}
                      step={1}
                      value={params.cropW}
                      onChange={(event) => set("cropW", Math.max(0, Math.round(Number(event.target.value))))}
                    />
                  </Field>
                  <Field label="高（px）">
                    <input
                      className="input"
                      type="number"
                      min={1}
                      step={1}
                      value={params.cropH}
                      onChange={(event) => set("cropH", Math.max(0, Math.round(Number(event.target.value))))}
                    />
                  </Field>
                </div>
                <div className="note">
                  宽 / 高必须大于 0，且矩形必须完整落在源图内；越界会在入队阶段被拒绝并指出「矩形 @ 源图尺寸」，不会输出黑边。
                </div>
              </>
            ) : null}
          </Group>

          <Group title="尺寸" icon="resize">
            <Field label="最长边上限">
              <select
                className="select"
                value={EDGE_PRESETS.some((preset) => preset.value === params.maxEdge) ? params.maxEdge : -1}
                onChange={(event) => {
                  const value = Number(event.target.value);
                  if (value >= 0) set("maxEdge", value);
                }}
              >
                {EDGE_PRESETS.map((preset) => (
                  <option key={preset.value} value={preset.value}>
                    {preset.label}
                  </option>
                ))}
                {!EDGE_PRESETS.some((preset) => preset.value === params.maxEdge) ? (
                  <option value={-1}>{params.maxEdge} px（自定义）</option>
                ) : null}
              </select>
            </Field>
            <div className="row2">
              <Field label="自定义（px，0 = 原尺寸）">
                <input
                  className="input"
                  type="number"
                  min={0}
                  step={1}
                  placeholder={String(params.maxEdge)}
                  value={customEdge}
                  onChange={(event) => setCustomEdge(event.target.value)}
                  onBlur={() => {
                    const value = Number(customEdge);
                    if (Number.isFinite(value) && value >= 0) set("maxEdge", Math.round(value));
                    setCustomEdge("");
                  }}
                />
              </Field>
              <Field label="当前结果">
                <div className="note" style={{ height: 32, display: "flex", alignItems: "center" }}>
                  {params.maxEdge > 0 ? `最长边 ≤ ${params.maxEdge} px` : "保持原始尺寸"}
                </div>
              </Field>
            </div>
            <div className="note">
              缩放使用 <code>force_original_aspect_ratio=decrease</code>，只缩不放，不会拉伸；小于上限的图片保持原样。
            </div>
          </Group>

          <Group title="方向" icon="crop">
            <SwitchRow
              label="按 EXIF 自动校正"
              hint="读取照片的 EXIF Orientation；开启后会关闭 FFmpeg 的隐式 autorotate，保证只校正一次"
              on={params.autoOrient}
              onChange={(value) => set("autoOrient", value)}
            />
            <Field label="额外旋转" tag={params.autoOrient ? "叠加在自动校正之后" : "绝对旋转"}>
              <Segmented
                value={params.rotation}
                onChange={(value) => set("rotation", value)}
                options={[
                  { value: "0", label: "不旋转" },
                  { value: "90", label: "90°" },
                  { value: "180", label: "180°" },
                  { value: "270", label: "270°" },
                ]}
              />
            </Field>
            <div className="note">
              自动校正与手动旋转是两次独立的像素变换：EXIF 修正「照片原本该朝哪」，手动旋转是你在修正后的基础上再做调整。
              不会出现「FFmpeg 自动转一次 + 我们又转一次」的双重旋转。
            </div>
          </Group>

          <Group title="说明" icon="info">
            <div className="note">
              图片编码由 FFmpeg 完成：JPG 走 mjpeg、PNG 走内置 png、WebP 走 libwebp。图片编码器没有做逐项试编，
              若当前构建缺少某个编码器（例如没有 libwebp），任务会失败并在任务中心给出原因，不会生成损坏文件。
            </div>
          </Group>

          <OutputNaming naming={naming} onChange={setNaming} />
        </div>
      </aside>

      <div className="bottombar">
        <div className="outdir">
          <Icon name="folder" size={14} />
          <code title={outputDir}>{outputDir || "未设置输出目录"}</code>
          <button
            className="btn small ghost"
            onClick={async () => {
              const picked = await import("@tauri-apps/plugin-dialog").then((m) =>
                m.open({ directory: true, multiple: false }),
              );
              if (typeof picked === "string") onChangeOutputDir(picked);
            }}
          >
            更改
          </button>
        </div>
        <div style={{ fontSize: 10.5, color: "var(--ff-fg3)" }}>
          {totals.count ? (
            <>
              共 <b style={{ color: "var(--ff-fg2)" }}>{totals.count}</b> 张 · 源文件 {formatBytes(totals.bytes)}
              {totals.duration ? ` · 时长 ${formatDuration(totals.duration)}` : ""}
            </>
          ) : (
            "添加图片后显示统计"
          )}
        </div>
        <button className="btn primary" onClick={start} disabled={starting || !files.length}>
          <Icon name="play" size={14} />
          {starting ? "正在入队…" : "开始转换"}
        </button>
      </div>
    </div>
  );
}
