import { useEffect, useMemo, useState } from "react";
import { formatDuration } from "@formatforge/design-tokens";
import { api, type GifParams, type Job, type MergeParams, type MuxParams } from "../lib/api";
import { DropZone, Field, FileTable, Group, OutputNaming, Segmented, SwitchRow, useMediaFiles } from "./shared";
import { loadPanelPrefs, savePanelPrefs } from "../lib/panelPrefs";
import { Icon } from "../components/Icon";

export interface CompositePanelProps {
  methodId: string;
  outputDir: string;
  ffmpegReady: boolean;
  onChangeOutputDir: (dir: string) => void;
  onEnqueued: (jobs: Job[]) => void;
  toast: (text: string, kind?: "ok" | "error" | "info") => void;
  onBack: () => void;
}

interface Naming {
  template: string;
  conflict: "suffix" | "skip" | "overwrite";
}

const DEFAULT_NAMING: Naming = { template: "", conflict: "suffix" };

function useNaming(methodId: string) {
  const [naming, setNaming] = useState<Naming>(() => loadPanelPrefs(`naming:${methodId}`, DEFAULT_NAMING));
  useEffect(() => savePanelPrefs(`naming:${methodId}`, naming), [methodId, naming]);
  return [naming, setNaming] as const;
}

function namingSpec(naming: Naming) {
  return {
    nameTemplate: naming.template.trim() || undefined,
    conflict: naming.conflict,
  };
}

async function pickDir(onChangeOutputDir: (dir: string) => void) {
  const picked = await import("@tauri-apps/plugin-dialog").then((m) =>
    m.open({ directory: true, multiple: false }),
  );
  if (typeof picked === "string") onChangeOutputDir(picked);
}

function BottomBar({
  outputDir,
  onChangeOutputDir,
  hint,
  canStart,
  starting,
  onStart,
}: {
  outputDir: string;
  onChangeOutputDir: (dir: string) => void;
  hint: React.ReactNode;
  canStart: boolean;
  starting: boolean;
  onStart: () => void;
}) {
  return (
    <div className="bottombar">
      <div className="outdir">
        <Icon name="folder" size={14} />
        <code title={outputDir}>{outputDir || "未设置输出目录"}</code>
        <button className="btn small ghost" onClick={() => pickDir(onChangeOutputDir)}>
          更改
        </button>
      </div>
      <div style={{ fontSize: 10.5, color: "var(--ff-fg3)" }}>{hint}</div>
      <button className="btn primary" onClick={onStart} disabled={starting || !canStart}>
        <Icon name="play" size={14} />
        {starting ? "正在入队…" : "开始处理"}
      </button>
    </div>
  );
}

// ---------------------------------------------------------------------------

