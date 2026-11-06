/**
 * @formatforge/settings
 * 可复用的分区设置存储：默认值 + 持久化 + 订阅 + 版本迁移。
 * 存储介质通过 StorageAdapter 注入（localStorage / 文件 / 内存 / 远端）。
 */

export interface StorageAdapter {
  getItem(key: string): string | null;
  setItem(key: string, value: string): void;
  removeItem(key: string): void;
}

export interface SettingsStoreOptions<T extends object> {
  key: string;
  defaults: T;
  storage: StorageAdapter;
  /** 版本号写入 payload，便于后续迁移。 */
  version?: number;
  /** 旧版本数据迁移；返回 undefined 表示丢弃并使用默认值。 */
  migrate?: (raw: unknown, fromVersion: number | undefined) => T | undefined;
  /** 反序列化后做一次浅层校验/归一化（例如枚举白名单）。 */
  sanitize?: (value: T) => T;
}

export interface SettingsStore<T extends object> {
  get(): Readonly<T>;
  set(patch: Partial<T>): Readonly<T>;
  replace(next: T): Readonly<T>;
  reset(): Readonly<T>;
  subscribe(listener: (value: Readonly<T>) => void): () => void;
}

interface Payload<T> {
  version: number;
  data: T;
}

export function createMemoryStorage(initial?: Record<string, string>): StorageAdapter {
  const map = new Map<string, string>(Object.entries(initial ?? {}));
  return {
    getItem: (key) => (map.has(key) ? (map.get(key) as string) : null),
    setItem: (key, value) => void map.set(key, value),
    removeItem: (key) => void map.delete(key),
  };
}

export function createLocalStorageAdapter(prefix = "transmutio:"): StorageAdapter {
  return {
    getItem: (key) => {
      try {
        return window.localStorage.getItem(prefix + key);
      } catch {
        return null;
      }
    },
    setItem: (key, value) => {
      try {
        window.localStorage.setItem(prefix + key, value);
      } catch {
        /* 隐私模式或配额不足时静默降级为"当次会话有效" */
      }
    },
    removeItem: (key) => {
      try {
        window.localStorage.removeItem(prefix + key);
      } catch {
        /* ignore */
      }
    },
  };
}

export function createSettingsStore<T extends object>(options: SettingsStoreOptions<T>): SettingsStore<T> {
  const version = options.version ?? 1;
  const listeners = new Set<(value: Readonly<T>) => void>();
  let state: T = load() ?? { ...options.defaults };

  function load(): T | undefined {
    const raw = options.storage.getItem(options.key);
    if (!raw) return undefined;
    try {
      const parsed = JSON.parse(raw) as Partial<Payload<T>> | T;
      const hasEnvelope = parsed && typeof parsed === "object" && "data" in (parsed as object);
      const data = hasEnvelope ? (parsed as Payload<T>).data : (parsed as T);
      const fromVersion = hasEnvelope ? (parsed as Payload<T>).version : undefined;
      let merged = { ...options.defaults, ...(data as object) } as T;
      if (fromVersion !== undefined && fromVersion !== version && options.migrate) {
        const migrated = options.migrate(merged, fromVersion);
        if (!migrated) return undefined;
        merged = { ...options.defaults, ...migrated } as T;
      }
      return options.sanitize ? options.sanitize(merged) : merged;
    } catch {
      return undefined;
    }
  }

  function persist() {
    const payload: Payload<T> = { version, data: state };
    options.storage.setItem(options.key, JSON.stringify(payload));
  }

  function emit() {
    const snapshot = { ...state } as Readonly<T>;
    listeners.forEach((fn) => fn(snapshot));
  }

  return {
    get: () => ({ ...state } as Readonly<T>),
    set(patch) {
      state = { ...state, ...patch };
      persist();
      emit();
      return { ...state } as Readonly<T>;
    },
    replace(next) {
      state = options.sanitize ? options.sanitize(next) : next;
      persist();
      emit();
      return { ...state } as Readonly<T>;
    },
    reset() {
      state = { ...options.defaults };
      options.storage.removeItem(options.key);
      emit();
      return { ...state } as Readonly<T>;
    },
    subscribe(listener) {
      listeners.add(listener);
      return () => listeners.delete(listener);
    },
  };
}
