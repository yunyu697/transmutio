import { useEffect, useMemo, useState } from "react";
import { formatBytes, formatDuration } from "@formatforge/design-tokens";
import { api, type CompressParams, type ImageGifParams, type ImageParams, type Job, type WatermarkParams } from "../lib/api";
import { DropZone, Field, FileTable, Group, OutputNaming, Segmented, SwitchRow, useMediaFiles, type NamingPrefs } from "./shared";
import { loadPanelPrefs, savePanelPrefs } from "../lib/panelPrefs";
import { findMethod } from "../catalog";
import { WATERMARK_DEFAULTS, normalizeWatermark, pickWatermarkImage } from "../lib/watermark";
import { Icon } from "../components/Icon";
import { PresetBar } from "../components/PresetBar";

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
  watermark: { ...WATERMARK_DEFAULTS },
  compress: {
    mode: "target",
    targetMode: "absolute",
    targetKb: 500,
    targetPercent: 60,
    quality: 80,
    keepMetadata: false,
    pngPalette: true,
  },
};

/** 数字输入兜底：非数字回退到旧值，其余按范围取整（百分比允许小数时单独处理）。 */
function clampNumber(raw: string, min: number, max: number, fallback: number, integer = true): number {
  const value = Number(raw);
  if (!Number.isFinite(value)) return fallback;
  const clamped = Math.min(max, Math.max(min, value));
  return integer ? Math.round(clamped) : clamped;
}

