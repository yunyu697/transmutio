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
  /** 是否可烧录字幕（FFmpeg 构建包含 libass / subtitles 滤镜） */
  subtitleBurn: boolean;
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
  /** 首条视频流码率（bps；部分容器缺失） */
  videoBitrate?: number | null;
  /** 首条音频流码率（bps；部分容器缺失） */
  audioBitrate?: number | null;
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
  | "video-split"
  | "audio-mp3"
  | "image-gif"
  | "audio-aac"
  | "audio-merge"
  | "audio-mix"
  | "audio-trim"
  | "image"
  | "image-compress"
  | "image-seq-video"
  | "video-frames"
  | "image-collage"
  | "pdf-merge"
  | "pdf-split"
  | "pdf-image"
  | "pdf-text";

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
  /** 压缩任务的目标字节数（quality 模式为空）。 */
  targetBytes?: number;
  /** 压缩任务是否达到目标大小；quality 模式为空。 */
  targetMet?: boolean;
  /** 压缩实际尝试次数（1–8）。 */
  attemptsUsed?: number;
  /** 压缩最终采用的质量（1–100）。 */
  qualityUsed?: number;
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

export interface DevicePreset {
  id: string;
  label: string;
  note: string;
  container: string;
  codec: string;
  resolution: string;
  fps: string;
  audioBitrateKbps: number;
}

/** 图片水印参数（后端 camelCase 反序列化；关闭时不传该字段）。 */
export interface WatermarkParams {
  enabled: boolean;
  /** image = 图片水印；text = 文字水印（drawtext） */
  mode: "image" | "text";
  /** 水印图片路径（png / jpg / jpeg / webp，mode=image） */
  path: string;
  /** 文字水印内容（mode=text） */
  text: string;
  /** 字体文件路径（mode=text，使用系统字体，缺失时后端明确报错） */
  fontPath: string;
  /** 字号：最终画面高度的百分比，1..20 */
  fontSizePercent: number;
  /** 字体颜色 #RRGGBB */
  fontColor: string;
  /** 描边宽度（像素，0..20） */
  borderWidth: number;
  /** 描边颜色 #RRGGBB */
  borderColor: string;
  /** nw | n | ne | w | c | e | sw | s | se | custom */
  position: string;
  /** 仅 custom：水印左上角坐标（像素） */
  x: number;
  y: number;
  /** 0..1 */
  opacity: number;
  /** 相对输出画面短边的百分比，5..50（仅图片水印） */
  scalePercent: number;
  /** 距画面边缘的留白（像素） */
  margin: number;
  /** -180..180（度，仅图片水印；文字水印必须为 0） */
  rotation: number;
}

/** 字幕参数（后端 camelCase 反序列化；关闭时不传该字段）。 */
export interface SubtitleParams {
  enabled: boolean;
  /** soft = 内嵌字幕轨；hard = 烧录进画面 */
  mode: "soft" | "hard";
  /** 主字幕文件（srt / ass / ssa / vtt） */
  path: string;
  /** 仅 soft：附加字幕轨 */
  extraPaths: string[];
  /** 字幕语言标签（默认 chi） */
  language: string;
  /** 主字幕标为默认轨（仅 soft） */
  defaultTrack: boolean;
  /** hard：字号（画面高度百分比，1–20） */
  fontSizePct: number;
  /** hard：描边 0–6 */
  outline: number;
  /** hard：bottom | middle | top */
  position: "bottom" | "middle" | "top";
  /** hard：#RRGGBB */
  color: string;
  /** hard：垂直边距（像素） */
  marginV: number;
}

/** 相对音量增益：只放大/衰减固定 dB，与音频卡的 loudnorm（-16 LUFS）语义不同。 */
export interface VolumeParams {
  enabled: boolean;
  /** -20..20 dB */
  gainDb: number;
  /** 静音：丢弃音轨（忽略 gainDb） */
  mute: boolean;
}

/** 按时间点切分（一个任务只切一段，前端按段拆成多个 JobSpec）。 */
export interface SplitParams {
  mode: "copy" | "encode";
  /** mp4 | mkv | webm | mov | ts | avi */
  container: string;
  segmentIndex: number;
  startSec: number;
  endSec: number;
}

