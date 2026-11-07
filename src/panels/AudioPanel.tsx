import { useEffect, useMemo, useState, type ReactNode } from "react";
import { formatBytes, formatDuration } from "@formatforge/design-tokens";
import { api, type AudioMergeParams, type AudioParams, type AudioTrimParams, type Capabilities, type Job } from "../lib/api";
import { AudioBitrateField, DropZone, Field, FileTable, Group, OutputNaming, Segmented, SwitchRow, useMediaFiles, type NamingPrefs } from "./shared";
import { loadPanelPrefs, savePanelPrefs } from "../lib/panelPrefs";
import { findMethod } from "../catalog";
import {
  AUDIO_FORMATS,
  audioFormatAvailable,
  findAudioFormat,
  formatProbeNotice,
} from "../lib/formats";
import { Icon } from "../components/Icon";
import { PresetBar } from "../components/PresetBar";
import { SPEED_MAX, SPEED_MIN, SPEED_PRESETS, SPEED_STEP, speedLabel } from "../lib/speed";

const DEFAULT_PARAMS: AudioParams = {
  format: "mp3",
  bitrateKbps: 192,
  rateMode: "bitrate",
  sampleRate: "source",
  channels: "source",
  normalize: false,
  speed: 1,
  keepPitch: true,
  volume: { enabled: false, gainDb: 0, mute: false },
};

interface AudioPanelProps {
  methodId: string;
  caps: Capabilities | null;
  outputDir: string;
  ffmpegReady: boolean;
  onChangeOutputDir: (dir: string) => void;
  onEnqueued: (jobs: Job[]) => void;
  toast: (text: string, kind?: "ok" | "error" | "info") => void;
  onBack: () => void;
}

/** 合并 / 截取是独立工作流，先分流再渲染，避免在同一组件里条件调用 hooks。 */
export function AudioPanel(props: AudioPanelProps) {
  if (props.methodId === "audio-merge") return <AudioMergePanel {...props} />;
  if (props.methodId === "audio-mix") return <AudioMergePanel {...props} mix />;
  if (props.methodId === "audio-trim") return <AudioTrimPanel {...props} />;
  return <AudioTranscode {...props} />;
}

