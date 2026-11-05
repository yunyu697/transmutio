/**
 * @formatforge/task-queue
 * 前端任务队列状态层：状态机 + 订阅 + 统计 + 与执行后端解耦（通过 TaskTransport）。
 * 后端可以是真的子进程（Tauri），也可以是 Web 端模拟实现。
 */

export type TaskStatus = "queued" | "running" | "done" | "failed" | "cancelled" | "skipped";

export interface TaskBase {
  id: string;
  status: TaskStatus;
  /** 0..1 */
  progress: number;
  createdAt: number;
  startedAt?: number | null;
  finishedAt?: number | null;
  message?: string | null;
  errorKind?: string | null;
}

export interface TaskTransport<T extends TaskBase> {
  /** 拉取全量任务快照。 */
  list(): Promise<T[]>;
  /** 请求取消；后端负责真正终止子进程。 */
  cancel(id: string): Promise<void>;
  /** 失败任务重试，后端会生成新的执行尝试。 */
  retry(id: string): Promise<void>;
  /** 打开任务输出所在目录（可选能力）。 */
  openOutput?(id: string): Promise<void>;
  /** 订阅增量更新；返回取消订阅函数。 */
  subscribe(
    onTask: (task: T) => void,
    onRemove: (id: string) => void,
  ): Promise<() => void>;
}

export interface TaskQueueStats {
  total: number;
  queued: number;
  running: number;
  done: number;
  failed: number;
  cancelled: number;
  skipped: number;
}

export interface TaskQueueState<T extends TaskBase> {
  tasks: T[];
  stats: TaskQueueStats;
  ready: boolean;
}

export interface TaskQueue<T extends TaskBase> {
  getState(): TaskQueueState<T>;
  subscribe(listener: () => void): () => void;
  refresh(): Promise<void>;
  cancel(id: string): Promise<void>;
  retry(id: string): Promise<void>;
  openOutput(id: string): Promise<void>;
  /** 本地乐观插入（入队后立即反映，无需等待后端刷新）。 */
  track(task: T): void;
  /** 清空本地已结束任务的展示（不影响后端历史）。 */
  clearFinished(): void;
  dispose(): void;
}

export function computeStats<T extends TaskBase>(tasks: T[]): TaskQueueStats {
  const stats: TaskQueueStats = { total: tasks.length, queued: 0, running: 0, done: 0, failed: 0, cancelled: 0, skipped: 0 };
  for (const task of tasks) stats[task.status] += 1;
  return stats;
}

export function createTaskQueue<T extends TaskBase>(transport: TaskTransport<T>): TaskQueue<T> {
  let state: TaskQueueState<T> = {
    tasks: [],
    stats: computeStats([]),
    ready: false,
  };
  const listeners = new Set<() => void>();
  let unsubscribe: (() => void) | null = null;
  let disposed = false;

  const emit = () => listeners.forEach((fn) => fn());

  const commit = (tasks: T[]) => {
    tasks.sort((a, b) => a.createdAt - b.createdAt);
    state = { tasks, stats: computeStats(tasks), ready: true };
    emit();
  };

  const upsert = (task: T) => {
    const next = state.tasks.slice();
    const idx = next.findIndex((item) => item.id === task.id);
    if (idx >= 0) next[idx] = task;
    else next.push(task);
    commit(next);
  };

  const removeLocal = (id: string) => {
    commit(state.tasks.filter((item) => item.id !== id));
  };

  const start = async () => {
    try {
      unsubscribe = await transport.subscribe(upsert, removeLocal);
    } catch {
      // 订阅失败不影响手动刷新
    }
    await refresh();
  };

  const refresh = async () => {
    const tasks = await transport.list();
    if (!disposed) commit(tasks);
  };

  void start();

  return {
    getState: () => state,
    subscribe(listener) {
      listeners.add(listener);
      return () => listeners.delete(listener);
    },
    refresh,
    async cancel(id) {
      await transport.cancel(id);
      await refresh();
    },
    async retry(id) {
      await transport.retry(id);
      await refresh();
    },
    async openOutput(id) {
      if (!transport.openOutput) throw new Error("当前后端不支持打开输出目录");
      await transport.openOutput(id);
    },
    track(task) {
      upsert(task);
    },
    clearFinished() {
      commit(state.tasks.filter((t) => t.status === "running" || t.status === "queued"));
    },
    dispose() {
      disposed = true;
      unsubscribe?.();
      listeners.clear();
    },
  };
}
