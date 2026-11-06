import { useEffect, useMemo, useState } from "react";
import { formatBytes, formatDuration } from "@formatforge/design-tokens";
import { api, type Capabilities, type Job, type SubtitleParams, type VideoParams, type WatermarkParams } from "../lib/api";
import {
  AudioBitrateField,
  DropZone,
  Field,
  FileTable,
  Group,
  OutputNaming,
  QualityTierField,
  Segmented,
  SwitchRow,
  VideoBitrateField,
  useMediaFiles,
  type NamingPrefs,
} from "./shared";
import { loadPanelPrefs, savePanelPrefs } from "../lib/panelPrefs";
import { WATERMARK_DEFAULTS, normalizeWatermark, pickFontFile, pickWatermarkImage } from "../lib/watermark";
import {
  SUBTITLE_DEFAULTS,
  SUBTITLE_SOFT_CONTAINERS,
  normalizeSubtitle,
  pickSubtitleFile,
} from "../lib/subtitle";
import {
  CODEC_LABELS,
  VIDEO_CONTAINERS,
  containerAvailable,
  containerCodecs,
  findContainer,
  formatProbeNotice,
} from "../lib/formats";
import { GifPanel, MergePanel, MuxPanel } from "./CompositePanels";
import { Icon } from "../components/Icon";
import { SPEED_MAX, SPEED_MIN, SPEED_PRESETS, SPEED_STEP, speedLabel } from "../lib/speed";

const DEFAULT_PARAMS: VideoParams = {
  container: "mp4",
  codec: "h264",
  encoder: "auto",
  rateMode: "quality",
  quality: 23,
  bitrateKbps: 8000,
  audioRateMode: "bitrate",
  resolution: "source",
  fps: "source",
  crop: "source",
  rotation: "0",
  audioMode: "aac",
  audioBitrateKbps: 192,
  hwDecode: true,
  faststart: true,
  speed: 1,
  keepPitch: true,
  watermark: { ...WATERMARK_DEFAULTS },
  subtitle: { ...SUBTITLE_DEFAULTS, extraPaths: [] },
  volume: { enabled: false, gainDb: 0, mute: false },
  split: { mode: "copy", container: "mp4", segmentIndex: 0, startSec: 0, endSec: 30 },
};

