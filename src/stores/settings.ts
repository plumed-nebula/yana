import { customRef, ref, readonly } from 'vue';
import { invoke } from '@tauri-apps/api/core';
import { info, debug, error as logError } from '@tauri-apps/plugin-log';

type PngCompressionMode = 'lossy' | 'lossless';
type PngOptimizationLevel = 'best' | 'default' | 'fast';

type PersistedSettings = {
  quality: number;
  convertToWebp: boolean;
  pngCompressionMode: PngCompressionMode;
  pngOptimization: PngOptimizationLevel;
  enableUploadCompression: boolean;
  maxConcurrentUploads: number;
};

const DEFAULTS: PersistedSettings = {
  quality: 80,
  convertToWebp: false,
  pngCompressionMode: 'lossless',
  pngOptimization: 'default',
  enableUploadCompression: false,
  maxConcurrentUploads: 5,
};

let singleton: ReturnType<typeof createStore> | null = null;

function safeJson(value: unknown): string {
  try {
    return JSON.stringify(value);
  } catch (err) {
    return String(value);
  }
}

function describeError(err: unknown): string {
  if (err instanceof Error) {
    const stack = err.stack ? `\n${err.stack}` : '';
    return `${err.name}: ${err.message}${stack}`;
  }
  return String(err);
}

function sanitizeQuality(input: unknown): number {
  let n = Number(input);
  if (!Number.isFinite(n)) n = DEFAULTS.quality;
  n = Math.round(n);
  if (n < 0) n = 0;
  if (n > 100) n = 100;
  return n;
}

function sanitizePngMode(value: unknown): PngCompressionMode {
  return value === 'lossy' ? 'lossy' : 'lossless';
}

function sanitizePngOptimization(value: unknown): PngOptimizationLevel {
  if (value === 'best' || value === 'fast') {
    return value;
  }
  return 'default';
}

function sanitizeConcurrency(input: unknown): number {
  let n = Number(input);
  if (!Number.isFinite(n)) n = DEFAULTS.maxConcurrentUploads;
  n = Math.round(n);
  if (n < 1) n = 1;
  if (n > 10) n = 10;
  return n;
}

function normalizePayload(
  payload:
    | (Partial<PersistedSettings> & { maxUploadConcurrency?: number })
    | null
    | undefined
): PersistedSettings {
  const pngModeFromBackend =
    (payload as any)?.pngCompressionMode ?? (payload as any)?.pngMode;
  const concurrencyFromBackend =
    (payload as any)?.maxConcurrentUploads ??
    (payload as any)?.maxUploadConcurrency;
  return {
    quality: sanitizeQuality(payload?.quality ?? DEFAULTS.quality),
    convertToWebp: Boolean(payload?.convertToWebp ?? DEFAULTS.convertToWebp),
    pngCompressionMode: sanitizePngMode(
      pngModeFromBackend ?? DEFAULTS.pngCompressionMode
    ),
    pngOptimization: sanitizePngOptimization(
      payload?.pngOptimization ?? DEFAULTS.pngOptimization
    ),
    enableUploadCompression: Boolean(
      payload?.enableUploadCompression ?? DEFAULTS.enableUploadCompression
    ),
    maxConcurrentUploads: sanitizeConcurrency(
      concurrencyFromBackend ?? DEFAULTS.maxConcurrentUploads
    ),
  };
}

