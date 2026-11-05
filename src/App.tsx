import { useCallback, useEffect, useMemo, useRef, useState, useSyncExternalStore } from "react";
import { applyTheme } from "@formatforge/design-tokens";
import { createTaskQueue } from "@formatforge/task-queue";
import { api, type Capabilities, type FlavorInfo, type Job, type PdfToolsInfo, type ToolsInfo } from "./lib/api";
import { settingsStore, syncTheme, type Settings } from "./lib/settings";
import { createTauriTransport } from "./lib/tauriTransport";
import { crashReporter, logger } from "./lib/telemetry";
import { sections, type MethodDef, type SectionDef } from "./catalog";
import { TopBar } from "./components/TopBar";
import { SideNav } from "./components/SideNav";
import { MethodGrid } from "./components/MethodGrid";
import { TaskCenter } from "./components/TaskCenter";
import { SettingsSheet } from "./components/SettingsSheet";
import { SetupWizard } from "./components/SetupWizard";
import { VideoPanel } from "./panels/VideoPanel";
import { AudioPanel } from "./panels/AudioPanel";
import { ImagePanel } from "./panels/ImagePanel";
import { CollagePanel, FramesPanel, PdfPanel, SequencePanel } from "./panels/AdvancedPanels";
import { Icon } from "./components/Icon";

type ToastKind = "ok" | "error" | "info";
interface Toast {
  id: number;
  text: string;
  kind: ToastKind;
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
  const [tasksOpen, setTasksOpen] = useState(() => settingsStore.get().taskCenterOpen);
  const [settingsOpen, setSettingsOpen] = useState(false);
  const [tools, setTools] = useState<ToolsInfo | null>(null);
  const [caps, setCaps] = useState<Capabilities | null>(null);
  const [flavor, setFlavor] = useState<FlavorInfo | null>(null);
  const [pdfTools, setPdfTools] = useState<PdfToolsInfo | null>(null);
  const [probing, setProbing] = useState(false);
  const [toasts, setToasts] = useState<Toast[]>([]);
  const [migrationNotice, setMigrationNotice] = useState<string | null>(null);
  const toastSeq = useRef(0);

  const taskQueue = useMemo(() => createTaskQueue(createTauriTransport()), []);
  const taskState = useSyncExternalStore(
    (listener) => taskQueue.subscribe(listener),
    () => taskQueue.getState(),
  );

  const toast = useCallback((text: string, kind: ToastKind = "info") => {
    const id = ++toastSeq.current;
    setToasts((prev) => [...prev.slice(-2), { id, text, kind }]);
    window.setTimeout(() => {
      setToasts((prev) => prev.filter((item) => item.id !== id));
    }, kind === "error" ? 6000 : 4000);
  }, []);

  const patchSettings = useCallback((patch: Partial<Settings>) => {
    settingsStore.set(patch);
    setSettings(settingsStore.get());
  }, []);

