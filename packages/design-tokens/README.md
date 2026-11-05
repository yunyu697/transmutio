# @formatforge/design-tokens

共享设计令牌包。目标是让所有前端（本工程与未来其它工程）使用同一套视觉变量，而不是各自复制颜色值。

## 接口

- `darkColors` / `lightColors` / `themes`：颜色令牌对象。
- `typography` / `radius` / `spacing` / `layout`：字号、圆角、间距、布局尺寸。
- `themeToCss(mode)`：返回 CSS 变量声明字符串（`--ff-*`）。
- `applyTheme(mode, root?)`：运行时切换 `<html data-theme>` 与 `color-scheme`。
- `formatBytes` / `formatDuration` / `formatRelativeTime`：与令牌配套的展示格式化纯函数。

## 约定

- 变量前缀统一 `--ff-`；深浅主题通过 `[data-theme="dark"|"light"]` 切换。
- 新增令牌必须同时补 `darkColors` 与 `lightColors`，否则 `themeToCss` 会输出 undefined。
- 本包不依赖 React 或任何 UI 框架，可被桌面端、Web 端、设计工具直接消费。

## 替换点

若后续共享包定稿，只需保证 `--ff-*` 变量名与 `themes` 结构兼容；应用侧通过 `src/lib/theme.ts` 单点对接，不散落在组件里。
