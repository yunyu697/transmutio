/**
 * Tauri 后端命令的类型化封装。UI 只依赖本文件，便于后续替换实现。
 */
import { invoke } from "@tauri-apps/api/core";

export interface ToolLocation {
  path: string;
  source: "env" | "bundled" | "app-dir" | "resources" | "path" | "custom" | string;
  exists: boolean;
}

export interface BuildFlags {
  version: string;
  configuration: string;
  gpl: boolean;
  nonfree: boolean;
  lgplOk: boolean;
  notes: string[];
}

export interface ToolsInfo {
  ffmpeg: ToolLocation | null;
  ffprobe: ToolLocation | null;
  ffmpegVersion: string | null;
  ffprobeVersion: string | null;
  build: BuildFlags;
  platform: string;
  arch: string;
  targetTriple: string;
  warnings: string[];
  searchNote: string;
}

export interface EncoderCap {
  id: string;
  codec: string;
  label: string;
  vendor: string;
  hardware: boolean;
  license: "lgpl" | "gpl" | "nonfree";
  allowed: boolean;
  compiled: boolean;
  available: boolean;
  reason: string | null;
}

export interface DecodeCap {
  id: string;
  label: string;
  available: boolean;
  reason: string | null;
}

export interface Capabilities {
  probedAt: number;
  cacheHit: boolean;
  fingerprint: string;
  platform: string;
  arch: string;
  ffmpegVersion: string;
  buildGpl: boolean;
  buildNonfree: boolean;
  videoEncoders: EncoderCap[];
  audioEncoders: EncoderCap[];
  decodeAccels: DecodeCap[];
  softwareH264Available: boolean;
  notes: string[];
}

export interface MediaInfo {
  path: string;
  name: string;
  ext: string;
  sizeBytes: number;
  kind: "video" | "audio" | "image" | "other";
  durationSec: number | null;
  width: number | null;
  height: number | null;
  videoCodec: string | null;
  audioCodec: string | null;
  pixFmt: string | null;
  bitrate: number | null;
  fps: number | null;
  sampleRate: number | null;
  channels: number | null;
  error: string | null;
}

export type JobKind =
  | "video-mp4"
  | "video-merge"
  | "video-mux"
  | "video-gif"
  | "audio-mp3"
  | "audio-aac"
  | "image"
  | "image-seq-video"
  | "video-frames"
  | "image-collage"
  | "pdf-merge"
  | "pdf-split"
  | "pdf-image";

export interface Job {
  id: string;
  kind: JobKind;
  input: string;
  /** 多输入任务（合并 / 混流）的完整输入列表；单输入任务为空数组。 */
  inputs: string[];
  inputName: string;
  output: string;
  params: Record<string, unknown>;
  status: "queued" | "running" | "done" | "failed" | "cancelled" | "skipped";
  progress: number;
  speed: string | null;
  etaSec: number | null;
  message: string | null;
  errorKind: string | null;
  encoderUsed: string | null;
  fallbackNote: string | null;
  attempts: number;
  /** suffix | skip | overwrite */
  conflict: string;
  /** 每任务诊断日志（完整 ffmpeg stderr），任务结束时生成 */
  logPath: string | null;
  durationSec: number | null;
  outputSize: number | null;
  createdAt: number;
  startedAt: number | null;
  finishedAt: number | null;
}

export interface JobSpec {
  kind: JobKind;
  input: string;
  /** 合并 / 混流的其余输入。 */
  inputs?: string[];
  outputDir: string;
  outputStem?: string;
  /** 命名模板：{name} {date} {index} {ext}；缺省等价于 {name} */
  nameTemplate?: string;
  /** 冲突策略：suffix（默认）| skip | overwrite */
  conflict?: "suffix" | "skip" | "overwrite";
  params: Record<string, unknown>;
}

