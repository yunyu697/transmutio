# @formatforge/task-queue

任务队列的前端状态层，与执行后端解耦。

## 接口

```ts
const queue = createTaskQueue<Task>(transport);
queue.getState();        // { tasks, stats, ready }
queue.subscribe(fn);     // 供 useSyncExternalStore 使用
queue.refresh();         // 拉全量快照
queue.cancel(id);        // 请求后端终止子进程
queue.retry(id);         // 失败重试
queue.openOutput(id);    // 打开输出目录（可选能力）
queue.track(task);       // 入队后乐观插入
queue.clearFinished();   // 仅清理已完成任务的展示
```

`TaskTransport<T>` 由宿主实现：本工程的实现是 Tauri `invoke` + 事件（见 `src/lib/tauriTransport.ts`）；Web 演示可换成内存实现。

## 状态机

`queued → running → done | failed | cancelled`，失败的 `retry` 由后端重新排队，前端只负责触发与刷新。

## 替换点

若共享包定稿，保持 `TaskTransport` 契约即可无缝替换；UI 组件不直接访问 Tauri API。
