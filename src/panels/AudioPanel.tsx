import { useEffect, useMemo, useState } from "react";
import { formatBytes, formatDuration } from "@formatforge/design-tokens";
import { api, type AudioParams, type Capabilities, type Job } from "../lib/api";
import { AudioBitrateField, DropZone, Field, FileTable, Group, OutputNaming, Segmented, SwitchRow, useMediaFiles, type NamingPrefs } from "./shared";
import { loadPanelPrefs, savePanelPrefs } from "../lib/panelPrefs";
import {
  AUDIO_FORMATS,
  audioFormatAvailable,
  findAudioFormat,
  formatProbeNotice,
} from "../lib/formats";
import { Icon } from "../components/Icon";
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

export function AudioPanel({
  methodId,
  caps,
  outputDir,
  ffmpegReady,
  onChangeOutputDir,
  onEnqueued,
  toast,
  onBack,
}: {
  methodId: string;
  caps: Capabilities | null;
  outputDir: string;
  ffmpegReady: boolean;
  onChangeOutputDir: (dir: string) => void;
  onEnqueued: (jobs: Job[]) => void;
  toast: (text: string, kind?: "ok" | "error" | "info") => void;
  onBack: () => void;
}) {
  const { files, busy, notice, pick, remove, clear } = useMediaFiles(["audio", "video"]);
  const defaults: AudioParams = {
    ...DEFAULT_PARAMS,
    format: methodId === "audio-aac" ? "aac" : "mp3",
    normalize: methodId === "audio-volume",
  };
  // 参数持久化：重启后恢复上次的码率/采样率/声道等设置。
  const [params, setParams] = useState<AudioParams>(() => {
    const loaded = loadPanelPrefs(`audio:${methodId}`, defaults);
    // 兼容旧版本持久化数据（当时还没有 volume 字段）。
    return { ...loaded, volume: loaded.volume ?? { enabled: false, gainDb: 0, mute: false } };
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
    // 「保持原始」按第一条文件的源音轨码率估算，「默认」按 AAC 内置默认 128 kbps 估算。
    const first = files[0];
    const sourceKbps = Math.round((first?.audioBitrate ?? 0) / 1000);
    const kbps =
      params.rateMode === "source" && sourceKbps > 0
        ? sourceKbps
        : params.rateMode === "default"
          ? 128
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
