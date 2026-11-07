import { createSettingsStore } from "@formatforge/settings";
import { createI18n, mergeCatalogs, baseCatalogs, type MessageCatalogs } from "@formatforge/i18n";
import type { ThemeMode } from "@formatforge/design-tokens";
import { applyTheme, themes } from "@formatforge/design-tokens";
import { invoke } from "@tauri-apps/api/core";
import { createNamespacedStorage } from "./storage";

export interface Settings {
  theme: ThemeMode;
  concurrency: number;
  outputDir: string;
  ffmpegPath: string;
  /** 首次引导页展示的下载源（空则用内置官方页面） */
  ffmpegSourceUrl: string;
  /** 内置下载器使用的来源 id；空 = 官方优先、失败按清单顺序回退 */
  ffmpegDownloadSource: string;
  allowGplEncoders: boolean;
  taskCenterOpen: boolean;
  lastSection: string;
  /** 左栏折叠态（64px rail）；持久化，重启后保持。 */
  navCollapsed: boolean;
  /** 最近使用的方法 id，最新在前，最多 4 条。 */
  recentMethods: string[];
  /** 每个方法上次使用的预设 id（按方法粒度记忆）。 */
  lastPresetId: Record<string, string>;
  /** 输出位置模式：source = 与源文件相同目录；custom = 自定义目录。 */
  outputDirMode: "source" | "custom";
  /** 批次全部完成后的动作；shutdown 还需环境变量才真正执行。 */
  afterAction: "none" | "open-folder" | "shutdown";
  /** 队列级暂停（重启后仍保持暂停，避免用户以为暂停只是当前会话）。 */
  queuePaused: boolean;
}

/** 中文词典：本工程的界面语言。en-US 仅保留骨架（未逐条翻译）。 */
export const zhCatalog: MessageCatalogs["zh-CN"] = {
  "app.name": "Transmutio",
  "app.tagline": "格式转换",
  "nav.video": "视频",
  "nav.audio": "音频",
  "nav.image": "图片",
  "top.search": "搜索转换方法…",
  "top.addFiles": "添加文件",
  "top.tasks": "任务中心",
  "top.settings": "设置",
  "top.theme": "切换主题",
  "action.start": "开始转换",
  "action.back": "返回",
  "action.add": "添加文件",
  "action.clearFinished": "清空已完成",
  "action.openFolder": "打开目录",
  "state.queued": "排队中",
  "state.running": "转换中",
  "state.done": "已完成",
  "state.failed": "失败",
  "state.cancelled": "已取消",
  "state.skipped": "已跳过",
  "badge.implemented": "可用",
  "badge.unimplemented": "未实现",
  "empty.tasks": "还没有任务。选择一条转换方法并添加文件即可开始。",
  "empty.files": "拖入文件，或点击「添加文件」。",
};

export const enCatalog: MessageCatalogs["en-US"] = {
  "app.name": "Transmutio",
  "app.tagline": "Media Converter",
  "nav.video": "Video",
  "nav.audio": "Audio",
  "nav.image": "Image",
  "top.search": "Search methods…",
  "top.addFiles": "Add files",
  "top.tasks": "Tasks",
  "top.settings": "Settings",
  "top.theme": "Toggle theme",
  "action.start": "Start",
  "action.back": "Back",
  "action.add": "Add files",
  "action.clearFinished": "Clear finished",
  "action.openFolder": "Open folder",
  "state.queued": "Queued",
  "state.running": "Running",
  "state.done": "Done",
  "state.failed": "Failed",
  "state.cancelled": "Cancelled",
  "state.skipped": "Skipped",
  "badge.implemented": "Ready",
  "badge.unimplemented": "Not implemented",
  "empty.tasks": "No tasks yet. Pick a method and add files to begin.",
  "empty.files": "Drop files here, or click “Add files”.",
};

export const i18n = createI18n({
  locale: "zh-CN",
  fallbackLocale: "zh-CN",
  catalogs: mergeCatalogs(baseCatalogs, { "zh-CN": zhCatalog, "en-US": enCatalog }),
});

export const settingsStore = createSettingsStore<Settings>({
  key: "settings",
  defaults: {
    theme: "dark",
    concurrency: 4,
    outputDir: "",
    ffmpegPath: "",
    ffmpegSourceUrl: "",
    ffmpegDownloadSource: "",
    allowGplEncoders: false,
    taskCenterOpen: true,
    lastSection: "video",
    navCollapsed: false,
    recentMethods: [],
    lastPresetId: {},
    outputDirMode: "source",
    afterAction: "none",
    queuePaused: false,
  },
  storage: createNamespacedStorage(),
  version: 1,
  sanitize: (value) => ({
    ...value,
    concurrency: Math.min(8, Math.max(1, Number(value.concurrency) || 4)),
    theme: value.theme === "light" ? "light" : "dark",
  }),
});

export function syncTheme(theme: ThemeMode): void {
  applyTheme(theme);
  // 只有 Tauri 内需要同步原生标题栏；浏览器预览与无头测试没有 __TAURI_INTERNALS__。
  if (typeof window === "undefined" || !("__TAURI_INTERNALS__" in window)) return;
  const { bg, bg2 } = themes[theme].colors;
  // bg = 页面底色；bg2 = 顶栏渐变起点，用作标题栏颜色，避免内容区与标题栏出现色差线。
  void invoke("set_window_chrome", { theme, bg, caption: bg2 }).catch(() => {
    /* 平台不支持标题栏配色时静默回退到 light/dark 标题栏，不影响使用 */
  });
}