/** 多图合成 GIF（palettegen + paletteuse 两遍）。 */
export interface ImageGifParams {
  /** 1..30 */
  fps: number;
  /** 0 = 无限循环 */
  loopCount: number;
  /** 0 = 保持首图宽度；>2000 时前端提示并自动填 1280 */
  maxWidth: number;
  dither: boolean;
}

export interface VideoParams {
  /** mp4 | mkv | webm | mov | ts | avi */
  container: string;
  codec: "h264" | "hevc" | "av1" | "vp9" | "vp8" | "mpeg4";
  encoder: string;
  /** quality（质量档位）| bitrate（自定义码率）| source（保持原始） */
  rateMode: "quality" | "bitrate" | "source";
  quality: number;
  bitrateKbps: number;
  /** 音频码控方式：bitrate | source（保持原始） */
  audioRateMode?: "bitrate" | "source";
  resolution: "source" | "2160" | "1440" | "1080" | "720" | "480";
  fps: "source" | "30" | "60";
  /** 居中裁剪画面比例（cropMode=ratio 时生效） */
  crop: "source" | "16:9" | "1:1" | "9:16";
  /** ratio（居中比例裁剪，默认）| rect（自定义矩形） */
  cropMode?: "ratio" | "rect";
  /** rect 模式：相对源画面宽高的百分比（0–100，原点左上角） */
  cropXPct?: number;
  cropYPct?: number;
  cropWPct?: number;
  cropHPct?: number;
  rotation: "0" | "90" | "180" | "270";
  audioMode: "aac" | "copy" | "none";
  audioBitrateKbps: number;
  hwDecode: boolean;
  faststart: boolean;
  /** 播放倍速：0.25–4.0，1 表示不变 */
  speed: number;
  /** 变速时保持音高（默认 true：用 atempo 链） */
  keepPitch: boolean;
  /** 图片水印；enabled=false 时入队不携带该字段 */
  watermark: WatermarkParams;
  /** 字幕；enabled=false 时入队不携带该字段 */
  subtitle: SubtitleParams;
  /** 相对音量增益（与音频卡 loudnorm 不同） */
  volume: VolumeParams;
  /** 分割参数（仅 video-split 卡片使用） */
  split: SplitParams;
}

export interface AudioParams {
  /** mp3 | aac | flac | wav | ogg | opus */
  format: "mp3" | "aac" | "flac" | "wav" | "ogg" | "opus";
  bitrateKbps: number;
  /** bitrate | source（保持原始） */
  rateMode?: "bitrate" | "source";
  sampleRate: "source" | "44100" | "48000";
  channels: "source" | "mono" | "stereo";
  /** EBU R128 响度归一化 */
  normalize: boolean;
  /** 播放倍速：0.25–4.0，1 表示不变 */
  speed: number;
  /** 变速时保持音高（默认 true：用 atempo 链） */
  keepPitch: boolean;
  /** 相对音量增益（与 loudnorm 互斥） */
  volume: VolumeParams;
}

/** 音频合并 / 截取的输出参数（两者共用输出格式与采样设置）。 */
export interface AudioOutputParams {
  /** mp3 | aac | flac | wav | ogg | opus */
  format: AudioParams["format"];
  bitrateKbps: number;
  /** bitrate | source（保持原始） */
  rateMode?: "bitrate" | "source";
  sampleRate: AudioParams["sampleRate"];
  channels: AudioParams["channels"];
  /** EBU R128 响度归一化 */
  normalize: boolean;
}

/** 音频合并：按列表顺序拼接，逐路统一采样率/声道后 concat。 */
export type AudioMergeParams = AudioOutputParams;

/** 音频截取：截取 [startSec, endSec) 并重编码。 */
export interface AudioTrimParams extends AudioOutputParams {
  startSec: number;
  endSec: number;
}