export interface VideoParams {
  /** mp4 | mkv | webm | mov | ts | avi */
  container: string;
  codec: "h264" | "hevc" | "av1" | "vp9" | "vp8" | "mpeg4";
  encoder: string;
  rateMode: "quality" | "bitrate";
  quality: number;
  bitrateKbps: number;
  resolution: "source" | "2160" | "1440" | "1080" | "720" | "480";
  fps: "source" | "30" | "60";
  /** 居中裁剪画面比例 */
  crop: "source" | "16:9" | "1:1" | "9:16";
  rotation: "0" | "90" | "180" | "270";
  audioMode: "aac" | "copy" | "none";
  audioBitrateKbps: number;
  hwDecode: boolean;
  faststart: boolean;
  /** 播放倍速：0.25–4.0，1 表示不变 */
  speed: number;
  /** 变速时保持音高（默认 true：用 atempo 链） */
  keepPitch: boolean;
}

export interface AudioParams {
  /** mp3 | aac | flac | wav | ogg | opus */
  format: "mp3" | "aac" | "flac" | "wav" | "ogg" | "opus";
  bitrateKbps: number;
  sampleRate: "source" | "44100" | "48000";
  channels: "source" | "mono" | "stereo";
  /** EBU R128 响度归一化 */
  normalize: boolean;
  /** 播放倍速：0.25–4.0，1 表示不变 */
  speed: number;
  /** 变速时保持音高（默认 true：用 atempo 链） */
  keepPitch: boolean;
}

export interface ImageParams {
  format: "jpg" | "png" | "webp" | "bmp" | "tiff";
  quality: number;
  maxEdge: number;
  /** 手动旋转，叠加在 EXIF 自动校正之后 */
  rotation: "0" | "90" | "180" | "270";
  /** 读取 EXIF Orientation 并自动校正（开启时 ffmpeg 的隐式 autorotate 会被关闭） */
  autoOrient: boolean;
  /** 裁剪模式：none | ratio | custom */
  cropMode: "none" | "ratio" | "custom";
  /** ratio 模式下的比例 */
  cropRatio: "1:1" | "16:9" | "9:16" | "4:3";
  /** custom 矩形（像素） */
  cropX: number;
  cropY: number;
  cropW: number;
  cropH: number;
}

export interface SeqParams {
  fps: number;
  startNumber: number;
  /** source（要求同尺寸）| first | max | fixed */
  sizeMode: "source" | "first" | "max" | "fixed";
  width: number;
  height: number;
  encoder: string;
  quality: number;
  bitrateKbps: number;
}

export interface FramesParams {
  fps: number;
  /** 0 = 保持源宽度 */
  width: number;
  format: "png" | "jpg";
  quality: number;
}

export interface CollageParams {
  cols: number;
  /** 0 = 自动 */
  rows: number;
  cellWidth: number;
  cellHeight: number;
  gap: number;
  /** #RRGGBB */
  background: string;
  fit: "contain" | "cover";
  format: "png" | "jpg" | "webp";
  quality: number;
}

export interface PdfToolsInfo {
  pdftoppm: string | null;
  source: "env" | "path" | "custom" | string | null;
  available: boolean;
  reason: string | null;
  note: string;
}

export interface PresetFile {
  version: number;
  kind: string;
  createdAt: number;
  panels: Record<string, unknown>;
  output: Record<string, unknown>;
}

export interface MergeParams {
  mode: "auto" | "copy" | "transcode";
  resolution: "source" | "2160" | "1440" | "1080" | "720" | "480";
  fps: "source" | "30" | "60";
  audioMode: "aac" | "none";
  codec: "h264" | "hevc";
  quality: number;
  bitrateKbps: number;
}

export interface MuxParams {
  mode: "replace" | "add";
  audioMode: "aac" | "copy";
  audioBitrateKbps: number;
}

export interface GifParams {
  fps: number;
  /** 0 = 保持源宽度 */
  width: number;
  startSec: number;
  /** 0 = 到片尾 */
  durationSec: number;
  /** true = palettegen/paletteuse 两阶段调色板优化 */
  optimize: boolean;
}

export interface FfToolInspection {
  path: string;
  exists: boolean;
  size: number;
  sha256: string;
  version: string;
  configuration: string;
  gpl: boolean;
  nonfree: boolean;
  lgplOk: boolean;
  hasH264Encoder: boolean;
  hasAacEncoder: boolean;
  notes: string[];
}

export interface AppInfo {
  version: string;
  platform: string;
  arch: string;
  debug: boolean;
  /** "lgpl" | "gpl"，由编译期 feature 决定 */
  flavor: string;
}