function createStore() {
  const ready = ref(false);
  const loading = ref(false);
  const lastError = ref<string | null>(null);

  let hydrating = true;
  let persistTimer: ReturnType<typeof setTimeout> | null = null;

  // 内部存储实际值
  const internalState: PersistedSettings = { ...DEFAULTS };

  // 创建自动保存的 customRef
  function createAutoSaveRef<T>(
    key: keyof PersistedSettings,
    sanitize?: (value: T) => T
  ) {
    return customRef<T>((track, trigger) => {
      return {
        get() {
          track();
          return internalState[key] as T;
        },
        set(newValue: T) {
          // 应用清理函数（如果有）
          const sanitized = sanitize ? sanitize(newValue) : newValue;

          // 更新内部状态
          (internalState[key] as T) = sanitized;

          // 触发响应式更新
          trigger();

          // 如果不在加载中，触发保存
          if (!hydrating && ready.value) {
            void debug(
              `[settings] ${key} changed to ${sanitized}, scheduling persist`
            );
            schedulePersist();
          }
        },
      };
    });
  }

  async function load() {
    if (loading.value) return;
    loading.value = true;
    hydrating = true;
    lastError.value = null;
    try {
      const payload = await invoke<PersistedSettings>('load_settings');
      await info(`[settings] loaded from backend: ${safeJson(payload)}`);
      const normalized = normalizePayload(payload);
      await info(`[settings] normalized: ${safeJson(normalized)}`);

      // 直接更新内部状态，不触发 setter
      Object.assign(internalState, normalized);

      await info(`[settings] state after load: ${safeJson(internalState)}`);
    } catch (err) {
      await logError(`[settings] load failed: ${describeError(err)}`);
      lastError.value = err instanceof Error ? err.message : String(err);
      Object.assign(internalState, DEFAULTS);
    } finally {
      loading.value = false;
      hydrating = false;
      ready.value = true;
    }
  }

  async function persist() {
    const payload: PersistedSettings = {
      quality: sanitizeQuality(internalState.quality),
      convertToWebp: internalState.convertToWebp,
      pngCompressionMode: sanitizePngMode(internalState.pngCompressionMode),
      pngOptimization: sanitizePngOptimization(internalState.pngOptimization),
      enableUploadCompression: Boolean(internalState.enableUploadCompression),
      maxConcurrentUploads: sanitizeConcurrency(
        internalState.maxConcurrentUploads
      ),
    };
    try {
      await debug(`[settings] persist: saving ${safeJson(payload)}`);
      await invoke('save_settings', { settings: payload });
      await info('[settings] persist: save_settings success');
      lastError.value = null;
    } catch (err) {
      await logError(`[settings] save failed: ${describeError(err)}`);
      lastError.value = err instanceof Error ? err.message : String(err);
    }
  }

  function schedulePersist() {
    if (hydrating) {
      void debug(`[settings] schedulePersist ignored: still hydrating`);
      return;
    }
    if (!ready.value) {
      void debug(`[settings] schedulePersist ignored: not ready yet`);
      return;
    }

    void debug(`[settings] schedulePersist: will persist in 400ms`);
    if (persistTimer) clearTimeout(persistTimer);
    persistTimer = setTimeout(() => {
      persistTimer = null;
      void persist();
    }, 400);
  }

  // 启动加载
  void load();

  // 创建所有的 auto-save refs
  const quality = createAutoSaveRef<number>('quality', sanitizeQuality);
  const convertToWebp = createAutoSaveRef<boolean>('convertToWebp');
  const pngCompressionMode = createAutoSaveRef<PngCompressionMode>(
    'pngCompressionMode',
    sanitizePngMode
  );
  const pngOptimization = createAutoSaveRef<PngOptimizationLevel>(
    'pngOptimization',
    sanitizePngOptimization
  );
  const enableUploadCompression = createAutoSaveRef<boolean>(
    'enableUploadCompression'
  );
  const maxConcurrentUploads = createAutoSaveRef<number>(
    'maxConcurrentUploads',
    sanitizeConcurrency
  );

  return {
    quality,
    convertToWebp,
    pngCompressionMode,
    pngOptimization,
    enableUploadCompression,
    maxConcurrentUploads,
    ready: readonly(ready),
    loading: readonly(loading),
    error: readonly(lastError),
    reload: load,
  };
}

// 简单的单例式 store，确保各视图共享同一份状态
export function useSettingsStore() {
  if (!singleton) singleton = createStore();
  return singleton;
}