export function MergePanel({ methodId, outputDir, ffmpegReady, onChangeOutputDir, onEnqueued, toast, onBack }: CompositePanelProps) {
  const { files, busy, notice, pick, remove, clear } = useMediaFiles("video");
  const defaults: MergeParams = {
    mode: "auto",
    resolution: "source",
    fps: "source",
    audioMode: "aac",
    codec: "h264",
    quality: 23,
    bitrateKbps: 0,
  };
  const [params, setParams] = useState<MergeParams>(() => loadPanelPrefs(`video-${methodId}`, defaults));
  const [naming, setNaming] = useNaming(methodId);
  const [starting, setStarting] = useState(false);
  useEffect(() => savePanelPrefs(`video-${methodId}`, params), [methodId, params]);

  const set = <K extends keyof MergeParams>(key: K, value: MergeParams[K]) =>
    setParams((prev) => ({ ...prev, [key]: value }));

  const usable = files.filter((file) => !file.error);
  const duration = usable.reduce((sum, file) => sum + (file.durationSec ?? 0), 0);
  const needTranscode = params.mode !== "copy";

  const start = async () => {
    if (usable.length < 2) {
      toast("合并至少需要 2 个视频文件（顺序决定拼接顺序）", "error");
      return;
    }
    if (!ffmpegReady) {
      toast("未找到 ffmpeg，无法开始合并", "error");
      return;
    }
    setStarting(true);
    try {
      const [first, ...rest] = usable;
      const jobs = await api.enqueue([
        {
          kind: "video-merge",
          input: first.path,
          inputs: rest.map((file) => file.path),
          outputDir,
          params: { ...params },
          ...namingSpec(naming),
        },
      ]);
      onEnqueued(jobs);
      toast(`已加入合并任务（${usable.length} 个输入）`, "ok");
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
            <h1>视频合并</h1>
            <p>按列表顺序拼接 · 同参直接复制流，不同参先统一转码 · 绝不覆盖原文件</p>
          </div>
          <button className="btn" style={{ marginLeft: "auto" }} onClick={pick} disabled={busy}>
            <Icon name="plus" size={14} />
            {busy ? "读取中…" : "添加文件"}
          </button>
        </div>
        <DropZone kind="video" busy={busy} onPick={pick} />
        {notice ? <div className="note warn" style={{ margin: "10px 20px 0" }}>{notice}</div> : null}
        <div className="note" style={{ margin: "10px 20px 0" }}>
          文件按添加顺序拼接；需要调整顺序请移除后重新添加。至少需要 2 个文件。
        </div>
        <FileTable files={files} onRemove={remove} onClear={clear} />
      </div>

      <aside className="inspector">
        <div className="inspector-head">
          <Icon name="settings" size={15} />
          <b>合并参数</b>
          <span className="sub">{usable.length} 个输入</span>
        </div>
        <div className="inspector-body">
          <Group title="拼接方式" icon="film">
            <Field label="模式">
              <Segmented
                value={params.mode}
                onChange={(value) => set("mode", value)}
                options={[
                  { value: "auto", label: "自动" },
                  { value: "copy", label: "直接拼接" },
                  { value: "transcode", label: "统一转码" },
                ]}
              />
            </Field>
            <div className="note">
              自动：编码/分辨率/帧率/音轨参数完全一致时直接复制流（最快、无损）；任一项不同则先统一转码再拼接。
              直接拼接遇到不同参会明确拒绝，不会产出损坏文件。
            </div>
          </Group>

          {needTranscode ? (
            <Group title="统一转码" icon="compress">
              <div className="row2">
                <Field label="目标分辨率">
                  <select className="select" value={params.resolution} onChange={(e) => set("resolution", e.target.value as MergeParams["resolution"])}>
                    <option value="source">按第一个文件</option>
                    <option value="2160">2160p</option>
                    <option value="1440">1440p</option>
                    <option value="1080">1080p</option>
                    <option value="720">720p</option>
                    <option value="480">480p</option>
                  </select>
                </Field>
                <Field label="帧率">
                  <select className="select" value={params.fps} onChange={(e) => set("fps", e.target.value as MergeParams["fps"])}>
                    <option value="source">按第一个文件</option>
                    <option value="30">30 fps</option>
                    <option value="60">60 fps</option>
                  </select>
                </Field>
              </div>
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
              <Field label="音轨">
                <Segmented
                  value={params.audioMode}
                  onChange={(value) => set("audioMode", value)}
                  options={[
                    { value: "aac", label: "转 AAC" },
                    { value: "none", label: "移除" },
                  ]}
                />
              </Field>
            </Group>
          ) : (
            <Group title="统一转码" icon="compress">
              <div className="note">「直接拼接」不重新编码，分辨率/帧率/质量设置不生效。</div>
            </Group>
          )}

          <OutputNaming naming={naming} onChange={setNaming} />
        </div>
      </aside>

      <BottomBar
        outputDir={outputDir}
        onChangeOutputDir={onChangeOutputDir}
        hint={
          usable.length >= 2 && duration
            ? `预计输出时长 ${formatDuration(duration)}`
            : "至少选择 2 个文件后显示时长"
        }
        canStart={usable.length >= 2}
        starting={starting}
        onStart={start}
      />
    </div>
  );
}

// ---------------------------------------------------------------------------

export function MuxPanel({ methodId, outputDir, ffmpegReady, onChangeOutputDir, onEnqueued, toast, onBack }: CompositePanelProps) {
  const video = useMediaFiles("video");
  const audio = useMediaFiles("audio");
  const defaults: MuxParams = { mode: "replace", audioMode: "aac", audioBitrateKbps: 192 };
  const [params, setParams] = useState<MuxParams>(() => loadPanelPrefs(`video-${methodId}`, defaults));
  const [naming, setNaming] = useNaming(methodId);
  const [starting, setStarting] = useState(false);
  useEffect(() => savePanelPrefs(`video-${methodId}`, params), [methodId, params]);

  const set = <K extends keyof MuxParams>(key: K, value: MuxParams[K]) =>
    setParams((prev) => ({ ...prev, [key]: value }));

  const videoFile = video.files.find((file) => !file.error);
  const audioFile = audio.files.find((file) => !file.error);

  const start = async () => {
    if (!videoFile) {
      toast("请选择视频文件", "error");
      return;
    }
    if (!audioFile) {
      toast("请选择要使用的音频文件", "error");
      return;
    }
    if (!ffmpegReady) {
      toast("未找到 ffmpeg，无法开始混流", "error");
      return;
    }
    setStarting(true);
    try {
      const jobs = await api.enqueue([
        {
          kind: "video-mux",
          input: videoFile.path,
          inputs: [audioFile.path],
          outputDir,
          params: { ...params },
          ...namingSpec(naming),
        },
      ]);
      onEnqueued(jobs);
      toast("已加入混流任务", "ok");
      video.clear();
      audio.clear();
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
            <h1>替换 / 添加音轨</h1>
            <p>视频流原样复制（不重编码） · 音频替换或追加 · 输出时长以视频为准</p>
          </div>
        </div>

        <div style={{ padding: "0 20px" }}>
          <div className="note" style={{ marginBottom: 8 }}>视频（画面来源，流复制）</div>
        </div>
        <DropZone kind="video" busy={video.busy} onPick={video.pick} />
        {video.notice ? <div className="note warn" style={{ margin: "10px 20px 0" }}>{video.notice}</div> : null}
        <FileTable files={video.files} onRemove={video.remove} onClear={video.clear} />

        <div style={{ padding: "0 20px" }}>
          <div className="note" style={{ margin: "14px 0 8px" }}>音频（音轨来源）</div>
        </div>
        <DropZone kind="audio" busy={audio.busy} onPick={audio.pick} />
        {audio.notice ? <div className="note warn" style={{ margin: "10px 20px 0" }}>{audio.notice}</div> : null}
        <FileTable files={audio.files} onRemove={audio.remove} onClear={audio.clear} />
      </div>

      <aside className="inspector">
        <div className="inspector-head">
          <Icon name="settings" size={15} />
          <b>混流参数</b>
          <span className="sub">{videoFile ? "已选视频" : "未选视频"}</span>
        </div>
        <div className="inspector-body">
          <Group title="音轨处理" icon="wave">
            <Field label="方式">
              <Segmented
                value={params.mode}
                onChange={(value) => set("mode", value)}
                options={[
                  { value: "replace", label: "替换原音轨" },
                  { value: "add", label: "追加音轨" },
                ]}
              />
            </Field>
            <div className="note">
              替换：丢弃视频原音轨，只保留新音频；追加：保留原音轨（若有）并新增一条。视频流始终直接复制。
            </div>
          </Group>

          <Group title="音频编码" icon="wave">
            <Field label="编码">
              <Segmented
                value={params.audioMode}
                onChange={(value) => set("audioMode", value)}
                options={[
                  { value: "aac", label: "转 AAC" },
                  { value: "copy", label: "原样复制" },
                ]}
              />
            </Field>
            {params.audioMode === "aac" ? (
              <Field label="音频码率">
                <select className="select" value={params.audioBitrateKbps} onChange={(e) => set("audioBitrateKbps", Number(e.target.value))}>
                  <option value={96}>96 kbps</option>
                  <option value={128}>128 kbps</option>
                  <option value={192}>192 kbps</option>
                  <option value={256}>256 kbps</option>
                </select>
              </Field>
            ) : (
              <div className="note">
                原样复制要求编码能被 MP4 直接容纳（AAC/MP3/AC3/Opus 等）；PCM、FLAC 等会被明确拒绝并提示改用 AAC。
              </div>
            )}
            <div className="note">音频比视频长时按视频时长截断；比视频短时尾部无声音，不会补静音。</div>
          </Group>

          <OutputNaming naming={naming} onChange={setNaming} />
        </div>
      </aside>

      <BottomBar
        outputDir={outputDir}
        onChangeOutputDir={onChangeOutputDir}
        hint={videoFile?.durationSec ? `视频时长 ${formatDuration(videoFile.durationSec)}` : "选择视频后显示时长"}
        canStart={Boolean(videoFile && audioFile)}
        starting={starting}
        onStart={start}
      />
    </div>
  );
}

// ---------------------------------------------------------------------------

export function GifPanel({ methodId, outputDir, ffmpegReady, onChangeOutputDir, onEnqueued, toast, onBack }: CompositePanelProps) {
  const { files, busy, notice, pick, remove, clear } = useMediaFiles("video");
  const defaults: GifParams = { fps: 10, width: 480, startSec: 0, durationSec: 0, optimize: true };
  const [params, setParams] = useState<GifParams>(() => loadPanelPrefs(`video-${methodId}`, defaults));
  const [naming, setNaming] = useNaming(methodId);
  const [starting, setStarting] = useState(false);
  useEffect(() => savePanelPrefs(`video-${methodId}`, params), [methodId, params]);

  const set = <K extends keyof GifParams>(key: K, value: GifParams[K]) =>
    setParams((prev) => ({ ...prev, [key]: value }));

  const usable = files.filter((file) => !file.error);
  const totalDuration = useMemo(
    () => usable.reduce((sum, file) => sum + (file.durationSec ?? 0), 0),
    [usable],
  );
  const perFileDuration = params.durationSec > 0 ? params.durationSec : null;

  const start = async () => {
    if (!usable.length) {
      toast("请先添加至少一个视频文件", "error");
      return;
    }
    if (!ffmpegReady) {
      toast("未找到 ffmpeg，无法生成 GIF", "error");
      return;
    }
    setStarting(true);
    try {
      const specs = usable.map((file) => ({
        kind: "video-gif" as const,
        input: file.path,
        outputDir,
        params: { ...params },
        ...namingSpec(naming),
      }));
      const jobs = await api.enqueue(specs);
      onEnqueued(jobs);
      toast(`已加入 ${jobs.length} 个 GIF 任务`, "ok");
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
            <h1>视频转 GIF</h1>
            <p>可配置帧率 / 宽度 / 起点 / 时长 · 可选调色板优化 · 绝不覆盖原文件</p>
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
          <b>GIF 参数</b>
          <span className="sub">{usable.length} 个文件</span>
        </div>
        <div className="inspector-body">
          <Group title="帧与尺寸" icon="film">
            <Field label="帧率（fps）">
              <select className="select" value={params.fps} onChange={(e) => set("fps", Number(e.target.value))}>
                {[5, 10, 12, 15, 20, 24, 30].map((fps) => (
                  <option key={fps} value={fps}>{fps} fps</option>
                ))}
              </select>
            </Field>
            <Field label="宽度（高度按比例）">
              <select className="select" value={params.width} onChange={(e) => set("width", Number(e.target.value))}>
                <option value={0}>保持源宽度</option>
                <option value={320}>320 px</option>
                <option value={480}>480 px</option>
                <option value={640}>640 px</option>
                <option value={800}>800 px</option>
                <option value={1080}>1080 px</option>
              </select>
            </Field>
          </Group>

          <Group title="截取区间" icon="crop">
            <div className="row2">
              <Field label="开始时间（秒）">
                <input className="input" type="number" min={0} step={0.1} value={params.startSec} onChange={(e) => set("startSec", Math.max(0, Number(e.target.value) || 0))} />
              </Field>
              <Field label="时长（秒，0 = 到片尾）">
                <input className="input" type="number" min={0} step={0.1} value={params.durationSec} onChange={(e) => set("durationSec", Math.max(0, Number(e.target.value) || 0))} />
              </Field>
            </div>
          </Group>

          <Group title="质量" icon="compress">
            <SwitchRow
              label="调色板优化"
              hint={params.optimize ? "palettegen + paletteuse：颜色更接近源视频，首帧略慢" : "直接转码：更快，但渐变/肤色容易出现色带"}
              on={params.optimize}
              onChange={(value) => set("optimize", value)}
            />
            <div className="note">
              调色板优化会先统计全片颜色再量化到 256 色，文件通常更小且色带更少；直接转码只做逐帧量化，速度更快但色彩明显更差。
            </div>
          </Group>

          <OutputNaming naming={naming} onChange={setNaming} />
        </div>
      </aside>

      <BottomBar
        outputDir={outputDir}
        onChangeOutputDir={onChangeOutputDir}
        hint={
          totalDuration
            ? `源总时长 ${formatDuration(totalDuration)}${perFileDuration ? ` · 每个截取 ${perFileDuration}s` : ""}`
            : "添加文件后显示时长"
        }
        canStart={usable.length > 0}
        starting={starting}
        onStart={start}
      />
    </div>
  );
}
