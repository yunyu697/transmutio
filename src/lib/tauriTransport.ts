import { listen } from "@tauri-apps/api/event";
import type { TaskTransport, TaskBase } from "@formatforge/task-queue";
import { api, type Job } from "./api";

/**
 * 把 Rust 后端的任务模型接到共享任务队列契约上。
 * 后端事件是唯一事实来源；前端不做进度插值，避免"看起来在动"的假进度。
 */
export function createTauriTransport(): TaskTransport<Job> {
  return {
    list: () => api.listJobs(),
    cancel: (id) => api.cancelJob(id),
    retry: (id) => api.retryJob(id),
    openOutput: (id) => api.openJobFolder(id),
    async subscribe(onTask, onRemove) {
      const unlistenUpdate = await listen<Job>("job:update", (event) => onTask(event.payload));
      const unlistenRemove = await listen<string>("job:removed", (event) => onRemove(event.payload));
      return () => {
        unlistenUpdate();
        unlistenRemove();
      };
    },
  };
}

export type { Job as Task };
export type TaskType = Job & TaskBase;