/** 分段文本解析：每行「开始-结束」，单位秒；格式非法返回 null。 */
function parseSegments(text: string): { startSec: number; endSec: number }[] | null {
  const segments: { startSec: number; endSec: number }[] = [];
  for (const raw of text.split(/\r?\n/)) {
    const line = raw.trim();
    if (!line) continue;
    const match = line.match(/^(\d+(?:\.\d+)?)\s*[-–~]\s*(\d+(?:\.\d+)?)$/);
    if (!match) return null;
    const startSec = Number(match[1]);
    const endSec = Number(match[2]);
    if (!Number.isFinite(startSec) || !Number.isFinite(endSec) || startSec < 0 || endSec <= startSec) {
      return null;
    }
    segments.push({ startSec, endSec });
  }
  return segments.length ? segments : null;
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

function qualityHintKbps(resolution: string, quality: number): number {
  const base = resolution === "2160" ? 16000 : resolution === "1440" ? 8000 : resolution === "720" ? 1800 : resolution === "480" ? 900 : 4200;
  const scale = Math.pow(2, (23 - quality) / 6);
  return Math.round(base * scale);
}

/** 「视频压缩」与「视频转 MP4」共用同一面板，只是默认码控方式不同。 */
function initialParams(methodId: string): VideoParams {
  if (methodId === "video-compress") {
    return { ...DEFAULT_PARAMS, rateMode: "bitrate", bitrateKbps: 4000, quality: 26 };
  }
  if (methodId === "video-crop") {
    return { ...DEFAULT_PARAMS, crop: "16:9" };
  }
  return DEFAULT_PARAMS;
}

export interface VideoPanelProps {
  methodId: string;
  caps: Capabilities | null;
  outputDir: string;
  ffmpegReady: boolean;
  onChangeOutputDir: (dir: string) => void;
  onEnqueued: (jobs: Job[]) => void;
  toast: (text: string, kind?: "ok" | "error" | "info") => void;
  onBack: () => void;
}

export function VideoPanel(props: VideoPanelProps) {
  // 合并 / 混流 / GIF 是独立工作流，直接交给专用面板；
  // 先分流再渲染子组件，避免在同一个组件里「有条件地调用 hooks」。
  if (props.methodId === "video-merge") return <MergePanel {...props} />;
  if (props.methodId === "video-mux") return <MuxPanel {...props} />;
  if (props.methodId === "video-gif") return <GifPanel {...props} />;
  return <VideoWorkbench {...props} />;
}

function VideoWorkbench({ methodId, caps, outputDir, ffmpegReady, onChangeOutputDir, onEnqueued, toast, onBack }: VideoPanelProps) {
  const { files, busy, notice, pick, remove, clear } = useMediaFiles("video");
  // 面板参数持久化：重启后恢复上次选择（按方法隔离，避免不同方法互相污染默认值）。
  const [params, setParams] = useState<VideoParams>(() => {
    const loaded = loadPanelPrefs(`video:${methodId}`, initialParams(methodId));
    // 兼容已删除的「默认」：视频侧映射到质量档 + CRF 23（旧默认就是 x264 缺省 CRF 23）；
    // 空值按旧版语义落到目标码率。不能静默丢弃旧设置。
    const rawMode = (loaded as { rateMode?: string }).rateMode;
    const normalized: VideoParams =
      rawMode === "default"
        ? { ...loaded, rateMode: "quality", quality: 23 }
        : rawMode === ""
          ? { ...loaded, rateMode: "bitrate" }
          : loaded;
    // 音轨侧同理会读回 `default`：后端按 128 kbps 编码，界面必须显示同一值，
    // 否则会显示旧的 320 kbps 却实际编码 128 kbps。
    const rawAudioMode = (loaded as { audioRateMode?: string }).audioRateMode;
    const audioNormalized: VideoParams =
      rawAudioMode === "default"
        ? { ...normalized, audioRateMode: "bitrate", audioBitrateKbps: 128 }
        : rawAudioMode === ""
          ? { ...normalized, audioRateMode: "bitrate" }
          : normalized;
    return {
      ...audioNormalized,
      watermark: normalizeWatermark(audioNormalized.watermark),
      subtitle: normalizeSubtitle(audioNormalized.subtitle),
      // 兼容旧版本持久化数据（当时还没有 volume / split 字段）。
      volume: audioNormalized.volume ?? { enabled: false, gainDb: 0, mute: false },
      split: audioNormalized.split ?? {
        mode: "copy",
        container: audioNormalized.container ?? "mp4",
        segmentIndex: 0,
        startSec: 0,
        endSec: 30,
      },
    };
  });
  const [naming, setNaming] = useState<NamingPrefs>(() => loadPanelPrefs(`naming:${methodId}`, { template: "", conflict: "suffix" }));
  const [starting, setStarting] = useState(false);
  const [splitText, setSplitText] = useState<string>(() => {
    const saved = loadPanelPrefs(`video:${methodId}:split`, { text: "0-30" });
    return typeof saved.text === "string" ? saved.text : "0-30";
  });
  useEffect(() => savePanelPrefs(`video:${methodId}`, params), [methodId, params]);
  useEffect(() => savePanelPrefs(`naming:${methodId}`, naming), [methodId, naming]);
  useEffect(
    () => savePanelPrefs(`video:${methodId}:split`, { text: splitText }),
    [methodId, splitText],
  );

  const set = <K extends keyof VideoParams>(key: K, value: VideoParams[K]) =>
    setParams((prev) => ({ ...prev, [key]: value }));
  const setWatermark = <K extends keyof WatermarkParams>(key: K, value: WatermarkParams[K]) =>
    setParams((prev) => ({ ...prev, watermark: { ...prev.watermark, [key]: value } }));
  const setSubtitle = <K extends keyof SubtitleParams>(key: K, value: SubtitleParams[K]) =>
    setParams((prev) => ({ ...prev, subtitle: { ...prev.subtitle, [key]: value } }));

  const isWatermark = methodId === "video-watermark";
  const isSubtitle = methodId === "video-subtitle";
  const isSplit = methodId === "video-split";

  // 字幕前置约束：容器不承载内嵌轨道、变速 + 烧录、缺少 libass 都不能入队。
  const subtitleBurnUnavailable = caps?.subtitleBurn === false;
  const subtitleSoftContainerBad =
    isSubtitle &&
    params.subtitle.enabled &&
    params.subtitle.mode === "soft" &&
    !SUBTITLE_SOFT_CONTAINERS.includes(params.container);
  const subtitleSpeedConflict =
    isSubtitle && params.subtitle.enabled && params.subtitle.mode === "hard" && params.speed !== 1;
  const subtitleBurnBlocked =
    isSubtitle && params.subtitle.enabled && params.subtitle.mode === "hard" && subtitleBurnUnavailable;
  const subtitleBlocked = subtitleSoftContainerBad || subtitleSpeedConflict || subtitleBurnBlocked;

  const codecCaps = useMemo(
    () => (caps ? caps.videoEncoders.filter((e) => e.codec === params.codec) : []),
    [caps, params.codec],
  );
  const containerSpec = findContainer(params.container) ?? VIDEO_CONTAINERS[0];
  const availableCodecs = useMemo(() => containerCodecs(caps, params.container), [caps, params.container]);
  const probeNotice = formatProbeNotice(ffmpegReady, caps);
  const codecOptions = useMemo(
    () =>
      containerSpec.codecs.map((codec) => {
        const ok = availableCodecs.includes(codec);
        return {
          value: codec,
          label: CODEC_LABELS[codec],
          disabled: !ok,
          title: ok ? undefined : `本机没有可用于 ${containerSpec.label} 的 ${CODEC_LABELS[codec]} 编码器`,
        };
      }),
    [availableCodecs, containerSpec],
  );

  // 探测完成后若当前编码器在新容器里不可用，自动切到该容器首个可用编码，
  // 避免保留一个会被后端拒绝的组合。
  useEffect(() => {
    if (!caps) return;
    if (!containerSpec.codecs.includes(params.codec) || !availableCodecs.includes(params.codec)) {
      const first = availableCodecs[0];
      if (first && first !== params.codec) {
        setParams((prev) => ({ ...prev, codec: first, encoder: "auto" }));
      }
    }
  }, [availableCodecs, caps, containerSpec.codecs, params.codec]);

  // 变速会改变音轨时长、音量增益需要滤镜，二者都不能「原样复制」；已选中 copy 时自动切回转码。
  useEffect(() => {
    if (params.audioMode === "copy" && (params.speed !== 1 || params.volume.enabled)) {
      setParams((prev) => ({ ...prev, audioMode: "aac" }));
    }
  }, [params.audioMode, params.speed, params.volume.enabled]);

  const estimate = useMemo(() => {
    const rawDuration = files.reduce((sum, f) => sum + (f.durationSec ?? 0), 0);
    if (!rawDuration) return null;
    const duration = rawDuration / (params.speed || 1);
    const first = files[0];
    const sourceVideoKbps = Math.round((first?.videoBitrate ?? first?.bitrate ?? 0) / 1000);
    // 「保持原始」优先用探测到的源视频码率，探测不到时按固定 4000 kbps 估算（与后端一致）。
    const videoKbps =
      params.rateMode === "bitrate"
        ? params.bitrateKbps
        : params.rateMode === "source"
          ? (sourceVideoKbps > 0 ? sourceVideoKbps : 4000)
          : qualityHintKbps(params.resolution, params.quality);
    const sourceAudioKbps = Math.round((first?.audioBitrate ?? 0) / 1000);
    const audioKbps =
      params.audioMode === "none"
        ? 0
        : params.audioRateMode === "source"
          ? (sourceAudioKbps > 0 ? sourceAudioKbps : 128)
          : params.audioBitrateKbps;
    return { bytes: ((videoKbps + audioKbps) * 1000 * duration) / 8, duration };
  }, [files, params]);

  const start = async () => {
    if (!files.length) {
      toast("请先添加至少一个视频文件", "error");
      return;
    }
    if (!ffmpegReady) {
      toast("未找到 ffmpeg，无法开始转换", "error");
      return;
    }
    const segments = isSplit ? parseSegments(splitText) : null;
    if (isSplit && !segments) {
      toast("分段格式应为逐行「开始-结束」（秒），例如 0-30；开始时间 ≥ 0 且结束必须大于开始", "error");
      return;
    }
    if (isWatermark && params.watermark.enabled) {
      if (params.watermark.mode === "text") {
        if (!params.watermark.text.trim()) {
          toast("文字水印已开启，请填写水印文字", "error");
          return;
        }
        if (!params.watermark.fontPath.trim()) {
          toast("文字水印已开启，请先选择字体文件（ttf / otf / ttc）", "error");
          return;
        }
      } else if (!params.watermark.path.trim()) {
        toast("水印已开启，请先选择水印图片（png / jpg / jpeg / webp）", "error");
        return;
      }
    }
    if (isSubtitle && params.subtitle.enabled) {
      if (!params.subtitle.path.trim()) {
        toast("字幕已开启，请先选择字幕文件（srt / ass / ssa / vtt）", "error");
        return;
      }
      if (subtitleSoftContainerBad) {
        toast(
          `容器 ${containerSpec.label} 不承载内嵌字幕轨，请改 MP4/MKV/MOV 或改用烧录`,
          "error",
        );
        return;
      }
      if (subtitleSpeedConflict) {
        toast("变速时不能烧录字幕：请改为内嵌字幕，或把倍速设回 1", "error");
        return;
      }
      if (subtitleBurnBlocked) {
        toast("当前 FFmpeg 未包含 libass，无法烧录字幕：请改用内嵌字幕，或安装带 libass 的 FFmpeg", "error");
        return;
      }
    }
    setStarting(true);
    try {
      // 水印 / 字幕关闭时不携带对应字段，保持与原逻辑完全一致。
      const { watermark, subtitle, ...paramsWithoutExtras } = params;
      const jobParams = {
        ...paramsWithoutExtras,
        ...(watermark.enabled ? { watermark } : {}),
        ...(isSubtitle && subtitle.enabled ? { subtitle } : {}),
      };
      // 分割：每段一个 JobSpec；一个文件 × 一个时间段 = 一个任务，失败能直接定位。
      const specs = isSplit
        ? files
            .filter((f) => !f.error)
            .flatMap((f) =>
              (segments ?? []).map((segment, index) => ({
                kind: "video-split" as const,
                input: f.path,
                outputDir,
                params: {
                  ...jobParams,
                  container: params.container,
                  split: {
                    mode: params.split.mode,
                    container: params.container,
                    segmentIndex: index,
                    startSec: segment.startSec,
                    endSec: segment.endSec,
                  },
                },
                nameTemplate: naming.template.trim() || undefined,
                conflict: naming.conflict,
              })),
            )
        : files
            .filter((f) => !f.error)
            .map((f) => ({
              kind: "video-mp4" as const,
              input: f.path,
              outputDir,
              params: jobParams,
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
            <h1>{methodId === "video-watermark" ? "添加水印" : methodId === "video-subtitle" ? "添加字幕" : methodId === "video-compress" ? "视频压缩" : methodId === "video-crop" ? "画面裁剪" : methodId === "video-split" ? "视频分割" : "视频转码"}</h1>
            <p>MP4 / MKV / WebM / MOV / TS / AVI · 编码器按容器与探测结果联动 · 硬件编码优先 · 绝不覆盖原文件</p>
          </div>
          <button className="btn" style={{ marginLeft: "auto" }} onClick={pick} disabled={busy}>
            <Icon name="plus" size={14} />
            {busy ? "读取中…" : "添加文件"}
          </button>
        </div>
        <DropZone kind="video" busy={busy} onPick={pick} />
        {notice ? <div className="note warn" style={{ margin: "10px 20px 0" }}>{notice}</div> : null}
        <FileTable files={files} onRemove={remove} onClear={clear} />
      </div>

      <aside className="inspector">
        <div className="inspector-head">
          <Icon name="settings" size={15} />
          <b>输出参数</b>
          <span className="sub">{files.length} 个文件</span>
        </div>
        <div className="inspector-body">
          <Group title="目标格式" icon="film">
            {probeNotice ? <div className={`note ${probeNotice.level === "warn" ? "warn" : ""}`}>{probeNotice.text}</div> : null}
            <Field label="容器" tag={`输出 .${containerSpec.ext}`}>
              <Segmented
                value={params.container}
                onChange={(value) => {
                  const spec = findContainer(value);
                  const nextCodec = spec?.codecs.find((codec) => containerCodecs(caps, value).includes(codec));
                  set("container", value);
                  set("encoder", "auto");
                  if (nextCodec) set("codec", nextCodec);
                }}
                options={VIDEO_CONTAINERS.map((container) => {
                  const ok = containerAvailable(caps, container.id);
                  return {
                    value: container.id,
                    label: container.label,
                    disabled: !ok,
                    title: ok
                      ? `可用编码：${containerCodecs(caps, container.id).map((codec) => CODEC_LABELS[codec]).join(" / ")}`
                      : `本机没有可与 ${container.label} 搭配的编码器（需要 ${container.codecs.map((c) => CODEC_LABELS[c]).join(" / ")}）`,
                  };
                })}
              />
            </Field>
            <Field label="视频编码">
              <Segmented
                value={params.codec}
                onChange={(value) => {
                  set("codec", value);
                  set("encoder", "auto");
                }}
                options={codecOptions}
              />
            </Field>
            <div className="note">
              容器与编码器按探测结果联动：不兼容的组合不会出现在可选项里；缺少编码器的选项灰显并说明原因。
              WebM 使用 VP9 / VP8 / AV1，音频自动改用 Opus / Vorbis；MP4 / MOV 固定 AAC。faststart 仅 MP4 / MOV 生效。
            </div>
          </Group>

          <Group title="编码器" icon="chip">
            <Field label="选择方式" tag="硬编优先">
              <Segmented<string>
                value={params.encoder === "auto" || params.encoder === "software" ? params.encoder : "manual"}
                onChange={(value) => {
                  if (value === "auto" || value === "software") set("encoder", value);
                }}
                options={[
                  { value: "auto", label: "自动（硬件优先）" },
                  { value: "software", label: "仅软件" },
                  ...(params.encoder !== "auto" && params.encoder !== "software"
                    ? [{ value: "manual" as const, label: "手动" }]
                    : []),
                ]}
              />
            </Field>
            <div style={{ display: "flex", flexDirection: "column", gap: 6 }}>
              {caps ? (
                codecCaps.length ? (
                  codecCaps.map((encoder) => (
                    <button
                      type="button"
                      key={encoder.id}
                      className={`enc-option ${params.encoder === encoder.id ? "on" : ""} ${encoder.available ? "" : "unavailable"}`}
                      onClick={() => encoder.available && set("encoder", encoder.id)}
                      title={encoder.reason ?? ""}
                    >
                      <span className={`enc-dot ${encoder.available ? "ok" : "bad"}`} />
                      <span className="enc-meta">
                        <b>
                          {encoder.label}
                          {encoder.hardware ? " · 硬编" : ""}
                        </b>
                        <small>
                          {encoder.available
                            ? `id: ${encoder.id} · 试编通过`
                            : encoder.reason ?? "不可用"}
                        </small>
                      </span>
                    </button>
                  ))
                ) : (
                  <div className="note warn">当前构建没有与所选编码格式匹配的编码器。</div>
                )
              ) : (
                <div className="note">正在探测本机编码能力…探测完成后可手动指定编码器。</div>
              )}
            </div>
            {params.encoder === "auto" ? (
              <div className="note">自动策略：优先硬件编码；初始化失败、参数被拒或显存不足时自动回退软件编码，并在任务中心标注回退原因。</div>
            ) : null}
          </Group>

          <Group title="画质与尺寸" icon="compress">
            <Field label="码控方式">
              <Segmented
                value={params.rateMode}
                onChange={(value) => set("rateMode", value)}
                options={[
                  { value: "quality", label: "质量档位" },
                  { value: "bitrate", label: "自定义码率" },
                  { value: "source", label: "保持原始" },
                ]}
              />
            </Field>
            {params.rateMode === "quality" ? (
              <QualityTierField crf={params.quality} onChange={(crf) => set("quality", crf)} />
            ) : null}
            {params.rateMode === "bitrate" ? (
              <VideoBitrateField kbps={params.bitrateKbps} onChange={(kbps) => set("bitrateKbps", kbps)} />
            ) : null}
            {params.rateMode === "source" ? (
              <div className="note">
                保持原始：沿用源文件的视频码率作为目标，输出体积接近源。
                读取不到源码率（ffprobe 失败）时按 4000 kbps 编码；新编码器无法逐字节复现原码率，实际码率会有偏差。
              </div>
            ) : null}
            <div className="row2">
              <Field label="分辨率">
                <select className="select" value={params.resolution} onChange={(e) => set("resolution", e.target.value as VideoParams["resolution"])}>
                  <option value="source">保持原始</option>
                  <option value="2160">2160p</option>
                  <option value="1440">1440p</option>
                  <option value="1080">1080p</option>
                  <option value="720">720p</option>
                  <option value="480">480p</option>
                </select>
              </Field>
              <Field label="帧率">
                <select className="select" value={params.fps} onChange={(e) => set("fps", e.target.value as VideoParams["fps"])}>
                  <option value="source">保持原始</option>
                  <option value="30">30 fps</option>
                  <option value="60">60 fps</option>
                </select>
              </Field>
            </div>
          </Group>

          <Group title="画面调整" icon="crop">
            <Field label="旋转" tag="顺时针">
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
            <Field label="裁剪比例" tag="居中">
              <select
                className="select"
                value={params.crop}
                onChange={(e) => set("crop", e.target.value as VideoParams["crop"])}
              >
                <option value="source">保持原始画面</option>
                <option value="16:9">16:9（横屏）</option>
                <option value="1:1">1:1（方形）</option>
                <option value="9:16">9:16（竖屏）</option>
              </select>
            </Field>
            <div className="note">
              处理顺序为「裁剪 → 旋转 → 缩放」；裁剪从画面中心取最大可用区域，不会拉伸。
            </div>
          </Group>

          {isWatermark ? (
            <Group title="水印" icon="watermark">
              <SwitchRow
                label="启用水印"
                hint="叠加图片或文字水印；关闭时不向 ffmpeg 传任何水印参数"
                on={params.watermark.enabled}
                onChange={(value) => setWatermark("enabled", value)}
              />
              {params.watermark.enabled ? (
                <>
                  <Field
                    label="水印类型"
                    tag={params.watermark.mode === "text" ? "drawtext 文字" : "图片叠加"}
                  >
                    <Segmented
                      value={params.watermark.mode}
                      onChange={(value) =>
                        setParams((prev) => ({
                          ...prev,
                          watermark: {
                            ...prev.watermark,
                            mode: value as "image" | "text",
                            // drawtext 不支持旋转：切到文字时强制归零，避免参数被静默忽略。
                            rotation: value === "text" ? 0 : prev.watermark.rotation,
                          },
                        }))
                      }
                      options={[
                        { value: "image", label: "图片水印" },
                        { value: "text", label: "文字水印" },
                      ]}
                    />
                  </Field>
                  {params.watermark.mode === "text" ? (
                    <>
                      <Field label="水印文字" tag="单行，最多 200 字">
                        <input
                          className="input"
                          type="text"
                          maxLength={200}
                          placeholder="例如：Transmutio 2026"
                          value={params.watermark.text}
                          onChange={(event) => setWatermark("text", event.target.value)}
                        />
                      </Field>
                      <Field label="字体文件" tag="ttf / otf / ttc（使用系统字体）">
                        <button
                          className="btn small"
                          type="button"
                          onClick={async () => {
                            const picked = await pickFontFile();
                            if (picked) setWatermark("fontPath", picked);
                          }}
                        >
                          选择字体文件
                        </button>
                        <div className="note" title={params.watermark.fontPath} style={{ wordBreak: "break-all" }}>
                          {params.watermark.fontPath ||
                            "尚未选择字体（中文建议 C:\\Windows\\Fonts\\msyh.ttc，缺失时任务会明确失败）"}
                        </div>
                      </Field>
                      <div className="row2">
                        <Field label={`字号 ${params.watermark.fontSizePercent}%`} tag="画面高度 %">
                          <input
                            className="input"
                            type="number"
                            min={1}
                            max={20}
                            step={0.5}
                            value={params.watermark.fontSizePercent}
                            onChange={(event) => {
                              const next = Number(event.target.value);
                              if (Number.isFinite(next)) setWatermark("fontSizePercent", next);
                            }}
                          />
                        </Field>
                        <Field label="描边宽度（px）" tag="0–20">
                          <input
                            className="input"
                            type="number"
                            min={0}
                            max={20}
                            step={1}
                            value={params.watermark.borderWidth}
                            onChange={(event) => {
                              const next = Number(event.target.value);
                              if (Number.isFinite(next)) setWatermark("borderWidth", Math.round(next));
                            }}
                          />
                        </Field>
                      </div>
                      <div className="row2">
                        <Field label="字体颜色" tag="#RRGGBB">
                          <input
                            className="input"
                            type="color"
                            value={params.watermark.fontColor}
                            onChange={(event) => setWatermark("fontColor", event.target.value)}
                          />
                        </Field>
                        <Field label="描边颜色" tag="#RRGGBB">
                          <input
                            className="input"
                            type="color"
                            value={params.watermark.borderColor}
                            onChange={(event) => setWatermark("borderColor", event.target.value)}
                          />
                        </Field>
                      </div>
                    </>
                  ) : (
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
                    </>
                  )}
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
                  {params.watermark.mode === "image" ? (
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
                  ) : (
                    <Field label="边距（px）" tag="距画面边缘">
                      <input
                        className="input"
                        type="number"
                        min={0}
                        step={1}
                        value={params.watermark.margin}
                        onChange={(event) => setWatermark("margin", Math.max(0, Math.round(Number(event.target.value) || 0)))}
                      />
                    </Field>
                  )}
                  <div className="note">
                    {params.watermark.mode === "text"
                      ? "文字水印由 ffmpeg drawtext 绘制：使用系统字体（不随应用打包），不支持换行与旋转；字体缺失或格式不支持时任务会直接失败并给出原因。"
                      : "水印按输出画面短边百分比缩放，九宫格会自动留出边距；自定义坐标为水印左上角像素位置。水印图仅支持 png / jpg / jpeg / webp。"}
                  </div>
                </>
              ) : null}
            </Group>
          ) : null}

          {isSubtitle ? (
            <Group title="字幕" icon="subtitle">
              <SwitchRow
                label="启用字幕"
                hint="关闭时不向 ffmpeg 传任何字幕参数，行为与未接入字幕时一致"
                on={params.subtitle.enabled}
                onChange={(value) => setSubtitle("enabled", value)}
              />
              {params.subtitle.enabled ? (
                <>
                  <Field label="字幕模式" tag={params.subtitle.mode === "hard" ? "烧录进画面" : "内嵌字幕轨"}>
                    <Segmented
                      value={params.subtitle.mode}
                      onChange={(value) =>
                        setParams((prev) => ({
                          ...prev,
                          subtitle: {
                            ...prev.subtitle,
                            mode: value,
                            // 烧录只支持一份字幕文件：切到 hard 时丢弃附加轨。
                            extraPaths: value === "hard" ? [] : prev.subtitle.extraPaths,
                          },
                        }))
                      }
                      options={[
                        { value: "soft", label: "内嵌", title: "字幕作为独立字幕轨封装进容器，可开关" },
                        {
                          value: "hard",
                          label: "烧录",
                          disabled: subtitleBurnUnavailable,
                          title: subtitleBurnUnavailable
                            ? "当前 FFmpeg 未包含 libass，无法烧录字幕"
                            : "把字幕渲染进画面，任意视频容器可用；变速时禁止",
                        },
                      ]}
                    />
                  </Field>
                  {subtitleBurnUnavailable ? (
                    <div className="note warn">当前 FFmpeg 未包含 libass，无法烧录字幕：请改用内嵌字幕，或安装带 libass 的 FFmpeg 构建。</div>
                  ) : null}
                  {subtitleSoftContainerBad ? (
                    <div className="note warn">
                      当前容器 {containerSpec.label} 不承载内嵌字幕轨，请改 MP4/MKV/MOV 或改用烧录。
                    </div>
                  ) : null}
                  {subtitleSpeedConflict ? (
                    <div className="note warn">
                      变速时不能烧录字幕（字幕时间轴会与变速后的画面/音频不同步）：请改为内嵌字幕，或把倍速设回 1。
                    </div>
                  ) : null}
                  <Field label="主字幕文件" tag="srt / ass / ssa / vtt">
                    <button
                      className="btn small"
                      type="button"
                      onClick={async () => {
                        const picked = await pickSubtitleFile(false);
                        if (picked?.[0]) setSubtitle("path", picked[0]);
                      }}
                    >
                      选择字幕文件
                    </button>
                    <div className="note" title={params.subtitle.path} style={{ wordBreak: "break-all" }}>
                      {params.subtitle.path || "尚未选择字幕文件"}
                    </div>
                  </Field>
                  {params.subtitle.mode === "soft" ? (
                    <Field label="附加字幕轨" tag="可多选">
                      <button
                        className="btn small"
                        type="button"
                        onClick={async () => {
                          const picked = await pickSubtitleFile(true);
                          if (!picked?.length) return;
                          setParams((prev) => {
                            const existing = new Set([prev.subtitle.path, ...prev.subtitle.extraPaths]);
                            return {
                              ...prev,
                              subtitle: {
                                ...prev.subtitle,
                                extraPaths: [
                                  ...prev.subtitle.extraPaths,
                                  ...picked.filter((path) => path !== prev.subtitle.path && !existing.has(path)),
                                ],
                              },
                            };
                          });
                        }}
                      >
                        添加更多字幕轨
                      </button>
                      {params.subtitle.extraPaths.length ? (
                        <div style={{ display: "flex", flexDirection: "column", gap: 4 }}>
                          {params.subtitle.extraPaths.map((path) => (
                            <div key={path} style={{ display: "flex", alignItems: "center", gap: 6 }}>
                              <code style={{ flex: 1, fontSize: 10.5, wordBreak: "break-all" }} title={path}>
                                {path}
                              </code>
                              <button
                                className="btn small ghost"
                                type="button"
                                onClick={() =>
                                  setSubtitle(
                                    "extraPaths",
                                    params.subtitle.extraPaths.filter((item) => item !== path),
                                  )
                                }
                              >
                                删除
                              </button>
                            </div>
                          ))}
                        </div>
                      ) : (
                        <div className="note">主字幕始终保留为第一条轨道；可继续添加上限内的附加字幕轨。</div>
                      )}
                    </Field>
                  ) : null}
                  <Field label="字幕语言" tag="默认 chi">
                    <input
                      className="input"
                      value={params.subtitle.language}
                      placeholder="chi"
                      onChange={(event) => setSubtitle("language", event.target.value)}
                    />
                  </Field>
                  {params.subtitle.mode === "soft" ? (
                    <SwitchRow
                      label="标为默认轨"
                      hint="给主字幕加 disposition:default，播放器默认选中该字幕"
                      on={params.subtitle.defaultTrack}
                      onChange={(value) => setSubtitle("defaultTrack", value)}
                    />
                  ) : null}
                  {params.subtitle.mode === "hard" ? (
                    <>
                      <Field label={`字号 ${params.subtitle.fontSizePct.toFixed(1)}%`} tag="画面高度 %">
                        <input
                          className="slider"
                          type="range"
                          min={1}
                          max={20}
                          step={0.5}
                          value={params.subtitle.fontSizePct}
                          onChange={(event) => setSubtitle("fontSizePct", Number(event.target.value))}
                        />
                      </Field>
                      <Field label={`描边 ${params.subtitle.outline}`} tag="0–6">
                        <input
                          className="slider"
                          type="range"
                          min={0}
                          max={6}
                          step={1}
                          value={params.subtitle.outline}
                          onChange={(event) => setSubtitle("outline", Number(event.target.value))}
                        />
                      </Field>
                      <Field label="位置">
                        <Segmented
                          value={params.subtitle.position}
                          onChange={(value) => setSubtitle("position", value)}
                          options={[
                            { value: "bottom", label: "底部" },
                            { value: "middle", label: "居中" },
                            { value: "top", label: "顶部" },
                          ]}
                        />
                      </Field>
                      <div className="row2">
                        <Field label="字幕颜色" tag="#RRGGBB">
                          <input
                            className="input"
                            value={params.subtitle.color}
                            onChange={(event) => {
                              const next = event.target.value;
                              if (/^#[0-9a-fA-F]{0,6}$/.test(next)) setSubtitle("color", next.toUpperCase());
                            }}
                            onBlur={() =>
                              setSubtitle(
                                "color",
                                /^#[0-9a-fA-F]{6}$/.test(params.subtitle.color)
                                  ? params.subtitle.color.toUpperCase()
                                  : SUBTITLE_DEFAULTS.color,
                              )
                            }
                          />
                        </Field>
                        <Field label="取色">
                          <input
                            className="input"
                            type="color"
                            value={
                              /^#[0-9a-fA-F]{6}$/.test(params.subtitle.color)
                                ? params.subtitle.color
                                : SUBTITLE_DEFAULTS.color
                            }
                            onChange={(event) => setSubtitle("color", event.target.value.toUpperCase())}
                          />
                        </Field>
                      </div>
                      <Field label="垂直边距（px）">
                        <input
                          className="input"
                          type="number"
                          min={0}
                          step={1}
                          value={params.subtitle.marginV}
                          onChange={(event) =>
                            setSubtitle("marginV", Math.max(0, Math.round(Number(event.target.value) || 0)))
                          }
                        />
                      </Field>
                    </>
                  ) : null}
                  <div className="note">
                    内嵌字幕仅 MP4 / MOV / MKV 承载（MP4 / MOV 用 mov_text，MKV 直接复制流）；WebM / TS / AVI 请改用烧录。烧录在任意视频容器可用，但不能与变速同时使用。
                  </div>
                </>
              ) : null}
            </Group>
          ) : null}

          <Group title="变速" icon="gauge">
            <Field label="倍速" tag={speedLabel(params.speed)}>
              <Segmented
                value={String(params.speed)}
                onChange={(value) => set("speed", Number(value))}
                options={SPEED_PRESETS.map((value) => ({
                  value: String(value),
                  label: speedLabel(value),
                }))}
              />
            </Field>
            <Field label="自定义倍速" tag="0.25–4.0">
              <input
                className="input"
                type="number"
                min={SPEED_MIN}
                max={SPEED_MAX}
                step={SPEED_STEP}
                value={params.speed}
                onChange={(e) => {
                  const next = Number(e.target.value);
                  // 越界时不写入，保留上一次合法值。
                  if (next === 0 || (next >= SPEED_MIN && next <= SPEED_MAX)) set("speed", next);
                }}
              />
            </Field>
            <SwitchRow
              label="保持音高"
              hint={
                params.keepPitch
                  ? "用 atempo 链变速，音高不变（单次 0.5–2.0，超出自动链式串接）"
                  : "关闭后音高随速度变化：会先把音频重采样到 48 kHz，输出采样率固定 48 kHz"
              }
              on={params.keepPitch}
              onChange={(value) => set("keepPitch", value)}
            />
            <div className="note">
              变速改变视频时长与音轨时长；与裁剪/旋转/缩放同时使用时顺序为「裁剪 → 旋转 → 缩放 → 变速」。音轨「原样复制」不能与变速同时使用。
            </div>
          </Group>

          <Group title="音频" icon="wave">
            <Field label="音轨">
              <Segmented
                value={params.audioMode}
                onChange={(value) => set("audioMode", value)}
                options={[
                  { value: "aac", label: "按容器转码" },
                  {
                    value: "copy",
                    label: "原样复制",
                    disabled: params.container === "webm" || params.speed !== 1 || params.volume.enabled,
                    title:
                      params.speed !== 1
                        ? "变速会改变音轨时长，不能原样复制"
                        : params.volume.enabled
                          ? "音量增益需要对音轨做滤镜，原样复制无法应用滤镜"
                          : params.container === "webm"
                            ? "WebM 只能封装 Opus / Vorbis，源音轨通常不兼容"
                            : undefined,
                  },
                  { value: "none", label: "移除" },
                ]}
              />
            </Field>
            {params.audioMode === "aac" ? (
              <AudioBitrateField
                mode={params.audioRateMode ?? "bitrate"}
                kbps={params.audioBitrateKbps}
                onChange={(mode, kbps) => {
                  // 同一控件既改模式也改数值，必须一起写入，避免模式与数值脱节。
                  setParams((prev) => ({
                    ...prev,
                    audioRateMode: mode === "" ? "bitrate" : mode,
                    audioBitrateKbps: kbps,
                  }));
                }}
              />
            ) : null}
            {params.audioMode === "copy" ? (
              <div className="note">原样复制不重新编码，速度快；若源音轨不被 MP4 支持会失败，届时可改为转 AAC。</div>
            ) : null}
          </Group>

          <Group title="音量" icon="wave">
            <SwitchRow
              label="启用音量增益"
              hint="相对增益：整条音轨统一放大/衰减固定 dB，不改变动态范围"
              on={params.volume.enabled}
              onChange={(value) =>
                setParams((prev) => ({
                  ...prev,
                  audioMode: value && prev.audioMode === "copy" ? "aac" : prev.audioMode,
                  volume: { ...prev.volume, enabled: value },
                }))
              }
            />
            {params.volume.enabled ? (
              <Field label={`增益 ${params.volume.gainDb > 0 ? "+" : ""}${params.volume.gainDb} dB`} tag="-20–20">
                <input
                  className="slider"
                  type="range"
                  min={-20}
                  max={20}
                  step={0.5}
                  value={params.volume.gainDb}
                  disabled={params.volume.mute}
                  onChange={(event) => {
                    const next = Number(event.target.value);
                    if (next >= -20 && next <= 20) {
                      setParams((prev) => ({ ...prev, volume: { ...prev.volume, gainDb: next } }));
                    }
                  }}
                />
                <input
                  className="input"
                  type="number"
                  min={-20}
                  max={20}
                  step={0.5}
                  value={params.volume.gainDb}
                  disabled={params.volume.mute}
                  onChange={(event) => {
                    const next = Number(event.target.value);
                    if (next >= -20 && next <= 20) {
                      setParams((prev) => ({ ...prev, volume: { ...prev.volume, gainDb: next } }));
                    }
                  }}
                />
              </Field>
            ) : null}
            <SwitchRow
              label="静音（移除音轨）"
              hint="静音时忽略增益，直接以 -an 丢弃音轨"
              on={params.volume.mute}
              onChange={(value) =>
                setParams((prev) => ({ ...prev, volume: { ...prev.volume, mute: value } }))
              }
            />
            <div className="note">
              音量＝相对增益；响度归一化（音频卡）＝把整体拉到 -16 LUFS，两者不同。
            </div>
          </Group>

          {isSplit ? (
            <Group title="分割" icon="split">
              <Field label="分割模式" tag={params.split.mode === "copy" ? "快，关键帧对齐" : "慢，帧级准确"}>
                <Segmented
                  value={params.split.mode}
                  onChange={(value) =>
                    setParams((prev) => ({ ...prev, split: { ...prev.split, mode: value as "copy" | "encode" } }))
                  }
                  options={[
                    {
                      value: "copy",
                      label: "流复制",
                      title: "直接复制音视频流，速度快；只能切在关键帧，起点会有 GOP 量级的偏移",
                    },
                    {
                      value: "encode",
                      label: "重编码",
                      title: "重编码视频，切点帧级准确（±1 帧），速度较慢",
                    },
                  ]}
                />
              </Field>
              <Field label="分段" tag="每行 开始-结束（秒）">
                <textarea
                  className="input"
                  rows={4}
                  value={splitText}
                  placeholder={"0-30\n30-60"}
                  onChange={(event) => setSplitText(event.target.value)}
                  style={{ resize: "vertical", fontFamily: "inherit" }}
                />
              </Field>
              <div className="note">
                每行生成一个独立任务（文件名自动加 -partN）：任一段失败可在任务中心单独定位。输出容器沿用上方「目标格式」所选容器。
                流复制只能切在关键帧，起点偏移属正常；需要帧级准确请选重编码。
              </div>
            </Group>
          ) : null}

          <Group title="高级" icon="gpu">
            <SwitchRow
              label="硬件解码"
              hint={
                caps?.decodeAccels.some((d) => d.available)
                  ? `将使用 ${caps.decodeAccels.find((d) => d.available)?.label}（已用真实样本验证）`
                  : "本机未验证到可用的硬件解码方法，将使用软件解码"
              }
              on={params.hwDecode}
              onChange={(value) => set("hwDecode", value)}
              disabled={!caps?.decodeAccels.some((d) => d.available)}
            />
            <SwitchRow label="faststart" hint="把 moov 前置，便于网页边下边播" on={params.faststart} onChange={(value) => set("faststart", value)} />
            {caps && !caps.softwareH264Available ? (
              <div className="note warn">
                默认（LGPL）路线下本机没有软件 H.264 编码器。若硬件编码不可用，任务会失败并提示原因，不会偷偷改用 GPL 编码器。
              </div>
            ) : null}
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
              const picked = await import("@tauri-apps/plugin-dialog").then((m) => m.open({ directory: true, multiple: false }));
              if (typeof picked === "string") onChangeOutputDir(picked);
            }}
          >
            更改
          </button>
        </div>
        <div style={{ fontSize: 10.5, color: "var(--ff-fg3)" }}>
          {estimate ? (
            <>
              粗估体积 <b style={{ color: "var(--ff-fg2)" }}>{formatBytes(estimate.bytes)}</b> · 时长{" "}
              {formatDuration(estimate.duration)}
            </>
          ) : (
            "添加文件后显示粗估体积"
          )}
        </div>
        <button
          className="btn primary"
          onClick={start}
          disabled={starting || !files.length || subtitleBlocked}
          title={
            subtitleSoftContainerBad
              ? `容器 ${containerSpec.label} 不承载内嵌字幕轨，请改 MP4/MKV/MOV 或改用烧录`
              : subtitleSpeedConflict
                ? "变速时不能烧录字幕"
                : subtitleBurnBlocked
                  ? "当前 FFmpeg 未包含 libass，无法烧录字幕"
                  : undefined
          }
        >
          <Icon name="play" size={14} />
          {starting ? "正在入队…" : isSplit ? "开始分割" : "开始转换"}
        </button>
      </div>
    </div>
  );
}
