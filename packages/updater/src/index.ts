/**
 * @formatforge/updater
 * 自动更新的适配层：定义契约 + 空实现 + 控制器。
 * 真正接入 Tauri updater 时，只需实现 UpdaterAdapter，不改 UI。
 */

export type UpdateState =
  | { phase: "idle" }
  | { phase: "checking" }
  | { phase: "up-to-date"; checkedAt: number }
  | { phase: "available"; info: UpdateInfo }
  | { phase: "downloading"; info: UpdateInfo; progress: number }
  | { phase: "error"; message: string };

export interface UpdateInfo {
  version: string;
  notes?: string;
}

export interface UpdaterAdapter {
  /** 检查更新；无更新返回 null。 */
  check(): Promise<UpdateInfo | null>;
  /** 下载并安装；onProgress 为 0..1。 */
  downloadAndInstall(info: UpdateInfo, onProgress: (ratio: number) => void): Promise<void>;
}

/** 未配置更新源时的占位实现，UI 会显示明确原因而不是假装可用。 */
export function createNullUpdater(reason: string): UpdaterAdapter {
  return {
    async check() {
      throw new Error(reason);
    },
    async downloadAndInstall() {
      throw new Error(reason);
    },
  };
}

export interface UpdaterController {
  getState(): UpdateState;
  subscribe(listener: () => void): () => void;
  check(): Promise<void>;
  install(): Promise<void>;
}

export function createUpdaterController(
  adapter: UpdaterAdapter,
  options: { enabled: boolean; disabledReason?: string } = { enabled: true },
): UpdaterController {
  let state: UpdateState = { phase: "idle" };
  const listeners = new Set<() => void>();
  const set = (next: UpdateState) => {
    state = next;
    listeners.forEach((fn) => fn());
  };

  return {
    getState: () => state,
    subscribe(listener) {
      listeners.add(listener);
      return () => listeners.delete(listener);
    },
    async check() {
      if (!options.enabled) {
        set({ phase: "error", message: options.disabledReason ?? "自动更新未启用" });
        return;
      }
      set({ phase: "checking" });
      try {
        const info = await adapter.check();
        if (info) set({ phase: "available", info });
        else set({ phase: "up-to-date", checkedAt: Date.now() });
      } catch (error) {
        set({ phase: "error", message: error instanceof Error ? error.message : String(error) });
      }
    },
    async install() {
      if (state.phase !== "available") return;
      const info = state.info;
      set({ phase: "downloading", info, progress: 0 });
      try {
        await adapter.downloadAndInstall(info, (ratio) => set({ phase: "downloading", info, progress: ratio }));
      } catch (error) {
        set({ phase: "error", message: error instanceof Error ? error.message : String(error) });
      }
    },
  };
}