/** 当前发行变体：编译期固定，前端据此切换文案、默认下载源与许可提示。 */
export interface FlavorInfo {
  id: "lgpl" | "gpl";
  productName: string;
  appIdentifier: string;
  legacyAppIdentifier: string;
  updateChannel: string;
  defaultSourceUrl: string;
  licenseLabel: string;
  /** GPL 变体恒为 true；LGPL 版仅 debug 构建可由开发开关打开 */
  allowGpl: boolean;
  allowGplCompiled: boolean;
  requireGplBuild: boolean;
  devOverrideAvailable: boolean;
  debug: boolean;
}

export interface MigrationNotice {
  migrated: boolean;
  detail: string;
  at: number;
  sources: string[];
}

export interface LegacyImportInfo {
  available: boolean;
  oldApp: string;
  oldLocal: string;
  pending: boolean;
}

export interface ImportSummary {
  files: number;
  bytes: number;
  backupApp: string;
  backupWebview: string | null;
  detail: string;
}

export const api = {
  flavorInfo: () => invoke<FlavorInfo>("flavor_info"),
  migrationNotice: () => invoke<MigrationNotice | null>("migration_notice"),
  legacyImportInfo: () => invoke<LegacyImportInfo>("legacy_import_info"),
  importLegacyData: () => invoke<ImportSummary>("import_legacy_data"),
  resolveTools: (customFfmpeg?: string) =>
    invoke<ToolsInfo>("resolve_tools", { customFfmpeg: customFfmpeg ?? null }),
  probeCapabilities: (customFfmpeg: string | undefined, allowGpl: boolean, force: boolean) =>
    invoke<Capabilities>("probe_capabilities", { customFfmpeg: customFfmpeg ?? null, allowGpl, force }),
  probeFiles: (paths: string[]) => invoke<MediaInfo[]>("probe_files", { paths }),
  enqueue: (specs: JobSpec[]) => invoke<Job[]>("enqueue_jobs", { specs }),
  listJobs: () => invoke<Job[]>("list_jobs"),
  cancelJob: (id: string) => invoke<void>("cancel_job", { id }),
  retryJob: (id: string) => invoke<void>("retry_job", { id }),
  removeFinished: () => invoke<string[]>("remove_finished_jobs"),
  setConcurrency: (value: number) => invoke<number>("set_job_concurrency", { value }),
  setPaused: (paused: boolean) => invoke<boolean>("set_job_paused", { paused }),
  getPaused: () => invoke<boolean>("get_job_paused"),
  setAfterAction: (action: "none" | "open-folder" | "shutdown") =>
    invoke<string>("set_after_action", { action }),
  openFolder: (path: string) => invoke<void>("open_folder", { path }),
  openJobFolder: (id: string) => invoke<void>("open_job_folder", { id }),
  openJobLog: (id: string) => invoke<void>("open_job_log", { id }),
  defaultOutputDir: () => invoke<string>("default_output_dir"),
  appInfo: () => invoke<AppInfo>("app_info"),
  saveDiagnostics: (text: string) => invoke<string>("save_diagnostics", { text }),
  inspectFfmpeg: (path: string) => invoke<FfToolInspection>("inspect_ffmpeg_file", { path }),
  pdfToolsStatus: (customDir?: string) =>
    invoke<PdfToolsInfo>("pdf_tools_status", { customDir: customDir ?? null }),
  exportPreset: (path: string, panels: Record<string, unknown>, output: Record<string, unknown>) =>
    invoke<PresetFile>("export_preset", { path, panels, output }),
  importPreset: (path: string) => invoke<PresetFile>("import_preset", { path }),
  installFfmpeg: (args: {
    ffmpegPath: string;
    ffprobePath?: string;
    expectedSha256?: string;
    allowGpl: boolean;
  }) =>
    invoke<ToolsInfo>("install_ffmpeg_files", {
      ffmpegPath: args.ffmpegPath,
      ffprobePath: args.ffprobePath ?? null,
      expectedSha256: args.expectedSha256 ?? null,
      allowGpl: args.allowGpl,
    }),
};
