import { useEffect, useMemo, useState } from "react";
import { formatBytes, formatDuration } from "@formatforge/design-tokens";
import { api, type Capabilities, type Job, type VideoParams } from "../lib/api";
import { DropZone, Field, FileTable, Group, OutputNaming, Segmented, SwitchRow, useMediaFiles, type NamingPrefs } from "./shared";
import { loadPanelPrefs, savePanelPrefs } from "../lib/panelPrefs";
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

const DEFAULT_PARAMS: VideoParams = {
  container: "mp4",
  codec: "h264",
  encoder: "auto",
  rateMode: "quality",
  quality: 23,
  bitrateKbps: 8000,
  resolution: "source",
  fps: "source",
  crop: "source",
  rotation: "0",
  audioMode: "aac",
  audioBitrateKbps: 192,
  hwDecode: true,
  faststart: true,
};

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
  const [params, setParams] = useState<VideoParams>(() => loadPanelPrefs(`video:${methodId}`, initialParams(methodId)));
  const [naming, setNaming] = useState<NamingPrefs>(() => loadPanelPrefs(`naming:${methodId}`, { template: "", conflict: "suffix" }));
  const [starting, setStarting] = useState(false);
  useEffect(() => savePanelPrefs(`video:${methodId}`, params), [methodId, params]);
  useEffect(() => savePanelPrefs(`naming:${methodId}`, naming), [methodId, naming]);

  const set = <K extends keyof VideoParams>(key: K, value: VideoParams[K]) =>
    setParams((prev) => ({ ...prev, [key]: value }));

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

  const estimate = useMemo(() => {
    const duration = files.reduce((sum, f) => sum + (f.durationSec ?? 0), 0);
    if (!duration) return null;
    const videoKbps = params.rateMode === "bitrate" ? params.bitrateKbps : qualityHintKbps(params.resolution, params.quality);
    const audioKbps = params.audioMode === "none" ? 0 : params.audioBitrateKbps;
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
    setStarting(true);
    try {
      const specs = files
        .filter((f) => !f.error)
        .map((f) => ({
          kind: "video-mp4" as const,
          input: f.path,
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
            <h1>{methodId === "video-compress" ? "视频压缩" : methodId === "video-crop" ? "画面裁剪" : "视频转码"}</h1>
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
                  { value: "quality", label: "恒定质量" },
                  { value: "bitrate", label: "目标码率" },
                ]}
              />
            </Field>
            {params.rateMode === "quality" ? (
              <Field label={`质量 ${params.quality}（数值越小越清晰）`}>
                <input
                  className="slider"
                  type="range"
                  min={16}
                  max={34}
                  value={params.quality}
                  onChange={(e) => set("quality", Number(e.target.value))}
                />
              </Field>
            ) : (
              <Field label="目标码率（kbps）">
                <input
                  className="input"
                  type="number"
                  min={300}
                  step={100}
                  value={params.bitrateKbps}
                  onChange={(e) => set("bitrateKbps", Math.max(300, Number(e.target.value) || 0))}
                />
              </Field>
            )}
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
                    disabled: params.container === "webm",
                    title: params.container === "webm" ? "WebM 只能封装 Opus / Vorbis，源音轨通常不兼容" : undefined,
                  },
                  { value: "none", label: "移除" },
                ]}
              />
            </Field>
            {params.audioMode === "aac" ? (
              <Field label="音频码率">
                <select
                  className="select"
                  value={params.audioBitrateKbps}
                  onChange={(e) => set("audioBitrateKbps", Number(e.target.value))}
                >
                  <option value={96}>96 kbps</option>
                  <option value={128}>128 kbps</option>
                  <option value={192}>192 kbps</option>
                  <option value={256}>256 kbps</option>
                  <option value={320}>320 kbps</option>
                </select>
              </Field>
            ) : null}
            {params.audioMode === "copy" ? (
              <div className="note">原样复制不重新编码，速度快；若源音轨不被 MP4 支持会失败，届时可改为转 AAC。</div>
            ) : null}
          </Group>

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
        <button className="btn primary" onClick={start} disabled={starting || !files.length}>
          <Icon name="play" size={14} />
          {starting ? "正在入队…" : "开始转换"}
        </button>
      </div>
    </div>
  );
}
