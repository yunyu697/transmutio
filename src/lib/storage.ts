/**
 * localStorage 命名空间：只读写 `transmutio:*`。
 *
 * 旧版 `formatforge:*` 残留键**不读取、不迁移、不删除**：用户数据留着无影响，
 * 主动删除没有必要。不要在这里加回前缀回退或迁移逻辑。
 */
export const STORAGE_PREFIX = "transmutio:";

function safeStorage(): Storage | null {
  try {
    return window.localStorage;
  } catch {
    return null;
  }
}

export function readNamespaced(key: string, storage: Storage | null = safeStorage()): string | null {
  if (!storage) return null;
  try {
    return storage.getItem(STORAGE_PREFIX + key);
  } catch {
    return null;
  }
}

export function writeNamespaced(key: string, value: string, storage: Storage | null = safeStorage()): void {
  if (!storage) return;
  try {
    storage.setItem(STORAGE_PREFIX + key, value);
  } catch {
    /* 隐私模式 / 配额不足：静默降级为不持久化 */
  }
}

export function removeNamespaced(key: string, storage: Storage | null = safeStorage()): void {
  if (!storage) return;
  try {
    storage.removeItem(STORAGE_PREFIX + key);
  } catch {
    /* ignore */
  }
}

/** 供 settings 包使用的 StorageAdapter（结构化类型即可，不额外 import 类型）。 */
export function createNamespacedStorage() {
  return {
    getItem: (key: string) => readNamespaced(key),
    setItem: (key: string, value: string) => writeNamespaced(key, value),
    removeItem: (key: string) => removeNamespaced(key),
  };
}

/**
 * 一次性清理 alpha11 遗留的两个键。只删这两个确切键：
 * - 不读、不写、不删 `formatforge:*`；
 * - 无迁移标记、无后续副作用，删完即结束。
 */
export function cleanupRetiredKeys(storage: Storage | null = safeStorage()): void {
  if (!storage) return;
  try {
    storage.removeItem("transmutio:storage-migration:v1");
    storage.removeItem("transmutio:migration-notice-dismissed");
  } catch {
    /* ignore */
  }
}
