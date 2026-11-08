/**
 * @formatforge/design-tokens
 * 共享设计令牌：颜色 / 字号 / 圆角 / 间距 / 尺寸。
 * 只依赖数据与纯函数，不依赖 UI 框架，便于后续被其它工程直接复用。
 */

export type ThemeMode = "dark" | "light";
/** 明暗偏好：「跟随系统」在运行时解析成 ThemeMode，只决定亮度基调。 */
export type Brightness = ThemeMode | "system";
export type PaletteId = "graphite" | "ocean" | "amber" | "forest";

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
  fg3: "#7C8DA8",
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
  fg3: "#5F7086",
  accent: "#0A8571",
  accentSoft: "rgba(10,133,113,0.12)",
  accent2: "#2E6FE0",
  ok: "#0F7F59",
  warn: "#96600A",
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

/** 默认配色（石墨）。 */
export const themes: Record<ThemeMode, ThemeDefinition> = {
  dark: { mode: "dark", colors: darkColors },
  light: { mode: "light", colors: lightColors },
};

export interface PaletteDefinition {
  id: PaletteId;
  /** 展示名：能看出气质差异，不做「主题 1/2/3」。 */
  name: string;
  hint: string;
  dark: ColorTokens;
  light: ColorTokens;
}

/**
 * 四套完整色板：背景、表面、边框、主次文字、强调色、状态色、悬停/选中用的 soft 色一起换。
 * 每一套都同时提供浅色与深色两份，明暗偏好与配色方案可任意组合。
 */
export const palettes: Record<PaletteId, PaletteDefinition> = {
  graphite: {
    id: "graphite",
    name: "石墨",
    hint: "冷灰中性 · 青绿强调",
    dark: darkColors,
    light: lightColors,
  },
  ocean: {
    id: "ocean",
    name: "深海",
    hint: "蓝调沉着 · 冷光强调",
    dark: {
      bg: "#070F1C",
      bg2: "#0B1626",
      surf: "#101D31",
      surf2: "#152540",
      surf3: "#1B2E4C",
      line: "#22375A",
      lineStrong: "#2D4671",
      fg: "#E6EEFB",
      fg2: "#A9BCDA",
      fg3: "#7B90B4",
      accent: "#5AA6FF",
      accentSoft: "rgba(90,166,255,0.16)",
      accent2: "#59E0C6",
      ok: "#4BD6A0",
      warn: "#F2AE45",
      danger: "#F2687E",
      shadow: "0 18px 44px rgba(2,10,26,0.55)",
    },
    light: {
      bg: "#E9F1FB",
      bg2: "#F4F8FF",
      surf: "#FFFFFF",
      surf2: "#EDF3FC",
      surf3: "#DCE7F7",
      line: "#D2E0F2",
      lineStrong: "#B4C8E4",
      fg: "#0F1E33",
      fg2: "#3E5677",
      fg3: "#5B7395",
      accent: "#1A64CC",
      accentSoft: "rgba(26,100,204,0.12)",
      accent2: "#0B8D7B",
      ok: "#12865C",
      warn: "#8F5600",
      danger: "#BE2A46",
      shadow: "0 14px 34px rgba(18,52,96,0.14)",
    },
  },
  amber: {
    id: "amber",
    name: "暖砂",
    hint: "米纸暖调 · 琥珀强调",
    dark: {
      bg: "#14100C",
      bg2: "#1A1510",
      surf: "#221B13",
      surf2: "#2A2119",
      surf3: "#33281E",
      line: "#3F3225",
      lineStrong: "#54422F",
      fg: "#F7EEE2",
      fg2: "#CDBBA6",
      fg3: "#A08B74",
      accent: "#F0AE55",
      accentSoft: "rgba(240,174,85,0.16)",
      accent2: "#8FB4D8",
      ok: "#6CC48F",
      warn: "#F0B44F",
      danger: "#EE7568",
      shadow: "0 18px 44px rgba(12,8,4,0.5)",
    },
    light: {
      bg: "#F7F1E8",
      bg2: "#FCF8F1",
      surf: "#FFFFFF",
      surf2: "#F6EEE1",
      surf3: "#EDE1D0",
      line: "#E3D5C2",
      lineStrong: "#CBB89E",
      fg: "#2A2118",
      fg2: "#584634",
      fg3: "#77604A",
      accent: "#9C5B06",
      accentSoft: "rgba(156,91,6,0.12)",
      accent2: "#2A6AA3",
      ok: "#186E45",
      warn: "#815100",
      danger: "#B0332C",
      shadow: "0 14px 34px rgba(70,50,20,0.14)",
    },
  },
  forest: {
    id: "forest",
    name: "苔原",
    hint: "绿灰低饱和 · 薄荷强调",
    dark: {
      bg: "#0A1210",
      bg2: "#0E1815",
      surf: "#13201C",
      surf2: "#182A24",
      surf3: "#1E332C",
      line: "#284237",
      lineStrong: "#345548",
      fg: "#E7F2ED",
      fg2: "#ABC5BA",
      fg3: "#81998B",
      accent: "#4BD79C",
      accentSoft: "rgba(75,215,156,0.15)",
      accent2: "#83C2FF",
      ok: "#4BD79C",
      warn: "#EFB055",
      danger: "#F0707A",
      shadow: "0 18px 44px rgba(4,14,10,0.5)",
    },
    light: {
      bg: "#ECF3EE",
      bg2: "#F5FAF6",
      surf: "#FFFFFF",
      surf2: "#EEF6F1",
      surf3: "#DFEBE3",
      line: "#D2E1D7",
      lineStrong: "#B6CCBE",
      fg: "#14261D",
      fg2: "#3C5949",
      fg3: "#5A7566",
      accent: "#0C8159",
      accentSoft: "rgba(12,129,89,0.12)",
      accent2: "#2A69A8",
      ok: "#0F724C",
      warn: "#8C5A00",
      danger: "#B42B44",
      shadow: "0 14px 34px rgba(16,54,36,0.14)",
    },
  },
};