function AudioTranscode({
  methodId,
  caps,
  outputDir,
  ffmpegReady,
  onChangeOutputDir,
  onEnqueued,
  toast,
  onBack,
}: AudioPanelProps) {
  const { files, busy, notice, pick, remove, clear } = useMediaFiles(["audio", "video"]);
  const defaults: AudioParams = {
    ...DEFAULT_PARAMS,
    format: methodId === "audio-aac" ? "aac" : "mp3",
    normalize: methodId === "audio-volume",
  };
  // 参数持久化：重启后恢复上次的码率/采样率/声道等设置。
  const [params, setParams] = useState<AudioParams>(() => {
    const loaded = loadPanelPrefs(`audio:${methodId}`, defaults);
    // 兼容已删除的「默认」：音频侧按 AAC 立体声缺省 128 kbps 映射到固定档位；
    // 空值按旧版语义落到固定档位。不能静默丢弃旧设置。
    const rawMode = (loaded as { rateMode?: string }).rateMode;
    const normalized: AudioParams =
      rawMode === "default"
        ? { ...loaded, rateMode: "bitrate", bitrateKbps: 128 }
        : rawMode === ""
          ? { ...loaded, rateMode: "bitrate" }
          : loaded;
    // 兼容旧版本持久化数据（当时还没有 volume 字段）。
    return { ...normalized, volume: normalized.volume ?? { enabled: false, gainDb: 0, mute: false } };
  });
  const [naming, setNaming] = useState<NamingPrefs>(() => loadPanelPrefs(`naming:${methodId}`, { template: "", conflict: "suffix" }));
  const [starting, setStarting] = useState(false);
  useEffect(() => savePanelPrefs(`audio:${methodId}`, params), [methodId, params]);
  useEffect(() => savePanelPrefs(`naming:${methodId}`, naming), [methodId, naming]);

  const set = <K extends keyof AudioParams>(key: K, value: AudioParams[K]) =>
    setParams((prev) => ({ ...prev, [key]: value }));

  const formatSpec = findAudioFormat(params.format) ?? AUDIO_FORMATS[0];
  const encoderCap = useMemo(
    () =>
      caps?.audioEncoders.find(
        (encoder) =>
          formatSpec.encoders.includes(encoder.id) && encoder.compiled && encoder.allowed,
      ) ?? null,
    [caps, formatSpec],
  );
  const encoderReady = !caps || audioFormatAvailable(caps, params.format);
  const probeNotice = formatProbeNotice(ffmpegReady, caps);
  const lossy = formatSpec.lossy;

  const estimate = useMemo(() => {
    const rawDuration = files.reduce((sum, file) => sum + (file.durationSec ?? 0), 0);
    if (!rawDuration || !lossy) return null;
    const duration = rawDuration / (params.speed || 1);
    // 「保持原始」按第一条文件的源音轨码率估算，读取不到时按固定 128 kbps 估算（与后端一致）。
    const first = files[0];
    const sourceKbps = Math.round((first?.audioBitrate ?? 0) / 1000);
    const kbps =
      params.rateMode === "source"
        ? (sourceKbps > 0 ? sourceKbps : 128)
        : params.bitrateKbps;
    const bytes = (kbps * 1000 * duration) / 8;
    return { bytes, duration };
  }, [files, params.rateMode, params.bitrateKbps, params.speed, lossy]);

  const start = async () => {
    if (!files.length) {
      toast("请先添加至少一个音频文件", "error");
      return;
    }
    if (!ffmpegReady) {
      toast("未找到 ffmpeg，无法开始转换", "error");
      return;
    }
    if (!encoderReady) {
      toast(
        `${formatSpec.label} 编码器不可用：${encoderCap?.reason ?? "当前 ffmpeg 构建未包含该编码器"}`,
        "error",
      );
      return;
    }
    setStarting(true);
    try {
      const kind = params.format === "aac" ? ("audio-aac" as const) : ("audio-mp3" as const);
      const specs = files
        .filter((file) => !file.error)
        .map((file) => ({
          kind,
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

  const title =
    methodId === "audio-volume" ? "音量标准化" : `音频转 ${formatSpec.label}`;

  return (
    <div className="workbench">
      <div className="wb-main">
        <div className="wb-head">
          <button className="btn small ghost" onClick={onBack} title="返回方法列表">
            ← 方法列表
          </button>
          <div>
            <h1>{title}</h1>
            <p>MP3 / AAC（M4A）/ FLAC / WAV / OGG / Opus · 编码器按探测结果可用性灰显 · 从音频或视频提取音轨</p>
          </div>
          <button className="btn" style={{ marginLeft: "auto" }} onClick={pick} disabled={busy}>
            <Icon name="plus" size={14} />
            {busy ? "读取中…" : "添加文件"}
          </button>
        </div>
        <DropZone kind={["audio", "video"]} busy={busy} onPick={pick} />
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
          <span className="sub">{files.length} 个文件</span>
        </div>
        <div className="inspector-body">
          <Group title="目标格式" icon="music">
            {probeNotice ? <div className={`note ${probeNotice.level === "warn" ? "warn" : ""}`}>{probeNotice.text}</div> : null}
            <Field label="编码格式" tag={`输出 .${formatSpec.ext}`}>
              <Segmented
                value={params.format}
                onChange={(value) => set("format", value)}
                options={AUDIO_FORMATS.map((format) => {
                  const ok = audioFormatAvailable(caps, format.id);
                  return {
                    value: format.id as AudioParams["format"],
                    label: format.label,
                    disabled: !ok,
                    title: ok
                      ? `输出 .${format.ext}`
                      : `本机 ffmpeg 没有可用的 ${format.label} 编码器（需要 ${format.encoders.join(" / ")}）`,
                  };
                })}
              />
            </Field>
            {lossy ? (
              <AudioBitrateField
                mode={params.rateMode ?? "bitrate"}
                kbps={params.bitrateKbps}
                onChange={(mode, kbps) =>
                  setParams((prev) => ({
                    ...prev,
                    rateMode: mode === "" ? "bitrate" : mode,
                    bitrateKbps: kbps,
                  }))
                }
              />
            ) : (
              <div className="note">{formatSpec.label} 为无损格式，不使用码率参数，输出大小由源决定。</div>
            )}
            <div className="note">
              源文件为视频时只提取第一条音轨；视频画面不会被保留。若源文件没有音轨，任务会失败并给出原因。
              源码率读不到（ffprobe 失败）时，「保持原始」按 128 kbps 编码。
            </div>
          </Group>

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
              无损格式（FLAC/WAV）变速同样生效；保持音高用 atempo 链。
            </div>
          </Group>

          <Group title="采样与声道" icon="wave">
            <Field label="采样率">
              <Segmented
                value={params.sampleRate}
                onChange={(value) => set("sampleRate", value)}
                options={[
                  { value: "source", label: "保持原始" },
                  { value: "44100", label: "44.1 kHz" },
                  { value: "48000", label: "48 kHz" },
                ]}
              />
            </Field>
            <Field label="声道">
              <Segmented
                value={params.channels}
                onChange={(value) => set("channels", value)}
                options={[
                  { value: "source", label: "保持原始" },
                  { value: "mono", label: "单声道" },
                  { value: "stereo", label: "立体声" },
                ]}
              />
            </Field>
            <SwitchRow
              label="音量归一化"
              hint={
                params.volume.enabled
                  ? "已启用音量增益：loudnorm 会把整体响度拉回 -16 LUFS，增益会被抵消；两者语义不同，请二选一"
                  : "EBU R128（-16 LUFS），把忽大忽小的音量拉到一致水平"
              }
              on={params.normalize}
              onChange={(value) => set("normalize", value)}
              disabled={params.volume.enabled}
            />
          </Group>

          <Group title="音量" icon="wave">
            <SwitchRow
              label="启用音量增益"
              hint="相对增益：整条音轨统一放大/衰减固定 dB，不改变动态范围"
              on={params.volume.enabled}
              onChange={(value) =>
                setParams((prev) => ({
                  ...prev,
                  // 开启增益时自动关闭归一化，避免后端拒绝这个互斥组合。
                  normalize: value ? false : prev.normalize,
                  volume: { ...prev.volume, enabled: value },
                }))
              }
            />
            {params.volume.enabled ? (
              <Field
                label={`增益 ${params.volume.gainDb > 0 ? "+" : ""}${params.volume.gainDb} dB`}
                tag="-20–20"
              >
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
                      setParams((prev) => ({
                        ...prev,
                        volume: { ...prev.volume, gainDb: next },
                      }));
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
                      setParams((prev) => ({
                        ...prev,
                        volume: { ...prev.volume, gainDb: next },
                      }));
                    }
                  }}
                />
              </Field>
            ) : null}
            <SwitchRow
              label="静音（移除音轨）"
              hint="静音时忽略增益；音频任务会输出一段等长静音"
              on={params.volume.mute}
              onChange={(value) =>
                setParams((prev) => ({ ...prev, volume: { ...prev.volume, mute: value } }))
              }
            />
            <div className="note">
              音量＝相对增益；响度归一化＝把整体拉到 -16 LUFS，两者不同且不能同时开启。
            </div>
          </Group>

          <Group title="编码器状态" icon="chip">
            {caps ? (
              <div className={`note ${encoderReady ? "" : "warn"}`}>
                {encoderReady
                  ? `${encoderCap?.label ?? formatSpec.encoders[0]} · 试编通过，可以使用。`
                  : `${formatSpec.label} 编码器不可用：${encoderCap?.reason ?? "当前 FFmpeg 构建未包含该编码器"}。`}
              </div>
            ) : (
              <div className="note">正在探测本机编码能力…</div>
            )}
            <div className="note">
              默认走 LGPL 路线：仅使用 FFmpeg 内置 AAC 与 LAME（LGPL）等许可友好编码器，不启用 fdk-aac 等 nonfree 组件。
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

// ---------------------------------------------------------------------------
// 音频合并 / 音频截取：共用输出格式与采样设置，文件列表语义与转码面板一致。

const DEFAULT_OUTPUT: AudioMergeParams = {
  format: "mp3",
  bitrateKbps: 192,
  rateMode: "bitrate",
  sampleRate: "source",
  channels: "source",
  normalize: false,
};

function AudioOutputSettings({
  params,
  patch,
  caps,
  ffmpegReady,
  sourceHint,
}: {
  params: AudioMergeParams;
  patch: (next: Partial<AudioMergeParams>) => void;
  caps: Capabilities | null;
  ffmpegReady: boolean;
  sourceHint: string;
}) {
  const spec = findAudioFormat(params.format) ?? AUDIO_FORMATS[0];
  const probeNotice = formatProbeNotice(ffmpegReady, caps);
  return (
    <>
      <Group title="目标格式" icon="music">
        {probeNotice ? (
          <div className={`note ${probeNotice.level === "warn" ? "warn" : ""}`}>{probeNotice.text}</div>
        ) : null}
        <Field label="编码格式" tag={`输出 .${spec.ext}`}>
          <Segmented
            value={params.format}
            onChange={(value) => patch({ format: value as AudioMergeParams["format"] })}
            options={AUDIO_FORMATS.map((format) => {
              const ok = audioFormatAvailable(caps, format.id);
              return {
                value: format.id as AudioMergeParams["format"],
                label: format.label,
                disabled: !ok,
                title: ok ? `输出 .${format.ext}` : `本机 ffmpeg 没有可用的 ${format.label} 编码器`,
              };
            })}
          />
        </Field>
        {spec.lossy ? (
          <AudioBitrateField
            mode={params.rateMode ?? "bitrate"}
            kbps={params.bitrateKbps}
            onChange={(mode, kbps) =>
              patch({ rateMode: mode === "" ? "bitrate" : mode, bitrateKbps: kbps })
            }
          />
        ) : (
          <div className="note">{spec.label} 为无损格式，不使用码率参数。</div>
        )}
      </Group>

      <Group title="采样与声道" icon="wave">
        <Field label="采样率">
          <Segmented
            value={params.sampleRate}
            onChange={(value) => patch({ sampleRate: value as AudioMergeParams["sampleRate"] })}
            options={[
              { value: "source", label: "保持原始" },
              { value: "44100", label: "44.1 kHz" },
              { value: "48000", label: "48 kHz" },
            ]}
          />
        </Field>
        <Field label="声道">
          <Segmented
            value={params.channels}
            onChange={(value) => patch({ channels: value as AudioMergeParams["channels"] })}
            options={[
              { value: "source", label: "保持原始" },
              { value: "mono", label: "单声道" },
              { value: "stereo", label: "立体声" },
            ]}
          />
        </Field>
        <div className="note">{sourceHint}</div>
        <SwitchRow
          label="音量归一化"
          hint="EBU R128（-16 LUFS），输出整体响度统一"
          on={params.normalize}
          onChange={(value) => patch({ normalize: value })}
        />
      </Group>
    </>
  );
}

function AudioBottomBar({
  methodId,
  outputDir,
  onChangeOutputDir,
  hint,
  canStart,
  starting,
  label,
  onStart,
}: {
  methodId: string;
  outputDir: string;
  onChangeOutputDir: (dir: string) => void;
  hint: ReactNode;
  canStart: boolean;
  starting: boolean;
  label: string;
  onStart: () => void;
}) {
  return (
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
      <div style={{ fontSize: 10.5, color: "var(--ff-fg3)" }}>{hint}</div>
      <button className="btn primary" onClick={onStart} disabled={starting || !canStart}>
        <Icon name="play" size={14} />
        {starting ? "正在入队…" : label}
      </button>
    </div>
  );
}

function AudioMergePanel({
  mix = false,
  caps,
  outputDir,
  ffmpegReady,
  onChangeOutputDir,
  onEnqueued,
  toast,
  onBack,
  }: AudioPanelProps & { mix?: boolean }) {
  const key = mix ? "audio-mix" : "audio-merge";
  const action = mix ? "混音" : "合并";
  const { files, busy, notice, pick, remove, clear } = useMediaFiles(["audio", "video"]);
  const [params, setParams] = useState<AudioMergeParams>(() =>
    loadPanelPrefs(`audio:${key}`, DEFAULT_OUTPUT),
  );
  const [naming, setNaming] = useState<NamingPrefs>(() =>
    loadPanelPrefs(`naming:${key}`, { template: "", conflict: "suffix" }),
  );
  const [starting, setStarting] = useState(false);
  useEffect(() => savePanelPrefs(`audio:${key}`, params), [key, params]);
  useEffect(() => savePanelPrefs(`naming:${key}`, naming), [key, naming]);
  const patch = (next: Partial<AudioMergeParams>) => setParams((prev) => ({ ...prev, ...next }));

  const usable = files.filter((file) => !file.error);
  const duration = mix
    ? Math.max(0, ...usable.map((file) => file.durationSec ?? 0))
    : usable.reduce((sum, file) => sum + (file.durationSec ?? 0), 0);
  const lossy = (findAudioFormat(params.format) ?? AUDIO_FORMATS[0]).lossy;
  const sourceKbps = Math.round((usable[0]?.audioBitrate ?? 0) / 1000);
  const kbps =
    params.rateMode === "source" ? (sourceKbps > 0 ? sourceKbps : 128) : params.bitrateKbps;
  const estimate =
    lossy && duration > 0
      ? { duration, bytes: (kbps * 1000 * duration) / 8 }
      : null;

  const start = async () => {
    if (usable.length < 2) {
      toast(`音频${action}至少需要 2 个音频文件`, "error");
      return;
    }
    if (!ffmpegReady) {
      toast(`未找到 ffmpeg，无法开始${action}`, "error");
      return;
    }
    setStarting(true);
    try {
      const [first, ...rest] = usable;
      const jobs = await api.enqueue([
        {
          kind: mix ? "audio-mix" : "audio-merge",
          input: first.path,
          inputs: rest.map((file) => file.path),
          outputDir,
          params: { ...params },
          nameTemplate: naming.template.trim() || undefined,
          conflict: naming.conflict,
        },
      ]);
      onEnqueued(jobs);
      toast(`已加入音频${action}任务（${usable.length} 个输入）`, "ok");
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
            <h1>音频{mix ? "混音" : "合并"}</h1>
            <p>
              {mix
                ? "多路同时混合 · 每路等权衰减防削波 · 时长取最长的一路"
                : "按列表顺序拼接 · 逐路统一采样率与声道后重编码 · 绝不覆盖原文件"}
            </p>
          </div>
          <button className="btn" style={{ marginLeft: "auto" }} onClick={pick} disabled={busy}>
            <Icon name="plus" size={14} />
            {busy ? "读取中…" : "添加文件"}
          </button>
        </div>
        <DropZone kind={["audio", "video"]} busy={busy} onPick={pick} />
        {notice ? (
          <div className="note warn" style={{ margin: "10px 20px 0" }}>
            {notice}
          </div>
        ) : null}
        <div className="note" style={{ margin: "10px 20px 0" }}>
          {mix
            ? "所有文件同时开始播放并混合（时长取最长的一路）；每路按 1/N 等权衰减避免削波。至少需要 2 个文件，源文件是视频时只取第一条音轨。"
            : "文件按添加顺序拼接；需要调整顺序请移除后重新添加。至少需要 2 个文件，源文件是视频时只取第一条音轨。"}
        </div>
        <FileTable files={files} onRemove={remove} onClear={clear} />
      </div>

      <aside className="inspector">
        <div className="inspector-head">
          <Icon name="settings" size={15} />
          <b>{mix ? "混音参数" : "合并参数"}</b>
          <span className="sub">{usable.length} 个输入</span>
        </div>
        <div className="inspector-body">
          <AudioOutputSettings
            params={params}
            patch={patch}
            caps={caps}
            ffmpegReady={ffmpegReady}
            sourceHint={
              mix
                ? "选择「保持原始」时，采样率与声道以第一个文件为准，其余文件会被统一到该参数再混合。"
                : "选择「保持原始」时，采样率与声道以第一个文件为准，其余文件会被统一到该参数再拼接。"
            }
          />
          <OutputNaming naming={naming} onChange={setNaming} />
        </div>
      </aside>

      <AudioBottomBar
        methodId={key}
        outputDir={outputDir}
        onChangeOutputDir={onChangeOutputDir}
        canStart={usable.length >= 2}
        starting={starting}
        label={`开始${action}`}
        onStart={start}
        hint={
          estimate ? (
            <>
              {mix ? "最长一路" : "总时长"} {formatDuration(estimate.duration)} · 粗估体积{" "}
              <b style={{ color: "var(--ff-fg2)" }}>{formatBytes(estimate.bytes)}</b>
            </>
          ) : (
            `至少添加 2 个文件后开始${action}`
          )
        }
      />
    </div>
  );
}

function AudioTrimPanel({
  caps,
  outputDir,
  ffmpegReady,
  onChangeOutputDir,
  onEnqueued,
  toast,
  onBack,
}: AudioPanelProps) {
  const { files, busy, notice, pick, remove, clear } = useMediaFiles(["audio", "video"]);
  const [params, setParams] = useState<AudioTrimParams>(() =>
    loadPanelPrefs("audio:audio-trim", { ...DEFAULT_OUTPUT, startSec: 0, endSec: 30 }),
  );
  const [naming, setNaming] = useState<NamingPrefs>(() =>
    loadPanelPrefs("naming:audio-trim", { template: "", conflict: "suffix" }),
  );
  const [starting, setStarting] = useState(false);
  useEffect(() => savePanelPrefs("audio:audio-trim", params), [params]);
  useEffect(() => savePanelPrefs("naming:audio-trim", naming), [naming]);
  const patch = (next: Partial<AudioTrimParams>) => setParams((prev) => ({ ...prev, ...next }));

  const usable = files.filter((file) => !file.error);
  const rangeLength = params.endSec - params.startSec;
  const rangeOk = params.startSec >= 0 && rangeLength > 0;
  const tooLong = usable.find((file) => (file.durationSec ?? 0) > 0 && params.endSec > (file.durationSec ?? 0) + 0.05);
  const lossy = (findAudioFormat(params.format) ?? AUDIO_FORMATS[0]).lossy;
  const sourceKbps = Math.round((usable[0]?.audioBitrate ?? 0) / 1000);
  const kbps =
    params.rateMode === "source" ? (sourceKbps > 0 ? sourceKbps : 128) : params.bitrateKbps;
  const estimate =
    lossy && rangeOk && usable.length
      ? { duration: rangeLength * usable.length, bytes: (kbps * 1000 * rangeLength * usable.length) / 8 }
      : null;

  const start = async () => {
    if (!usable.length) {
      toast("请先添加至少一个音频文件", "error");
      return;
    }
    if (!rangeOk) {
      toast("截取区间无效：起点必须 ≥ 0 且终点大于起点", "error");
      return;
    }
    if (tooLong) {
      toast(
        `${tooLong.name} 的时长只有 ${formatDuration(tooLong.durationSec ?? 0)}，终点不能超过源时长`,
        "error",
      );
      return;
    }
    if (!ffmpegReady) {
      toast("未找到 ffmpeg，无法开始截取", "error");
      return;
    }
    setStarting(true);
    try {
      const jobs = await api.enqueue(
        usable.map((file) => ({
          kind: "audio-trim" as const,
          input: file.path,
          outputDir,
          params: { ...params },
          nameTemplate: naming.template.trim() || undefined,
          conflict: naming.conflict,
        })),
      );
      onEnqueued(jobs);
      toast(`已加入 ${jobs.length} 个音频截取任务`, "ok");
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
            <h1>音频截取</h1>
            <p>按时间区间截取并重编码 · 区间对所有文件生效 · 起点终点单位为秒</p>
          </div>
          <button className="btn" style={{ marginLeft: "auto" }} onClick={pick} disabled={busy}>
            <Icon name="plus" size={14} />
            {busy ? "读取中…" : "添加文件"}
          </button>
        </div>
        <DropZone kind={["audio", "video"]} busy={busy} onPick={pick} />
        {notice ? (
          <div className="note warn" style={{ margin: "10px 20px 0" }}>
            {notice}
          </div>
        ) : null}
        <div className="note" style={{ margin: "10px 20px 0" }}>
          同一区间会应用到列表里的每个文件；源文件是视频时只取第一条音轨。终点超出源时长会在入队阶段被拒绝并给出原因。
        </div>
        <FileTable files={files} onRemove={remove} onClear={clear} />
      </div>

      <aside className="inspector">
        <div className="inspector-head">
          <Icon name="settings" size={15} />
          <b>截取参数</b>
          <span className="sub">{usable.length} 个文件</span>
        </div>
        <div className="inspector-body">
          <Group title="截取区间" icon="scissors">
            <div className="row2">
              <Field label="起点（秒）">
                <input
                  className="input"
                  type="number"
                  min={0}
                  step={0.1}
                  value={params.startSec}
                  onChange={(event) => {
                    const next = Number(event.target.value);
                    if (Number.isFinite(next) && next >= 0) patch({ startSec: next });
                  }}
                />
              </Field>
              <Field label="终点（秒）">
                <input
                  className="input"
                  type="number"
                  min={0}
                  step={0.1}
                  value={params.endSec}
                  onChange={(event) => {
                    const next = Number(event.target.value);
                    if (Number.isFinite(next) && next >= 0) patch({ endSec: next });
                  }}
                />
              </Field>
            </div>
            <div className={`note ${!rangeOk || tooLong ? "warn" : ""}`}>
              {!rangeOk
                ? "区间无效：起点必须 ≥ 0 且终点大于起点。"
                : tooLong
                  ? `区间超出 ${tooLong.name} 的源时长（${formatDuration(tooLong.durationSec ?? 0)}），入队会被拒绝。`
                  : `将截取 ${formatDuration(rangeLength)}${usable[0]?.durationSec ? `（${usable[0].name} 源时长 ${formatDuration(usable[0].durationSec)}）` : ""}。`}
            </div>
          </Group>

          <AudioOutputSettings
            params={params}
            patch={patch}
            caps={caps}
            ffmpegReady={ffmpegReady}
            sourceHint="「保持原始」的采样率与声道取源文件；截取一律重新编码，边界按时间点准确。"
          />
          <OutputNaming naming={naming} onChange={setNaming} />
        </div>
      </aside>

      <AudioBottomBar
        methodId="audio-trim"
        outputDir={outputDir}
        onChangeOutputDir={onChangeOutputDir}
        canStart={usable.length > 0 && rangeOk && !tooLong}
        starting={starting}
        label="开始截取"
        onStart={start}
        hint={
          estimate ? (
            <>
              {usable.length} 个任务 · 合计时长 {formatDuration(estimate.duration)} · 粗估体积{" "}
              <b style={{ color: "var(--ff-fg2)" }}>{formatBytes(estimate.bytes)}</b>
            </>
          ) : (
            "添加文件并填写有效区间后开始"
          )
        }
      />
    </div>
  );
}