/** 图片压缩参数（仅 image-compress 任务读取；默认值对应后端默认）。 */
export interface CompressParams {
  /** target = 逼近目标大小；quality = 固定质量 */
  mode: "target" | "quality";
  /** absolute = 目标 KB；percent = 相对原图百分比 */
  targetMode: "absolute" | "percent";
  /** 目标绝对大小 KB，1–5000 */
  targetKb: number;
  /** 相对原图百分比，1–99 */
  targetPercent: number;
  /** 固定质量 1–100 */
  quality: number;
  /** 保留 EXIF/ICC（默认 false：剥离以减小体积） */
  keepMetadata: boolean;
  /** PNG 调色板量化（默认 true） */
  pngPalette: boolean;
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
  /** 图片水印；enabled=false 时入队不携带该字段 */
  watermark: WatermarkParams;
  /** 压缩参数；仅 image-compress 入队时携带 */
  compress: CompressParams;
}

export interface SeqParams {
  fps: number;
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
  pdftotext: string | null;
  source: "env" | "path" | "custom" | string | null;
  available: boolean;
  reason: string | null;
  textAvailable: boolean;
  textReason: string | null;
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
  /** bitrate（换轨场景没有「保持原始」：源码率属于另一路音轨） */
  audioRateMode?: "bitrate";
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

/** 内置下载清单里的一个来源（官方直连或第三方镜像）。 */
export interface PlanSource {
  sourceId: string;
  label: string;
  url: string;
  mirror: boolean;
}

/** 当前平台/变体可自动下载的一个包；校验和来自内置清单。 */
export interface PlanEntry {
  packageId: string;
  kind: "ffmpeg" | "poppler";
  version: string;
  buildLabel: string;
  file: string;
  size: number;
  sha256: string;
  sources: PlanSource[];
}

/** 没有可固定校验和的发布包时给出手动方案（不会伪造可下载项）。 */
export interface ToolsManualEntry {
  kind: string;
  platform: string;
  flavor: string;
  label: string;
  url: string;
  reason: string;
}

export interface AvifParams {
  quality: number;
  speed: number;
}

export interface AvifResult {
  src: string;
  output?: string | null;
  width: number;
  height: number;
  bytes: number;
  ok: boolean;
  message?: string | null;
}

export interface ExifFields {
  description?: string;
  artist?: string;
  copyright?: string;
  software?: string;
  dateTimeOriginal?: string;
  make?: string;
  model?: string;
  lensModel?: string;
  orientation?: number;
}

export interface ExifWriteResult {
  src: string;
  output?: string | null;
  ok: boolean;
  message?: string | null;
}

export interface ToolsPlan {
  platform: string;
  arch: string;
  flavor: string;
  entries: PlanEntry[];
  manual: ToolsManualEntry[];
  notes: string[];
}

export interface ToolsDownloadProgress {
  kind: string;
  packageId: string;
  phase: "checking" | "download" | "verify" | "extract" | "land" | "done" | "retry";
  sourceId: string;
  sourceLabel: string;
  sourceIndex: number;
  sourceTotal: number;
  attempt: number;
  received: number;
  total: number;
  message: string;
}

export interface LandedTool {
  kind: string;
  dir: string;
  version: string;
  archiveSha256: string;
  tools: ToolsInfo | null;
  pdf: PdfToolsInfo | null;
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

// ---------------------------------------------------------------- 更新器

export interface UpdateMirror {
  id: string;
  label: string;
  prefix: string;
  official: boolean;
}

export interface UpdateSourceAttempt {
  id: string;
  label: string;
  ok: boolean;
  elapsedMs: number;
  error: string | null;
}

export interface UpdateCandidate {
  version: string;
  tag: string;
  notes: string;
  publishedAt: string;
  assetName: string;
  assetSize: number;
  assetSha256: string | null;
  assetUrl: string;
  sumsUrl: string | null;
  signatureUrl: string | null;
  sourceId: string;
  sourceLabel: string;
}

export interface UpdateCheckResult {
  currentVersion: string;
  channel: string;
  platform: string;
  arch: string;
  available: boolean;
  candidate: UpdateCandidate | null;
  attempts: UpdateSourceAttempt[];
  /** install = 下载 → 校验 → 启动安装；download-only = 只下载，安装交给系统/包管理器 */
  installMode: "install" | "download-only";
  installModeLabel: string;
  signatureConfigured: boolean;
  signatureNote: string;
  homepage: string;
}

export interface UpdaterInfo {
  currentVersion: string;
  channel: string;
  platform: string;
  arch: string;
  homepage: string;
  mirrors: UpdateMirror[];
  installMode: "install" | "download-only";
  installModeLabel: string;
  signatureConfigured: boolean;
  signatureNote: string;
}

export interface PendingUpdate {
  version: string;
  tag: string;
  assetName: string;
  path: string;
  size: number;
  sha256: string;
  sourceId: string;
  sourceLabel: string;
  signatureVerified: boolean;
  installable: boolean;
  installBlockReason: string | null;
  downloadedAt: number;
}

export interface UpdateDownloadResult {
  pending: PendingUpdate;
  attempts: UpdateSourceAttempt[];
  signatureVerified: boolean;
  installable: boolean;
  installBlockReason: string | null;
}

export interface UpdateInstallResult {
  launched: boolean;
  method: string;
  message: string;
}

export interface UpdateProgress {
  phase: "download" | "retry" | "done" | string;
  sourceId: string;
  sourceLabel: string;
  sourceIndex: number;
  sourceTotal: number;
  received: number;
  total: number;
  percent: number;
  message: string;
}

export const api = {
  flavorInfo: () => invoke<FlavorInfo>("flavor_info"),
  resolveTools: (customFfmpeg?: string) =>
    invoke<ToolsInfo>("resolve_tools", { customFfmpeg: customFfmpeg ?? null }),
  probeCapabilities: (customFfmpeg: string | undefined, allowGpl: boolean, force: boolean) =>
    invoke<Capabilities>("probe_capabilities", { customFfmpeg: customFfmpeg ?? null, allowGpl, force }),
  probeFiles: (paths: string[]) => invoke<MediaInfo[]>("probe_files", { paths }),
  listDevicePresets: () => invoke<DevicePreset[]>("list_device_presets"),
  /** 抽取视频某时间点的预览帧，返回 PNG data URL（裁剪面板框选用）。 */
  previewFrame: (path: string, timeSec?: number, maxEdge?: number) =>
    invoke<string>("preview_frame", {
      path,
      timeSec: timeSec ?? null,
      maxEdge: maxEdge ?? null,
    }),
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
  toolsPlan: () => invoke<ToolsPlan>("tools_download_plan"),
  downloadTool: (packageId: string, sourceId?: string) =>
    invoke<LandedTool>("download_tool", { packageId, sourceId: sourceId ?? null }),
  cancelToolDownload: () => invoke<void>("cancel_tool_download"),
  writeExif: (files: string[], fields: ExifFields, outputDir?: string) =>
    invoke<ExifWriteResult[]>("write_exif", { files, fields, outputDir: outputDir ?? null }),
  encodeAvif: (files: string[], params: AvifParams, outputDir?: string) =>
    invoke<AvifResult[]>("encode_avif", { files, params, outputDir: outputDir ?? null }),
  updateInfo: () => invoke<UpdaterInfo>("update_info"),
  updateCheck: (preferred?: string) =>
    invoke<UpdateCheckResult>("update_check", { preferred: preferred ?? null }),
  updatePending: () => invoke<PendingUpdate | null>("update_pending"),
  updateDownload: (candidate: UpdateCandidate, preferred?: string) =>
    invoke<UpdateDownloadResult>("update_download", {
      version: candidate.version,
      tag: candidate.tag,
      assetName: candidate.assetName,
      assetSize: candidate.assetSize,
      assetSha256: candidate.assetSha256,
      assetUrl: candidate.assetUrl,
      sumsUrl: candidate.sumsUrl,
      signatureUrl: candidate.signatureUrl,
      preferred: preferred ?? null,
    }),
  updateInstall: () => invoke<UpdateInstallResult>("update_install"),
  openHomepage: () => invoke<void>("open_homepage"),
};