export const paletteIds = Object.keys(palettes) as PaletteId[];

export function isPaletteId(value: unknown): value is PaletteId {
  return typeof value === "string" && value in palettes;
}

/** 「跟随系统」在这里落地：只影响亮度，配色方案保持用户所选。 */
export function resolveMode(brightness: Brightness, systemPrefersDark: boolean): ThemeMode {
  if (brightness === "system") return systemPrefersDark ? "dark" : "light";
  return brightness;
}

export function paletteColors(palette: PaletteId, mode: ThemeMode): ColorTokens {
  return palettes[palette][mode];
}

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
export function themeToCss(mode: ThemeMode, palette: PaletteId = "graphite"): string {
  const colors = paletteColors(palette, mode);
  const lines = CSS_VAR_MAP.map(([key, cssVar]) => `${cssVar}: ${colors[key]};`);
  lines.push(`--ff-font: ${typography.fontFamily};`);
  lines.push(`--ff-font-mono: ${typography.monoFamily};`);
  return lines.join("\n");
}

/**
 * 全部配色的 CSS：亮度由 [data-theme] 决定，色板由 [data-palette] 决定，两者正交。
 * 深色是默认（html 规则），浅色覆盖在 [data-theme="light"]；每套配色再各自覆盖，
 * 组合选择器优先级更高，保证「深色 + 任意配色」与「浅色 + 任意配色」都成立。
 */
export function allThemeCss(): string {
  const parts = [`html{${themeToCss("dark", "graphite")}}`, `html[data-theme="light"]{${themeToCss("light", "graphite")}}`];
  for (const id of paletteIds) {
    if (id === "graphite") continue;
    parts.push(`html[data-palette="${id}"]{${themeToCss("dark", id)}}`);
    parts.push(`html[data-palette="${id}"][data-theme="light"]{${themeToCss("light", id)}}`);
  }
  return parts.join("\n");
}

/** 运行时切换：亮度写 data-theme，配色写 data-palette，样式全部由 CSS 变量接管。 */
export function applyTheme(palette: PaletteId, mode: ThemeMode, root?: HTMLElement): void {
  const el = root ?? (typeof document !== "undefined" ? document.documentElement : undefined);
  if (!el) return;
  el.setAttribute("data-theme", mode);
  el.setAttribute("data-palette", palette);
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
