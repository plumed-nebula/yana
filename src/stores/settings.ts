import { reactive, watch, ref, readonly, toRefs } from 'vue';
import { invoke } from '@tauri-apps/api/core';
import { info, debug, error as logError } from '@tauri-apps/plugin-log';

type PngCompressionMode = 'lossy' | 'lossless';
type PngOptimizationLevel = 'best' | 'default' | 'fast';

type PersistedSettings = {
  quality: number;
  convertToWebp: boolean;
  forceAnimatedWebp: boolean;
  pngCompressionMode: PngCompressionMode;
  pngOptimization: PngOptimizationLevel;
  enableUploadCompression: boolean;
  maxConcurrentUploads: number;
};

type SettingsState = PersistedSettings;

const DEFAULTS: PersistedSettings = {
  quality: 80,
  convertToWebp: false,
  forceAnimatedWebp: false,
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
    forceAnimatedWebp: Boolean(
      payload?.forceAnimatedWebp ?? DEFAULTS.forceAnimatedWebp
    ),
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
  const state = reactive<SettingsState>({ ...DEFAULTS });
  const ready = ref(false);
  const loading = ref(false);
  const lastError = ref<string | null>(null);

  let hydrating = true;
  let persistTimer: ReturnType<typeof setTimeout> | null = null;
  let pendingPersist = false;

  async function load() {
    if (loading.value) return;
    loading.value = true;
    hydrating = true;
    pendingPersist = false;
    lastError.value = null;
    try {
      const payload = await invoke<PersistedSettings>('load_settings');
      await info(`[settings] loaded from backend: ${safeJson(payload)}`);
      const normalized = normalizePayload(payload);
      await info(`[settings] normalized: ${safeJson(normalized)}`);
      state.quality = normalized.quality;
      state.convertToWebp = normalized.convertToWebp;
      state.forceAnimatedWebp = normalized.forceAnimatedWebp;
      state.pngCompressionMode = normalized.pngCompressionMode;
      state.pngOptimization = normalized.pngOptimization;
      state.enableUploadCompression = normalized.enableUploadCompression;
      state.maxConcurrentUploads = normalized.maxConcurrentUploads;
      await info(`[settings] state after load: ${safeJson({ ...state })}`);
    } catch (err) {
      await logError(`[settings] load failed: ${describeError(err)}`);
      lastError.value = err instanceof Error ? err.message : String(err);
      // 加载失败时回退到默认值
      Object.assign(state, DEFAULTS);
    } finally {
      loading.value = false;
      hydrating = false;
      ready.value = true;
      if (pendingPersist) {
        const shouldPersist = pendingPersist;
        pendingPersist = false;
        if (shouldPersist) {
          schedulePersist();
        }
      }
    }
  }

  async function persist() {
    const payload: PersistedSettings = {
      quality: sanitizeQuality(state.quality),
      convertToWebp: state.convertToWebp,
      forceAnimatedWebp: state.forceAnimatedWebp,
      pngCompressionMode: sanitizePngMode(state.pngCompressionMode),
      pngOptimization: sanitizePngOptimization(state.pngOptimization),
      enableUploadCompression: Boolean(state.enableUploadCompression),
      maxConcurrentUploads: sanitizeConcurrency(state.maxConcurrentUploads),
    };
    try {
      await invoke('save_settings', { settings: payload });
      lastError.value = null;
    } catch (err) {
      await logError(`[settings] save failed: ${describeError(err)}`);
      lastError.value = err instanceof Error ? err.message : String(err);
    }
  }

  function schedulePersist() {
    if (!ready.value || hydrating) {
      pendingPersist = true;
      return;
    }
    pendingPersist = false;
    if (persistTimer) clearTimeout(persistTimer);
    persistTimer = setTimeout(() => {
      persistTimer = null;
      pendingPersist = false;
      void persist();
    }, 400);
  }

  // 监听整个 state 对象以简化逻辑并确保触发
  watch(
    () => ({ ...state }),
    (newState, oldState) => {
      const snapshot = {
        new: { ...newState },
        old: { ...oldState },
      };
      void debug(`[settings] state changed: ${safeJson(snapshot)}`);

      // 实时校验并更新
      const sanitizedQuality = sanitizeQuality(newState.quality);
      if (sanitizedQuality !== newState.quality) {
        state.quality = sanitizedQuality;
      }

      const sanitizedConcurrency = sanitizeConcurrency(
        newState.maxConcurrentUploads
      );
      if (sanitizedConcurrency !== newState.maxConcurrentUploads) {
        state.maxConcurrentUploads = sanitizedConcurrency;
      }

      // 关闭 WebP 时自动取消动图强制转换
      if (!newState.convertToWebp && oldState.convertToWebp) {
        if (state.forceAnimatedWebp) {
          void info('[settings] auto-disabling forceAnimatedWebp');
          state.forceAnimatedWebp = false;
        }
      }

      schedulePersist();
    },
    { deep: true }
  );

  void load();

  const refs = toRefs(state);

  return {
    quality: refs.quality,
    convertToWebp: refs.convertToWebp,
    forceAnimatedWebp: refs.forceAnimatedWebp,
    pngCompressionMode: refs.pngCompressionMode,
    pngOptimization: refs.pngOptimization,
    enableUploadCompression: refs.enableUploadCompression,
    maxConcurrentUploads: refs.maxConcurrentUploads,
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
