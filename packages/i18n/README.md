# @formatforge/i18n

共享国际化内核，纯函数 + 订阅模型，不依赖 React。

## 接口

- `createI18n({ locale, fallbackLocale, catalogs })` → `{ locale, t, setLocale, subscribe, availableLocales }`
- `t(key, vars?)`：支持 `{name}` 插值；缺失键回退到 `fallbackLocale`，再回退到原始 key。
- `baseCatalogs`：通用动作词条（取消/确定/重试…），宿主可覆盖。
- `mergeCatalogs(...)`：合并多份词典。

## 边界

- 本包只负责"取词"，不负责持久化语言偏好（由 `@formatforge/settings` 负责）。
- 产品文案必须由宿主注入，共享层不写业务文案。

## 当前接入

本工程界面全中文（`zh-CN`），已内置 `en-US` 词典骨架但未逐条翻译；设置页不提供语言切换（本版仅简体中文）。
