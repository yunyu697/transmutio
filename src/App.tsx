import { useCallback, useEffect, useMemo, useRef, useState, useSyncExternalStore } from "react";
import { getVersion } from "@tauri-apps/api/app";
import { createTaskQueue } from "@formatforge/task-queue";
import { api, type Capabilities, type FlavorInfo, type Job, type ToolsInfo } from "./lib/api";
import { settingsStore, syncTheme, type Settings } from "./lib/settings";
import { createTauriTransport } from "./lib/tauriTransport";
import { crashReporter, logger } from "./lib/telemetry";
import { findMethod, sections, type MethodDef, type SectionDef } from "./catalog";
import { SideNav } from "./components/SideNav";
import { MethodGrid } from "./components/MethodGrid";
import { TaskCenter, QueueBar } from "./components/TaskCenter";
import { SettingsSheet } from "./components/SettingsSheet";
import { SetupWizard } from "./components/SetupWizard";
import { VideoPanel } from "./panels/VideoPanel";
import { AudioPanel } from "./panels/AudioPanel";
import { ImagePanel } from "./panels/ImagePanel";
import { AvifPanel, CollagePanel, ExifPanel, FramesPanel, SequencePanel } from "./panels/AdvancedPanels";
import { applyPreset, lastPresetFor, panelKeyFor } from "./lib/presets";
import { readNamespaced, writeNamespaced } from "./lib/storage";

type ToastKind = "ok" | "error" | "info";
interface Toast {
  id: number;
  text: string;
  kind: ToastKind;
  action?: { label: string; run: () => void };
}

const DEFAULT_SECTION = sections[0];

