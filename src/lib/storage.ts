/**
 * localStorage 命名空间迁移：formatforge:* → transmutio:*。
 *
 * 迁移在模块加载时执行一次，规则：读旧键 → 写新键 → 读回校验 → 校验通过才删旧键；
 * 任一步失败保留旧键，不丢用户配置。标记键存在即跳过，保证幂等。
 *
 * TODO(v0.2)：满一个版本周期后删除 LEGACY_PREFIX 的回退读取与镜像写入（兼容旧版回滚）。
 */
export const STORAGE_PREFIX = "transmutio:";
export const LEGACY_PREFIX = "formatforge:";
export const MIGRATION_KEY = `${STORAGE_PREFIX}storage-migration:v1`;
/** 一个版本周期内双写旧前缀：旧版回滚时仍能读到配置。 */
const MIRROR_LEGACY = true;

export interface MigrationResult {
  alreadyDone: boolean;
  migrated: string[];
  conflicts: string[];
  failed: string[];
}

function safeStorage(): Storage | null {
  try {
    return window.localStorage;
  } catch {
    return null;
  }
}

export function migrateStoragePrefixes(storage: Storage | null = safeStorage()): MigrationResult {
  const result: MigrationResult = { alreadyDone: false, migrated: [], conflicts: [], failed: [] };
  if (!storage) return result;
  try {
    if (storage.getItem(MIGRATION_KEY)) {
      result.alreadyDone = true;
      return result;
    }
  } catch {
    return result;
  }

  const legacyKeys: string[] = [];
  try {
    for (let i = 0; i < storage.length; i += 1) {
      const key = storage.key(i);
      if (key && key.startsWith(LEGACY_PREFIX)) legacyKeys.push(key);
    }
  } catch {
    return result;
  }

  for (const oldKey of legacyKeys) {
    const suffix = oldKey.slice(LEGACY_PREFIX.length);
    const newKey = STORAGE_PREFIX + suffix;
    try {
      const value = storage.getItem(oldKey);
      if (value == null) continue;
      const existing = storage.getItem(newKey);
      if (existing != null) {
        // 已有新键：内容相同则只清旧键；不同则保留旧键并记录冲突，绝不用旧值覆盖新值。
        if (existing === value) storage.removeItem(oldKey);
        else result.conflicts.push(suffix);
        continue;
      }
      storage.setItem(newKey, value);
      if (storage.getItem(newKey) !== value) {
        storage.removeItem(newKey);
        result.failed.push(suffix);
        continue;
      }
      storage.removeItem(oldKey);
      result.migrated.push(suffix);
    } catch {
      result.failed.push(suffix);
    }
  }

  try {
    storage.setItem(
      MIGRATION_KEY,
      JSON.stringify({ at: Date.now(), migrated: result.migrated.length, conflicts: result.conflicts.length }),
    );
  } catch {
    /* 标记写不进去：下次启动会重新校验，仍然幂等 */
  }
  return result;
}

export function readNamespaced(key: string, storage: Storage | null = safeStorage()): string | null {
  if (!storage) return null;
  try {
    const value = storage.getItem(STORAGE_PREFIX + key);
    if (value != null) return value;
    return storage.getItem(LEGACY_PREFIX + key);
  } catch {
    return null;
  }
}

export function writeNamespaced(key: string, value: string, storage: Storage | null = safeStorage()): void {
  if (!storage) return;
  try {
    storage.setItem(STORAGE_PREFIX + key, value);
    if (MIRROR_LEGACY) storage.setItem(LEGACY_PREFIX + key, value);
  } catch {
    /* 隐私模式 / 配额不足：静默降级为不持久化 */
  }
}

export function removeNamespaced(key: string, storage: Storage | null = safeStorage()): void {
  if (!storage) return;
  try {
    storage.removeItem(STORAGE_PREFIX + key);
    storage.removeItem(LEGACY_PREFIX + key);
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

// 模块加载即迁移，早于任何 settings store / 面板参数的读取。
migrateStoragePrefixes();
