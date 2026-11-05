/**
 * @formatforge/design-tokens
 * 共享设计令牌：颜色 / 字号 / 圆角 / 间距 / 尺寸。
 * 只依赖数据与纯函数，不依赖 UI 框架，便于后续被其它工程直接复用。
 */

export type ThemeMode = "dark" | "light";

export interface ColorTokens {
  bg: string;
  bg2: string;
  surf: string;
  surf2: string;
  surf3: string;
  line: string;
  lineStrong: string;
  fg: string;
  fg2: string;
  fg3: string;
  accent: string;
  accentSoft: string;
  accent2: string;
  ok: string;
  warn: string;
  danger: string;
  shadow: string;
}

export const darkColors: ColorTokens = {
  bg: "#0A0E14",
  bg2: "#0D131B",
  surf: "#121A24",
  surf2: "#17212D",
  surf3: "#1E2937",
  line: "#233040",
  lineStrong: "#2E3D50",
  fg: "#E9EFF8",
  fg2: "#A7B4C6",
  fg3: "#71809A",
  accent: "#2ED3B7",
  accentSoft: "rgba(46,211,183,0.14)",
  accent2: "#4F8CFF",
  ok: "#3DDC97",
  warn: "#F5A524",
  danger: "#F0546A",
  shadow: "0 18px 44px rgba(0,0,0,0.45)",
};

export const lightColors: ColorTokens = {
  bg: "#EDF1F6",
  bg2: "#F6F8FB",
  surf: "#FFFFFF",
  surf2: "#F2F6FA",
  surf3: "#E3EAF3",
  line: "#D9E3EF",
  lineStrong: "#C4D2E3",
  fg: "#15202B",
  fg2: "#4A5A6E",
  fg3: "#77879B",
  accent: "#0FAE95",
  accentSoft: "rgba(15,174,149,0.12)",
  accent2: "#2E6FE0",
  ok: "#17A673",
  warn: "#B36B00",
  danger: "#D22F4A",
  shadow: "0 14px 34px rgba(20,40,70,0.12)",
};

/** 与上一轮设计稿保持一致的字号阶梯（px）。 */
export const typography = {
  fontFamily: '"Inter", "Segoe UI", "PingFang SC", "Microsoft YaHei UI", system-ui, sans-serif',
  monoFamily: '"JetBrains Mono", "Cascadia Mono", "Consolas", monospace',
  size: { display: 20, title: 14, body: 12.5, caption: 11, micro: 10 },
  weight: { regular: 400, medium: 500, semibold: 600, bold: 700 },
  lineHeight: { tight: 1.25, normal: 1.5 },
} as const;

export const radius = { control: 8, card: 14, window: 20, pill: 999 } as const;

export const spacing = { grid: 4, xs: 4, sm: 8, md: 12, lg: 16, xl: 24, xxl: 32 } as const;

export const layout = {
  topBar: 58,
  sideNav: 236,
  taskCenter: 352,
  inspector: 402,
  statusBar: 30,
} as const;

export interface ThemeDefinition {
  mode: ThemeMode;
  colors: ColorTokens;
}

export const themes: Record<ThemeMode, ThemeDefinition> = {
  dark: { mode: "dark", colors: darkColors },
  light: { mode: "light", colors: lightColors },
};

const CSS_VAR_MAP: Array<[keyof ColorTokens, string]> = [
  ["bg", "--ff-bg"],
  ["bg2", "--ff-bg2"],
  ["surf", "--ff-surf"],
  ["surf2", "--ff-surf2"],
  ["surf3", "--ff-surf3"],
  ["line", "--ff-line"],
  ["lineStrong", "--ff-line-strong"],
  ["fg", "--ff-fg"],
  ["fg2", "--ff-fg2"],
  ["fg3", "--ff-fg3"],
  ["accent", "--ff-accent"],
  ["accentSoft", "--ff-accent-soft"],
  ["accent2", "--ff-accent2"],
  ["ok", "--ff-ok"],
  ["warn", "--ff-warn"],
  ["danger", "--ff-danger"],
  ["shadow", "--ff-shadow"],
];

/** 输出 CSS 变量声明文本，宿主工程可直接注入 :root。 */
export function themeToCss(mode: ThemeMode): string {
  const { colors } = themes[mode];
  const lines = CSS_VAR_MAP.map(([key, cssVar]) => `${cssVar}: ${colors[key]};`);
  lines.push(`--ff-font: ${typography.fontFamily};`);
  lines.push(`--ff-font-mono: ${typography.monoFamily};`);
  return lines.join("\n");
}

/** 运行时切换主题：只改 <html data-theme>，样式由 CSS 变量接管。 */
export function applyTheme(mode: ThemeMode, root?: HTMLElement): void {
  const el = root ?? (typeof document !== "undefined" ? document.documentElement : undefined);
  if (!el) return;
  el.setAttribute("data-theme", mode);
  el.style.colorScheme = mode === "dark" ? "dark" : "light";
}

export function formatBytes(bytes: number, fractionDigits = 1): string {
  if (!Number.isFinite(bytes) || bytes <= 0) return "0 B";
  const units = ["B", "KB", "MB", "GB", "TB"];
  const idx = Math.min(Math.floor(Math.log(bytes) / Math.log(1024)), units.length - 1);
  const value = bytes / Math.pow(1024, idx);
  return `${value.toFixed(idx === 0 ? 0 : fractionDigits)} ${units[idx]}`;
}

export function formatDuration(seconds?: number | null): string {
  if (seconds == null || !Number.isFinite(seconds) || seconds <= 0) return "—";
  const s = Math.floor(seconds);
  const h = Math.floor(s / 3600);
  const m = Math.floor((s % 3600) / 60);
  const sec = s % 60;
  const pad = (n: number) => String(n).padStart(2, "0");
  return h > 0 ? `${h}:${pad(m)}:${pad(sec)}` : `${m}:${pad(sec)}`;
}

export function formatRelativeTime(ts: number): string {
  const diff = Date.now() - ts;
  if (diff < 60_000) return "刚刚";
  if (diff < 3_600_000) return `${Math.floor(diff / 60_000)} 分钟前`;
  if (diff < 86_400_000) return `${Math.floor(diff / 3_600_000)} 小时前`;
  return `${Math.floor(diff / 86_400_000)} 天前`;
}
