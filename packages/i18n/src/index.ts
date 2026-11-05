/**
 * @formatforge/i18n
 * 极简 i18n 内核：无框架依赖、支持插值、支持运行时切换与订阅。
 * 业务词典由宿主注入，避免把产品文案固化在共享层。
 */

export type LocaleCode = string;
export type MessageDict = Record<string, string>;
export type MessageCatalogs = Record<LocaleCode, MessageDict>;

export interface I18nOptions {
  locale?: LocaleCode;
  fallbackLocale?: LocaleCode;
  /** 宿主提供的词典，按 locale 分组。 */
  catalogs: MessageCatalogs;
}

export interface I18n {
  readonly locale: LocaleCode;
  t(key: string, vars?: Record<string, string | number>): string;
  setLocale(locale: LocaleCode): void;
  subscribe(listener: (locale: LocaleCode) => void): () => void;
  availableLocales(): LocaleCode[];
}

/** 各产品都会用到的通用词条，可在宿主词典中覆盖。 */
export const baseCatalogs: MessageCatalogs = {
  "zh-CN": {
    "common.cancel": "取消",
    "common.confirm": "确定",
    "common.close": "关闭",
    "common.retry": "重试",
    "common.remove": "移除",
    "common.open": "打开",
    "common.loading": "加载中…",
  },
  "en-US": {
    "common.cancel": "Cancel",
    "common.confirm": "Confirm",
    "common.close": "Close",
    "common.retry": "Retry",
    "common.remove": "Remove",
    "common.open": "Open",
    "common.loading": "Loading…",
  },
};

function interpolate(template: string, vars?: Record<string, string | number>): string {
  if (!vars) return template;
  return template.replace(/\{(\w+)\}/g, (_m, name: string) =>
    Object.prototype.hasOwnProperty.call(vars, name) ? String(vars[name]) : `{${name}}`,
  );
}

export function createI18n(options: I18nOptions): I18n {
  const fallback = options.fallbackLocale ?? "zh-CN";
  let current: LocaleCode = options.locale ?? fallback;
  const listeners = new Set<(locale: LocaleCode) => void>();

  const lookup = (locale: LocaleCode, key: string): string | undefined =>
    options.catalogs[locale]?.[key] ?? baseCatalogs[locale]?.[key];

  return {
    get locale() {
      return current;
    },
    t(key, vars) {
      const raw = lookup(current, key) ?? lookup(fallback, key) ?? key;
      return interpolate(raw, vars);
    },
    setLocale(locale) {
      if (locale === current) return;
      current = locale;
      listeners.forEach((fn) => fn(locale));
    },
    subscribe(listener) {
      listeners.add(listener);
      return () => listeners.delete(listener);
    },
    availableLocales() {
      return Object.keys(options.catalogs);
    },
  };
}

/** 合并多份词典（后者覆盖前者），用于宿主 + 扩展包组合。 */
export function mergeCatalogs(...catalogs: MessageCatalogs[]): MessageCatalogs {
  const out: MessageCatalogs = {};
  for (const catalog of catalogs) {
    for (const [locale, dict] of Object.entries(catalog)) {
      out[locale] = { ...(out[locale] ?? {}), ...dict };
    }
  }
  return out;
}