/** 水印位置九宫格：每行一组，配合「自定义坐标」一起构成 3×3 + 自定义。 */
const WATERMARK_ROWS: { value: string; label: string }[][] = [
  [
    { value: "nw", label: "左上" },
    { value: "n", label: "上" },
    { value: "ne", label: "右上" },
  ],
  [
    { value: "w", label: "左" },
    { value: "c", label: "居中" },
    { value: "e", label: "右" },
  ],
  [
    { value: "sw", label: "左下" },
    { value: "s", label: "下" },
    { value: "se", label: "右下" },
  ],
];

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
  const [params, setParams] = useState<ImageParams>(() => {
    const loaded = loadPanelPrefs(`image:${methodId}`, defaults);
    return {
      ...loaded,
      watermark: normalizeWatermark(loaded.watermark),
      compress: { ...DEFAULT_PARAMS.compress, ...loaded.compress },
    };
  });
  const [naming, setNaming] = useState<NamingPrefs>(() => loadPanelPrefs(`naming:${methodId}`, { template: "", conflict: "suffix" }));
  const [starting, setStarting] = useState(false);
  useEffect(() => savePanelPrefs(`image:${methodId}`, params), [methodId, params]);
  useEffect(() => savePanelPrefs(`naming:${methodId}`, naming), [methodId, naming]);
  const [customEdge, setCustomEdge] = useState("");
  const [gif, setGif] = useState<ImageGifParams>(() =>
    loadPanelPrefs(`image:${methodId}:gif`, { fps: 10, loopCount: 0, maxWidth: 0, dither: true }),
  );
  useEffect(() => savePanelPrefs(`image:${methodId}:gif`, gif), [methodId, gif]);
  const setGifField = <K extends keyof ImageGifParams>(key: K, value: ImageGifParams[K]) =>
    setGif((prev) => ({ ...prev, [key]: value }));

  const isGif = methodId === "image-gif";

  const set = <K extends keyof ImageParams>(key: K, value: ImageParams[K]) =>
    setParams((prev) => ({ ...prev, [key]: value }));
  const setWatermark = <K extends keyof WatermarkParams>(key: K, value: WatermarkParams[K]) =>
    setParams((prev) => ({ ...prev, watermark: { ...prev.watermark, [key]: value } }));
  const setCompress = <K extends keyof CompressParams>(key: K, value: CompressParams[K]) =>
    setParams((prev) => ({ ...prev, compress: { ...prev.compress, [key]: value } }));

  const isWatermark = methodId === "image-watermark";
  const isCompress = methodId === "image-compress";
  // BMP/TIFF 后端直接拒绝；PNG 允许但目标大小模式收益有限（仅提示不阻止）。
  const compressUnsupported = isCompress && (params.format === "bmp" || params.format === "tiff");

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
    if (isGif && files.filter((file) => !file.error).length < 2) {
      toast("图片转 GIF 至少需要 2 张图片，请多选后再开始", "error");
      return;
    }
    if (isGif && (gif.fps < 1 || gif.fps > 30)) {
      toast("GIF 帧率必须在 1–30 之间", "error");
      return;
    }
    if (isWatermark && params.watermark.enabled && !params.watermark.path.trim()) {
      toast("水印已开启，请先选择水印图片（png / jpg / jpeg / webp）", "error");
      return;
    }
    if (compressUnsupported) {
      toast("BMP / TIFF 不支持压缩，请改选 JPG / PNG / WebP", "error");
      return;
    }
    setStarting(true);
    try {
      if (isGif) {
        // 多张图合并成一个任务：按文件名自然排序（img2 < img10），input=第一张，其余走 inputs。
        const sorted = files
          .filter((file) => !file.error)
          .slice()
          .sort((a, b) => a.name.localeCompare(b.name, undefined, { numeric: true, sensitivity: "base" }));
        const spec = {
          kind: "image-gif" as const,
          input: sorted[0].path,
          inputs: sorted.slice(1).map((file) => file.path),
          outputDir,
          params: { ...gif },
          nameTemplate: naming.template.trim() || undefined,
          conflict: naming.conflict,
        };
        const jobs = await api.enqueue([spec]);
        onEnqueued(jobs);
        toast(`已加入 ${jobs.length} 个任务（${sorted.length} 张图合成 GIF）`, "ok");
        clear();
        return;
      }
      // 水印关闭时不携带 watermark；非压缩方法不携带 compress，保持原逻辑完全一致。
      const { watermark, compress, ...paramsWithoutOptional } = params;
      const specs = files
        .filter((file) => !file.error)
        .map((file) => ({
          kind: isCompress ? ("image-compress" as const) : ("image" as const),
          input: file.path,
          outputDir,
          params: {
            ...paramsWithoutOptional,
            ...(watermark.enabled ? { watermark } : {}),
            ...(isCompress ? { compress } : {}),
          },
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
            <h1>{methodId === "image-watermark" ? "图片加水印" : methodId === "image-resize" ? "图片批量缩放" : methodId === "image-crop" ? "图片批量裁剪" : methodId === "image-compress" ? "图片压缩" : methodId === "image-gif" ? "图片转 GIF" : "图片批量转换"}</h1>
            <p>{isGif ? "多张图片按文件名自然排序合成 GIF · 两遍调色板（palettegen + paletteuse）" : "等比缩放不拉伸 · 永不覆盖原图 · 输出目录与源目录相同时自动加序号"}</p>
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
          {isGif ? (
            <Group title="GIF" icon="gif">
              <Field label="帧率（fps）" tag="1–30">
                <input
                  className="input"
                  type="number"
                  min={1}
                  max={30}
                  step={1}
                  value={gif.fps}
                  onChange={(event) => setGifField("fps", clampNumber(event.target.value, 1, 30, gif.fps))}
                />
              </Field>
              <Field label="循环次数" tag="0 = 无限循环">
                <input
                  className="input"
                  type="number"
                  min={0}
                  step={1}
                  value={gif.loopCount}
                  onChange={(event) =>
                    setGifField("loopCount", clampNumber(event.target.value, 0, 999, gif.loopCount))
                  }
                />
              </Field>
              <Field label="最大宽度（px）" tag="0 = 保持首图宽度">
                <input
                  className="input"
                  type="number"
                  min={0}
                  step={1}
                  value={gif.maxWidth}
                  onChange={(event) => {
                    const raw = Number(event.target.value);
                    if (!Number.isFinite(raw) || raw < 0) return;
                    if (raw > 2000) {
                      setGifField("maxWidth", 1280);
                      toast("最大宽度超过 2000px，已自动降到 1280px（GIF 过大会显著增大体积）", "info");
                    } else {
                      setGifField("maxWidth", Math.round(raw));
                    }
                  }}
                />
              </Field>
              <SwitchRow
                label="抖动（dither）"
                hint="paletteuse 用 sierra2_4a 抖动，色彩过渡更平滑；关闭后颜色更锐利但可能断层"
                on={gif.dither}
                onChange={(value) => setGifField("dither", value)}
              />
              <div className="note">
                按文件名自然排序（img2 在 img10 之前）后合成一个任务；每张图显示 1/fps 秒。GIF 只使用本组参数，
                上方格式 / 裁剪 / 尺寸设置不参与。GIF 后端固定用两遍调色板，避免单遍编码产生偏色。
              </div>
            </Group>
          ) : null}

          {!isGif ? <Group title="目标格式" icon="image">
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
          </Group> : null}

          {isCompress ? (
            <Group title="压缩" icon="compress">
              <Field label="模式" tag={params.compress.mode === "target" ? "二分逼近目标" : "固定质量重编码"}>
                <Segmented
                  value={params.compress.mode}
                  onChange={(value) => setCompress("mode", value)}
                  options={[
                    { value: "target", label: "目标大小" },
                    { value: "quality", label: "固定质量" },
                  ]}
                />
              </Field>
              {params.compress.mode === "target" ? (
                <>
                  <Field label="目标口径">
                    <Segmented
                      value={params.compress.targetMode}
                      onChange={(value) => setCompress("targetMode", value)}
                      options={[
                        { value: "absolute", label: "KB" },
                        { value: "percent", label: "原图百分比" },
                      ]}
                    />
                  </Field>
                  {params.compress.targetMode === "absolute" ? (
                    <Field label="目标大小（KB）" tag="1–5000">
                      <input
                        className="input"
                        type="number"
                        min={1}
                        max={5000}
                        step={1}
                        value={params.compress.targetKb}
                        onChange={(event) =>
                          setCompress("targetKb", clampNumber(event.target.value, 1, 5000, params.compress.targetKb))
                        }
                      />
                    </Field>
                  ) : (
                    <Field label={`目标为原图的 ${params.compress.targetPercent}%`} tag="1–99">
                      <div style={{ display: "flex", alignItems: "center", gap: 8 }}>
                        <input
                          className="slider"
                          type="range"
                          min={1}
                          max={99}
                          step={1}
                          style={{ flex: 1 }}
                          value={params.compress.targetPercent}
                          onChange={(event) =>
                            setCompress(
                              "targetPercent",
                              clampNumber(event.target.value, 1, 99, params.compress.targetPercent),
                            )
                          }
                        />
                        <input
                          className="input"
                          type="number"
                          min={1}
                          max={99}
                          step={1}
                          style={{ width: 64 }}
                          value={params.compress.targetPercent}
                          onChange={(event) =>
                            setCompress(
                              "targetPercent",
                              clampNumber(event.target.value, 1, 99, params.compress.targetPercent),
                            )
                          }
                        />
                      </div>
                    </Field>
                  )}
                  <div className="note">
                    target 模式用二分逼近目标大小（最多 8 次）；JPEG 质量下限 40、WebP 35，压到下限仍超目标时任务仍为完成但会标记「未达标」。
                  </div>
                </>
              ) : (
                <Field label={`质量 ${params.compress.quality}`} tag="1–100">
                  <div style={{ display: "flex", alignItems: "center", gap: 8 }}>
                    <input
                      className="slider"
                      type="range"
                      min={1}
                      max={100}
                      step={1}
                      style={{ flex: 1 }}
                      value={params.compress.quality}
                      onChange={(event) =>
                        setCompress("quality", clampNumber(event.target.value, 1, 100, params.compress.quality))
                      }
                    />
                    <input
                      className="input"
                      type="number"
                      min={1}
                      max={100}
                      step={1}
                      style={{ width: 64 }}
                      value={params.compress.quality}
                      onChange={(event) =>
                        setCompress("quality", clampNumber(event.target.value, 1, 100, params.compress.quality))
                      }
                    />
                  </div>
                </Field>
              )}
              <SwitchRow
                label="保留元数据（EXIF/ICC）"
                hint="默认关闭：剥离 EXIF/ICC，进一步减小体积"
                on={params.compress.keepMetadata}
                onChange={(value) => setCompress("keepMetadata", value)}
              />
              {params.format === "png" ? (
                <div className="note warn">
                  PNG 重编码几乎不减小体积，目标大小模式只做最大压缩/调色板量化，可能仍达不到目标
                </div>
              ) : null}
              {compressUnsupported ? (
                <div className="note warn">BMP / TIFF 不支持压缩，请改选 JPG / PNG / WebP 后再开始</div>
              ) : null}
              <div className="note">压缩不改变尺寸；若同时裁剪 / 缩放 / 水印，先做那些再编码。</div>
            </Group>
          ) : null}

          <Group title="裁剪" icon="crop" collapsed={methodId !== "image-crop"}>
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

          {isWatermark ? (
            <Group title="水印" icon="watermark">
              <SwitchRow
                label="启用水印"
                hint="为每张图片叠加同一张水印图；关闭时不向 ffmpeg 传任何水印参数"
                on={params.watermark.enabled}
                onChange={(value) => setWatermark("enabled", value)}
              />
              {params.watermark.enabled ? (
                <>
                  <Field label="水印图片" tag="png / jpg / jpeg / webp">
                    <button
                      className="btn small"
                      type="button"
                      onClick={async () => {
                        const picked = await pickWatermarkImage();
                        if (picked) setWatermark("path", picked);
                      }}
                    >
                      选择水印图片
                    </button>
                    <div className="note" title={params.watermark.path} style={{ wordBreak: "break-all" }}>
                      {params.watermark.path || "尚未选择水印图片"}
                    </div>
                  </Field>
                  <Field label="位置" tag={params.watermark.position === "custom" ? "自定义坐标" : "九宫格"}>
                    {WATERMARK_ROWS.map((row) => (
                      <Segmented
                        key={row.map((option) => option.value).join("-")}
                        value={params.watermark.position}
                        onChange={(value) => setWatermark("position", value)}
                        options={row}
                      />
                    ))}
                    <Segmented
                      value={params.watermark.position}
                      onChange={(value) => setWatermark("position", value)}
                      options={[{ value: "custom", label: "自定义坐标" }]}
                    />
                  </Field>
                  {params.watermark.position === "custom" ? (
                    <div className="row2">
                      <Field label="X（px）">
                        <input
                          className="input"
                          type="number"
                          step={1}
                          value={params.watermark.x}
                          onChange={(event) => setWatermark("x", Math.round(Number(event.target.value) || 0))}
                        />
                      </Field>
                      <Field label="Y（px）">
                        <input
                          className="input"
                          type="number"
                          step={1}
                          value={params.watermark.y}
                          onChange={(event) => setWatermark("y", Math.round(Number(event.target.value) || 0))}
                        />
                      </Field>
                    </div>
                  ) : null}
                  <Field label={`不透明度 ${params.watermark.opacity.toFixed(2)}`} tag="0–1">
                    <input
                      className="slider"
                      type="range"
                      min={0}
                      max={1}
                      step={0.05}
                      value={params.watermark.opacity}
                      onChange={(event) => setWatermark("opacity", Number(event.target.value))}
                    />
                  </Field>
                  <Field label={`缩放 ${params.watermark.scalePercent}%`} tag="相对短边 %">
                    <input
                      className="slider"
                      type="range"
                      min={5}
                      max={50}
                      step={1}
                      value={params.watermark.scalePercent}
                      onChange={(event) => setWatermark("scalePercent", Number(event.target.value))}
                    />
                  </Field>
                  <div className="row2">
                    <Field label="边距（px）">
                      <input
                        className="input"
                        type="number"
                        min={0}
                        step={1}
                        value={params.watermark.margin}
                        onChange={(event) => setWatermark("margin", Math.max(0, Math.round(Number(event.target.value) || 0)))}
                      />
                    </Field>
                    <Field label={`旋转 ${params.watermark.rotation}°`} tag="-180–180">
                      <input
                        className="input"
                        type="number"
                        min={-180}
                        max={180}
                        step={1}
                        value={params.watermark.rotation}
                        onChange={(event) => {
                          const next = Number(event.target.value);
                          if (Number.isFinite(next) && next >= -180 && next <= 180) setWatermark("rotation", Math.round(next));
                        }}
                      />
                    </Field>
                  </div>
                  <div className="note">
                    水印按输出画面短边百分比缩放，九宫格会自动留出边距；自定义坐标为水印左上角像素位置。水印图仅支持 png / jpg / jpeg / webp。
                  </div>
                </>
              ) : null}
            </Group>
          ) : null}

          <Group title="方向" icon="crop" collapsed>
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
        <PresetBar methodId={methodId} title={findMethod(methodId)?.title ?? methodId} />
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
        <div style={{ fontSize: 12, color: "var(--ff-fg3)" }}>
          {totals.count ? (
            <>
              共 <b style={{ color: "var(--ff-fg2)" }}>{totals.count}</b> 张 · 源文件 {formatBytes(totals.bytes)}
              {totals.duration ? ` · 时长 ${formatDuration(totals.duration)}` : ""}
            </>
          ) : (
            "添加图片后显示统计"
          )}
        </div>
        <button
          className="btn primary"
          onClick={start}
          disabled={starting || !files.length || compressUnsupported}
          title={compressUnsupported ? "BMP / TIFF 不支持压缩" : undefined}
        >
          <Icon name="play" size={14} />
          {starting ? "正在入队…" : isGif ? "生成 GIF" : isCompress ? "开始压缩" : "开始转换"}
        </button>
      </div>
    </div>
  );
}