export default function App() {
  const [settings, setSettings] = useState<Readonly<Settings>>(() => settingsStore.get());
  const [activeSection, setActiveSection] = useState<string>(() => {
    const saved = settingsStore.get().lastSection;
    return sections.some((section) => section.id === saved) ? saved : DEFAULT_SECTION.id;
  });
  const [activeMethod, setActiveMethod] = useState<MethodDef | null>(null);
  const [search, setSearch] = useState("");
  const [tasksOpen, setTasksOpen] = useState(() => settingsStore.get().taskDrawerOpen);
  const [settingsOpen, setSettingsOpen] = useState(false);
  const [tools, setTools] = useState<ToolsInfo | null>(null);
  const [caps, setCaps] = useState<Capabilities | null>(null);
  const [flavor, setFlavor] = useState<FlavorInfo | null>(null);
  const [appVersion, setAppVersion] = useState("");
  const [probing, setProbing] = useState(false);
  /** 首次能力探测是否已有结果。未完成前不渲染引导页，避免每次启动先闪一下引导页再跳主界面。 */
  const [bootDone, setBootDone] = useState(false);
  const [toasts, setToasts] = useState<Toast[]>([]);
  const toastSeq = useRef(0);
  /** 应用预设后递增，用于重建面板以读取新的 localStorage 参数。 */
  const [presetNonce, setPresetNonce] = useState(0);

  const taskQueue = useMemo(() => createTaskQueue(createTauriTransport()), []);
  const taskState = useSyncExternalStore(
    (listener) => taskQueue.subscribe(listener),
    () => taskQueue.getState(),
  );

  const toast = useCallback(
    (text: string, kind: ToastKind = "info", action?: Toast["action"]) => {
      const id = ++toastSeq.current;
      setToasts((prev) => [...prev.slice(-2), { id, text, kind, action }]);
      window.setTimeout(
        () => {
          setToasts((prev) => prev.filter((item) => item.id !== id));
        },
        kind === "error" ? 6000 : action ? 8000 : 4000,
      );
    },
    [],
  );

  const patchSettings = useCallback((patch: Partial<Settings>) => {
    settingsStore.set(patch);
    setSettings(settingsStore.get());
  }, []);

  // 主题与设置同步（syncTheme 内部同时处理 CSS 变量与原生标题栏）
  useEffect(() => {
    syncTheme(settings.theme);
  }, [settings.theme]);

  useEffect(() => {
    const unsubscribe = settingsStore.subscribe((value) => setSettings(value));
    return unsubscribe;
  }, []);

  // 并发数写回后端
  useEffect(() => {
    void api.setConcurrency(settings.concurrency).catch((error) => {
      logger.warn("setConcurrency failed", { error: String(error) });
    });
  }, [settings.concurrency]);

  // 队列级暂停写回后端：暂停后不再调度新任务，运行中的任务会继续跑完。
  useEffect(() => {
    void api.setPaused(settings.queuePaused).catch((error) => {
      logger.warn("setPaused failed", { error: String(error) });
    });
  }, [settings.queuePaused]);

  // 批次完成动作写回后端（none / open-folder / shutdown）。
  useEffect(() => {
    void api.setAfterAction(settings.afterAction).catch((error) => {
      logger.warn("setAfterAction failed", { error: String(error) });
    });
  }, [settings.afterAction]);

  // 崩溃上报挂载
  useEffect(() => {
    const uninstall = crashReporter.install();
    logger.info("app boot", { platform: navigator.platform, language: navigator.language });
    return uninstall;
  }, []);

  useEffect(() => {
    void getVersion().then((value) => setAppVersion(value || "")).catch(() => setAppVersion(""));
  }, []);

  // 面板里应用预设后重建面板，让 useState 初始化读到新参数。
  useEffect(() => {
    const onApplied = () => setPresetNonce((value) => value + 1);
    window.addEventListener("transmutio:preset-applied", onApplied);
    return () => window.removeEventListener("transmutio:preset-applied", onApplied);
  }, []);

  const bootTools = useCallback(
    async (customPath: string, force: boolean) => {
      setProbing(true);
      try {
        const info = await api.resolveTools(customPath || undefined);
        setTools(info);
        // 工具路径一确定就放开首屏：是否进引导页只取决于 tools，
        // 编码器探测可能很慢（首次要试编），不能压着首屏一起等。
        setBootDone(true);
        const capabilities = await api.probeCapabilities(customPath || undefined, settingsStore.get().allowGplEncoders, force);
        setCaps(capabilities);
      } catch (error) {
        const message = error instanceof Error ? error.message : String(error);
        logger.error("tools probe failed", { message });
        toast(`初始化失败：${message}`, "error");
        setBootDone(true);
      } finally {
        setProbing(false);
      }
    },
    [toast],
  );

  useEffect(() => {
    void api.flavorInfo()
      .then(setFlavor)
      .catch((error) => logger.warn("flavor info failed", { error: String(error) }));
  }, []);

  useEffect(() => {
    void bootTools(settingsStore.get().ffmpegPath, false);
    // 仅启动时执行一次；后续通过「重新探测」或设置页应用路径触发
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  const reprobe = useCallback(
    (force: boolean) => {
      void bootTools(settingsStore.get().ffmpegPath, force).then(() => toast("已重新探测硬件编码能力", "ok"));
    },
    [bootTools, toast],
  );

  const currentSection = sections.find((section) => section.id === activeSection) ?? DEFAULT_SECTION;
  const query = search.trim().toLowerCase();

  const visibleMethods = useMemo(() => {
    if (!query) return currentSection.methods;
    const all = sections.flatMap((section) =>
      section.methods.filter(
        (method) => method.title.toLowerCase().includes(query) || method.desc.toLowerCase().includes(query),
      ),
    );
    return all;
  }, [currentSection, query]);

  const selectSection = (section: SectionDef) => {
    setActiveSection(section.id);
    setActiveMethod(null);
    setSearch("");
    patchSettings({ lastSection: section.id });
  };

  /** 点品牌名 / 图标：回到当前分区的方法列表（入口已合并，不再有独立首页）。 */
  const goRoot = () => {
    setActiveMethod(null);
    setSearch("");
  };

  const pickMethod = (method: MethodDef) => {
    if (!method.implemented) {
      toast(`「${method.title}」本版尚未实现，已保留入口但不会执行转换。`, "info");
      return;
    }
    setActiveSection(method.sectionId);
    setSearch("");
    patchSettings({ lastSection: method.sectionId });

    // 启动默认载入该「方法」上次用过的预设；带撤销，避免覆盖刚调好的参数。
    const preset = lastPresetFor(method.id);
    if (preset) {
      const key = panelKeyFor(method.id);
      const before = key ? readNamespaced("panel:" + key) : null;
      applyPreset(preset);
      setPresetNonce((value) => value + 1);
      toast(`已载入上次预设「${preset.name}」`, "info", {
        label: "撤销",
        run: () => {
          if (!key) return;
          if (before === null) writeNamespaced("panel:" + key, "{}");
          else writeNamespaced("panel:" + key, before);
          setPresetNonce((value) => value + 1);
        },
      });
    }
    setActiveMethod(method);
  };

  const enqueued = (jobs: Job[]) => {
    jobs.forEach((job) => taskQueue.track(job));
    void taskQueue.refresh();
  };

  const toggleTasks = () => {
    const next = !tasksOpen;
    setTasksOpen(next);
    patchSettings({ taskDrawerOpen: next });
  };

  const ffmpegReady = Boolean(tools?.ffmpeg?.exists);
  // GPL 变体编译期固定 allowGpl=true；LGPL 版仍看本机设置（release 下无效）。
  const effectiveAllowGpl = Boolean(flavor?.id === "gpl" || settings.allowGplEncoders);
  const gplBlocked = Boolean(tools && !tools.build.lgplOk && !effectiveAllowGpl);
  // GPL 版必须配 GPL 构建：当前是 LGPL 构建时同样进入引导页。
  const gplBuildMissing = Boolean(
    flavor?.requireGplBuild && tools?.ffmpeg?.exists && tools.build.lgplOk && !tools.build.nonfree,
  );
  const needsSetup = !ffmpegReady || gplBlocked || gplBuildMissing;
  const running = taskState.stats.running + taskState.stats.queued;

  // 输出位置：默认写回源目录（后端以 @source 解析），自定义时使用表单路径。
  const outputDir = settings.outputDirMode === "custom" ? settings.outputDir : "@source";

  // 视图标识：变化时重建容器并重放一次入场动画（搜索输入不参与，避免每敲一个字闪一次）。
  const viewKey = activeMethod ? `method:${activeMethod.id}` : query ? "search" : `section:${activeSection}`;

  const renderPanel = () => {
    if (!activeMethod) {
      if (query) {
        return (
          <MethodGrid
            title={`搜索：${search}`}
            subtitle={`在所有分区中找到 ${visibleMethods.length} 个方法`}
            methods={visibleMethods}
            onPick={pickMethod}
            onUnimplemented={pickMethod}
          />
        );
      }
      return (
        <MethodGrid
          title={currentSection.name}
          subtitle={currentSection.hint}
          methods={currentSection.methods}
          onPick={pickMethod}
          onUnimplemented={pickMethod}
        />
      );
    }
    const common = {
      outputDir,
      ffmpegReady,
      onChangeOutputDir: (dir: string) =>
        patchSettings({ outputDir: dir, outputDirMode: "custom" }),
      onEnqueued: enqueued,
      toast,
      onBack: () => setActiveMethod(null),
    };
    const panelKey = `${activeMethod.id}:${presetNonce}`;
    if (activeMethod.panel === "video")
      return <VideoPanel key={panelKey} methodId={activeMethod.id} caps={caps} {...common} />;
    if (activeMethod.panel === "audio")
      return <AudioPanel key={panelKey} methodId={activeMethod.id} caps={caps} {...common} />;
    if (activeMethod.panel === "image") return <ImagePanel key={panelKey} methodId={activeMethod.id} {...common} />;
    if (activeMethod.panel === "sequence")
      return <SequencePanel key={panelKey} methodId={activeMethod.id} {...common} />;
    if (activeMethod.panel === "frames") return <FramesPanel key={panelKey} methodId={activeMethod.id} {...common} />;
    if (activeMethod.panel === "collage")
      return <CollagePanel key={panelKey} methodId={activeMethod.id} {...common} />;
    if (activeMethod.panel === "exif") return <ExifPanel key={panelKey} methodId={activeMethod.id} {...common} />;
    if (activeMethod.panel === "avif") return <AvifPanel key={panelKey} methodId={activeMethod.id} {...common} />;
    return null;
  };

  const softEncoders = caps
    ? caps.videoEncoders
        .filter((encoder) => encoder.available && encoder.allowed && !encoder.hardware)
        .map((encoder) => encoder.id)
        .join(" / ") || "无"
    : "—";
  const hardEncoders = caps
    ? caps.videoEncoders.filter((encoder) => encoder.available && encoder.allowed && encoder.hardware).length
    : 0;

  return (
    <div className="app">
      <div className="body">
        {!bootDone ? (
          // 首次探测未完成：只铺应用底色，不渲染引导页（否则每次启动都会先闪一下引导页再跳主界面）。
          <div className="app-boot" aria-hidden="true" />
        ) : needsSetup ? (
          <SetupWizard
            tools={tools}
            flavor={flavor}
            settings={settings}
            probing={probing}
            onPatch={patchSettings}
            onReprobe={() => void bootTools(settingsStore.get().ffmpegPath, true)}
            onInstalled={(info) => {
              const path = info.ffmpeg?.path ?? "";
              patchSettings({ ffmpegPath: path });
              void bootTools(path, true);
            }}
            onDevOverride={() => {
              patchSettings({ allowGplEncoders: true });
              void bootTools(settingsStore.get().ffmpegPath, true);
            }}
            toast={toast}
          />
        ) : (
          <>
            <SideNav
              activeSection={activeSection}
              activeMethodId={activeMethod?.id ?? null}
              collapsed={settings.navCollapsed}
              search={search}
              outputDir={settings.outputDir}
              outputDirMode={settings.outputDirMode}
              taskCount={running}
              tasksOpen={tasksOpen}
              theme={settings.theme}
              onSelectSection={selectSection}
              onGoRoot={goRoot}
              onSearch={setSearch}
              onToggleCollapsed={() => patchSettings({ navCollapsed: !settings.navCollapsed })}
              onToggleTasks={toggleTasks}
              onToggleTheme={() => patchSettings({ theme: settings.theme === "dark" ? "light" : "dark" })}
              onOpenSettings={() => setSettingsOpen(true)}
            />
            <main className="main">
              <div className="view" key={viewKey}>
                {renderPanel()}
              </div>
            </main>
          </>
        )}
      </div>

      {bootDone && !needsSetup ? (
        <div className="queue-slot">
          {tasksOpen ? (
            <TaskCenter
              tasks={taskState.tasks}
              stats={taskState.stats}
              concurrency={settings.concurrency}
              onConcurrency={(value) => patchSettings({ concurrency: value })}
              paused={settings.queuePaused}
              onPause={(value) => patchSettings({ queuePaused: value })}
              onCancel={(id) => {
                void taskQueue.cancel(id).catch((error) => toast(`取消失败：${error}`, "error"));
              }}
              onRetry={(id) => {
                void taskQueue.retry(id).catch((error) => toast(`重试失败：${error}`, "error"));
              }}
              onOpenOutput={(id) => {
                void taskQueue.openOutput(id).catch((error) => toast(`打开目录失败：${error}`, "error"));
              }}
              onOpenLog={(id) => {
                void api.openJobLog(id).catch((error) => toast(`打开日志失败：${error}`, "error"));
              }}
              onClearFinished={() => {
                void api
                  .removeFinished()
                  .then(() => taskQueue.refresh())
                  .catch((error) => toast(`清理失败：${error}`, "error"));
              }}
              onClose={() => {
                setTasksOpen(false);
                patchSettings({ taskDrawerOpen: false });
              }}
            />
          ) : null}
          <QueueBar
            stats={taskState.stats}
            tasks={taskState.tasks}
            paused={settings.queuePaused}
            open={tasksOpen}
            onToggle={toggleTasks}
            onPause={(value) => patchSettings({ queuePaused: value })}
          />
        </div>
      ) : null}

      <div className="statusbar">
        <span>
          <span className={`dot ${ffmpegReady ? "" : "bad"}`} />
          {ffmpegReady ? `FFmpeg ${tools?.ffmpegVersion ?? "已就绪"}` : "未找到 FFmpeg"}
        </span>
        <span
          className={`flavor-tag ${flavor?.id === "gpl" ? "gpl" : ""}`}
          title={
            flavor
              ? `${flavor.productName} · ${flavor.licenseLabel} · 更新通道 ${flavor.updateChannel}`
              : "变体信息不可用"
          }
        >
          {flavor ? (flavor.id === "gpl" ? `${flavor.productName} · GPL` : `${flavor.productName} · LGPL`) : "变体未知"}
          <span
            className="flavor-build"
            style={tools?.build.nonfree ? { color: "var(--ff-warn)" } : undefined}
          >
            {tools
              ? ` · FFmpeg 构建：${tools.build.nonfree ? "nonfree" : tools.build.gpl ? "GPL" : "LGPL"}${
                  tools.build.gpl ? "（含 libx264/libx265）" : "（无 libx264/libx265）"
                }`
              : " · FFmpeg 构建：未知"}
          </span>
        </span>
        <span>
          软编：{softEncoders} · 硬编：{hardEncoders} 个可用
        </span>
        <span className="spacer" />
        <span>
          并发 {settings.concurrency} · 运行 {taskState.stats.running} · 排队 {taskState.stats.queued}
        </span>
        <span>
          {appVersion ? `v${appVersion} · ` : ""}
          {tools?.platform ?? "—"}/{tools?.arch ?? "—"}
        </span>
      </div>

      <SettingsSheet
        open={settingsOpen}
        settings={settings}
        flavor={flavor}
        tools={tools}
        caps={caps}
        probing={probing}
        onClose={() => setSettingsOpen(false)}
        onPatch={(patch) => {
          patchSettings(patch);
          if (patch.allowGplEncoders !== undefined) {
            void bootTools(settingsStore.get().ffmpegPath, true);
          }
        }}
        onApplyPath={(path) => {
          patchSettings({ ffmpegPath: path });
          void bootTools(path, true);
        }}
        onReprobe={() => reprobe(true)}
        onExportDiagnostics={async () => {
          const { exportDiagnostics } = await import("./lib/telemetry");
          return exportDiagnostics();
        }}
        toast={toast}
      />

      {toasts.length ? (
        <div className="toasts">
          {toasts.map((item) => (
            <div key={item.id} className={`toast ${item.kind === "info" ? "" : item.kind}`}>
              <span>{item.text}</span>
              {item.action ? (
                <button
                  type="button"
                  className="toast-action"
                  onClick={() => {
                    item.action?.run();
                    setToasts((prev) => prev.filter((toastItem) => toastItem.id !== item.id));
                  }}
                >
                  {item.action.label}
                </button>
              ) : null}
            </div>
          ))}
        </div>
      ) : null}
    </div>
  );
}