  // 主题与设置同步
  useEffect(() => {
    syncTheme(settings.theme);
    applyTheme(settings.theme);
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

  // 旧版数据已迁移的一次性提示：关闭后写入 localStorage，不再重复弹出。
  useEffect(() => {
    const key = "transmutio:migration-notice-dismissed";
    if (localStorage.getItem(key) === "1") return;
    void api
      .migrationNotice()
      .then((info) => {
        if (info?.migrated) setMigrationNotice(info.detail);
      })
      .catch(() => {});
  }, []);

  const bootTools = useCallback(
    async (customPath: string, force: boolean) => {
      setProbing(true);
      try {
        const info = await api.resolveTools(customPath || undefined);
        setTools(info);
        const capabilities = await api.probeCapabilities(customPath || undefined, settingsStore.get().allowGplEncoders, force);
        setCaps(capabilities);
        if (!customPath) {
          const dir = settingsStore.get().outputDir || (await api.defaultOutputDir());
          if (dir && dir !== settingsStore.get().outputDir) patchSettings({ outputDir: dir });
        }
      } catch (error) {
        const message = error instanceof Error ? error.message : String(error);
        logger.error("tools probe failed", { message });
        toast(`初始化失败：${message}`, "error");
      } finally {
        setProbing(false);
      }
    },
    [patchSettings, toast],
  );

  useEffect(() => {
    void api.flavorInfo()
      .then(setFlavor)
      .catch((error) => logger.warn("flavor info failed", { error: String(error) }));
  }, []);

  // PDF 外部渲染器检测：缺失时入口仍可进入，面板内说明如何配置并支持重新检测。
  useEffect(() => {
    void api.pdfToolsStatus().then(setPdfTools).catch(() => setPdfTools(null));
  }, []);

  const redetectPdfTools = useCallback(async () => {
    try {
      const next = await api.pdfToolsStatus();
      setPdfTools(next);
      return next;
    } catch {
      setPdfTools(null);
      return null;
    }
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

  const pickMethod = (method: MethodDef) => {
    if (!method.implemented) {
      toast(`「${method.title}」本版尚未实现，已保留入口但不会执行转换。`, "info");
      return;
    }
    setActiveMethod(method);
  };

  const enqueued = (jobs: Job[]) => {
    jobs.forEach((job) => taskQueue.track(job));
    void taskQueue.refresh();
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

  const renderPanel = () => {
    if (!activeMethod) {
      return (
        <MethodGrid
          title={query ? `搜索：${search}` : currentSection.name}
          subtitle={query ? `在所有分区中找到 ${visibleMethods.length} 个方法` : currentSection.hint}
          methods={visibleMethods}
          onPick={pickMethod}
          onUnimplemented={pickMethod}
        />
      );
    }
    const common = {
      outputDir: settings.outputDir,
      ffmpegReady,
      onChangeOutputDir: (dir: string) => patchSettings({ outputDir: dir }),
      onEnqueued: enqueued,
      toast,
      onBack: () => setActiveMethod(null),
    };
    if (activeMethod.panel === "video") return <VideoPanel methodId={activeMethod.id} caps={caps} {...common} />;
    if (activeMethod.panel === "audio") return <AudioPanel methodId={activeMethod.id} caps={caps} {...common} />;
    if (activeMethod.panel === "image") return <ImagePanel methodId={activeMethod.id} {...common} />;
    if (activeMethod.panel === "sequence") return <SequencePanel methodId={activeMethod.id} {...common} />;
    if (activeMethod.panel === "frames") return <FramesPanel methodId={activeMethod.id} {...common} />;
    if (activeMethod.panel === "collage") return <CollagePanel methodId={activeMethod.id} {...common} />;
    if (activeMethod.panel === "pdf") return <PdfPanel methodId={activeMethod.id} pdfTools={pdfTools} onRedetectPdfTools={redetectPdfTools} {...common} />;
    return null;
  };

  return (
    <div className="app">
      <TopBar
        theme={settings.theme}
        search={search}
        taskCount={running}
        tasksOpen={tasksOpen}
        ffmpegReady={ffmpegReady}
        onSearch={setSearch}
        onToggleTheme={() => patchSettings({ theme: settings.theme === "dark" ? "light" : "dark" })}
        onToggleTasks={() => {
          const next = !tasksOpen;
          setTasksOpen(next);
          patchSettings({ taskCenterOpen: next });
        }}
        onOpenSettings={() => setSettingsOpen(true)}
      />

      {migrationNotice ? (
        <div className="migration-banner" role="status">
          <span>{migrationNotice}</span>
          <button
            className="btn small ghost"
            onClick={() => {
              localStorage.setItem("transmutio:migration-notice-dismissed", "1");
              setMigrationNotice(null);
            }}
          >
            知道了
          </button>
        </div>
      ) : null}

      <div className="body">
        {needsSetup ? (
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
            <SideNav activeSection={activeSection} tools={tools} onSelect={selectSection} />
            <main className="main">{renderPanel()}</main>
          </>
        )}
        {tasksOpen && !needsSetup ? (
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
              patchSettings({ taskCenterOpen: false });
            }}
          />
        ) : null}
      </div>

      <div className="statusbar">
        <span>
          <span className={`dot ${ffmpegReady ? "" : "bad"}`} />
          {ffmpegReady ? `FFmpeg ${tools?.ffmpegVersion ?? "已就绪"}` : "未找到 FFmpeg"}
        </span>
        <span className={`flavor-tag ${flavor?.id === "gpl" ? "gpl" : ""}`} title={
          flavor
            ? `${flavor.productName} · ${flavor.licenseLabel} · 更新通道 ${flavor.updateChannel}`
            : "变体信息不可用"
        }>
          {flavor ? (flavor.id === "gpl" ? `${flavor.productName} · GPL` : `${flavor.productName} · LGPL`) : "变体未知"}
        </span>
        <span className={tools?.build.nonfree ? "" : tools?.build.lgplOk ? "" : undefined} style={tools?.build.nonfree ? { color: "var(--ff-warn)" } : undefined}>
          {tools
            ? `FFmpeg 构建：${tools.build.nonfree ? "nonfree" : tools.build.gpl ? "GPL" : "LGPL"}${
                tools.build.gpl ? "（含 libx264/libx265）" : "（无 libx264/libx265）"
              }`
            : "FFmpeg 构建：未知"}
        </span>
        {caps ? (
          <span>
            软编：{caps.videoEncoders.filter((encoder) => encoder.available && encoder.allowed && !encoder.hardware).map((encoder) => encoder.id).join(" / ") || "无"}
            {" · "}硬编：{caps.videoEncoders.filter((encoder) => encoder.available && encoder.allowed && encoder.hardware).length} 个可用
          </span>
        ) : null}
        <span className="spacer" />
        <span>
          并发 {settings.concurrency} · 运行 {taskState.stats.running} · 排队 {taskState.stats.queued}
        </span>
        <span>
          {tools?.platform ?? "—"}/{tools?.arch ?? "—"}
        </span>
        {!tasksOpen ? (
          <button className="btn small ghost" onClick={() => setTasksOpen(true)}>
            <Icon name="grid" size={12} />
            任务中心
          </button>
        ) : null}
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
              {item.text}
            </div>
          ))}
        </div>
      ) : null}
    </div>
  );
}
