import type { Capabilities } from "./api";

/**
 * 视频容器 / 音频格式清单（与 Rust `containers.rs` 保持一致）。
 * 之前这里只有单一 mp4 与 mp3/aac，原因是清单被硬编码；现在容器与编码器组合
 * 集中声明，并按 ffmpeg 探测结果决定可用性（不可用的灰显并说明原因，不消失）。
 */
export type VideoCodecId = "h264" | "hevc" | "av1" | "vp9" | "vp8" | "mpeg4";

export interface VideoContainerDef {
  id: string;
  label: string;
  ext: string;
  codecs: VideoCodecId[];
  audio: string[];
  faststart: boolean;
}

export const VIDEO_CONTAINERS: VideoContainerDef[] = [
  { id: "mp4", label: "MP4", ext: "mp4", codecs: ["h264", "hevc", "av1"], audio: ["aac"], faststart: true },
  {
    id: "mkv",
    label: "MKV",
    ext: "mkv",
    codecs: ["h264", "hevc", "av1", "vp9", "vp8"],
    audio: ["aac", "libopus", "libvorbis"],
    faststart: false,
  },
  { id: "webm", label: "WebM", ext: "webm", codecs: ["vp9", "vp8", "av1"], audio: ["libopus", "libvorbis"], faststart: false },
  { id: "mov", label: "MOV", ext: "mov", codecs: ["h264", "hevc"], audio: ["aac"], faststart: true },
  { id: "ts", label: "TS", ext: "ts", codecs: ["h264", "hevc"], audio: ["aac"], faststart: false },
  { id: "avi", label: "AVI", ext: "avi", codecs: ["h264", "mpeg4"], audio: ["libmp3lame"], faststart: false },
];

export const CODEC_LABELS: Record<VideoCodecId, string> = {
  h264: "H.264",
  hevc: "HEVC",
  av1: "AV1",
  vp9: "VP9",
  vp8: "VP8",
  mpeg4: "MPEG-4",
};

export interface AudioFormatDef {
  id: string;
  label: string;
  ext: string;
  encoders: string[];
  lossy: boolean;
}

export const AUDIO_FORMATS: AudioFormatDef[] = [
  { id: "mp3", label: "MP3", ext: "mp3", encoders: ["libmp3lame", "libshine"], lossy: true },
  { id: "aac", label: "AAC（M4A）", ext: "m4a", encoders: ["aac"], lossy: true },
  { id: "flac", label: "FLAC", ext: "flac", encoders: ["flac"], lossy: false },
  { id: "wav", label: "WAV", ext: "wav", encoders: ["pcm_s16le"], lossy: false },
  { id: "ogg", label: "OGG（Vorbis）", ext: "ogg", encoders: ["libvorbis"], lossy: true },
  { id: "opus", label: "Opus", ext: "opus", encoders: ["libopus"], lossy: true },
];

export const PROBE_MISSING_NOTICE = "未检测到 ffmpeg 或编码器不可用：格式清单保持完整，但不可用的选项已灰显并注明原因。";

export function findContainer(id: string): VideoContainerDef | undefined {
  return VIDEO_CONTAINERS.find((container) => container.id === id);
}

export function findAudioFormat(id: string): AudioFormatDef | undefined {
  return AUDIO_FORMATS.find((format) => format.id === id);
}

export function encoderUsable(caps: Capabilities | null, encoderId: string): boolean {
  if (!caps) return false;
  return caps.audioEncoders.concat(caps.videoEncoders).some(
    (encoder) => encoder.id === encoderId && encoder.compiled && encoder.allowed && encoder.available,
  );
}

export function codecAvailable(caps: Capabilities | null, codec: string): boolean {
  if (!caps) return false;
  return caps.videoEncoders.some(
    (encoder) => encoder.codec === codec && encoder.compiled && encoder.allowed && encoder.available,
  );
}

/** 容器里至少有一个编码器可用才算可用；不适配的组合永远不会出现在编码器列表里。 */
export function containerCodecs(caps: Capabilities | null, containerId: string): VideoCodecId[] {
  const container = findContainer(containerId);
  if (!container) return [];
  return container.codecs.filter((codec) => codecAvailable(caps, codec));
}

export function containerAvailable(caps: Capabilities | null, containerId: string): boolean {
  return containerCodecs(caps, containerId).length > 0;
}

export function audioFormatAvailable(caps: Capabilities | null, formatId: string): boolean {
  const format = findAudioFormat(formatId);
  if (!format || !caps) return false;
  return format.encoders.some((encoder) => encoderUsable(caps, encoder));
}

/** 首次探测尚未完成时返回「正在探测」，探测完成但无任何编码器时返回明确缺失提示。 */
export function formatProbeNotice(
  ffmpegReady: boolean,
  caps: Capabilities | null,
): { level: "info" | "warn"; text: string } | null {
  if (!ffmpegReady) return { level: "warn", text: PROBE_MISSING_NOTICE };
  if (!caps) {
    return { level: "info", text: "正在探测本机编码器：格式清单已完整渲染，可用性会在探测完成后自动刷新。" };
  }
  const anyVideo = caps.videoEncoders.some((e) => e.compiled && e.allowed && e.available);
  const anyAudio = caps.audioEncoders.some((e) => e.compiled && e.allowed && e.available);
  if (!anyVideo || !anyAudio) return { level: "warn", text: PROBE_MISSING_NOTICE };
  return null;
}
