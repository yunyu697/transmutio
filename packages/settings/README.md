# @formatforge/settings

分区设置存储，可注入存储介质，供桌面端 / Web 端 / 测试复用。

## 接口

- `createSettingsStore<T>({ key, defaults, storage, version?, migrate?, sanitize? })`
  - `get()` / `set(patch)` / `replace(next)` / `reset()` / `subscribe(fn)`
- `createLocalStorageAdapter(prefix?)`：浏览器持久化适配器（异常自动降级）。
- `createMemoryStorage(initial?)`：测试/无持久化环境适配器。

## 数据格式

```json
{ "version": 1, "data": { "...": "..." } }
```

版本号变化时调用 `migrate`；返回 `undefined` 表示数据不可用，回落默认值。

## 替换点

桌面端若要把设置写入操作系统配置目录（而非 WebView localStorage），实现一个 `StorageAdapter`（可基于 Tauri 命令）在本工程 `src/lib/settings.ts` 注入即可，业务组件无需改动。
