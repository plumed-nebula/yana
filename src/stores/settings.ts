import { reactive, watch, ref, readonly, toRefs } from 'vue';
import { invoke } from '@tauri-apps/api/core';

type PngCompressionMode = 'lossy' | 'lossless';
type PngOptimizationLevel = 'best' | 'default' | 'fast';

type PersistedSettings = {
  quality: number;
  convertToWebp: boolean;
  forceAnimatedWebp: boolean;
  pngCompressionMode: PngCompressionMode;
  pngOptimization: PngOptimizationLevel;
};

type SettingsState = PersistedSettings;

const DEFAULTS: PersistedSettings = {
  quality: 80,
  convertToWebp: false,
  forceAnimatedWebp: false,
  pngCompressionMode: 'lossless',
  pngOptimization: 'default',
};

let singleton: ReturnType<typeof createStore> | null = null;

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

function normalizePayload(
  payload: Partial<PersistedSettings> | null | undefined
): PersistedSettings {
  return {
    quality: sanitizeQuality(payload?.quality ?? DEFAULTS.quality),
    convertToWebp: Boolean(payload?.convertToWebp ?? DEFAULTS.convertToWebp),
    forceAnimatedWebp: Boolean(
      payload?.forceAnimatedWebp ?? DEFAULTS.forceAnimatedWebp
    ),
    pngCompressionMode: sanitizePngMode(
      payload?.pngCompressionMode ?? DEFAULTS.pngCompressionMode
    ),
    pngOptimization: sanitizePngOptimization(
      payload?.pngOptimization ?? DEFAULTS.pngOptimization
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

  async function load() {
    if (loading.value) return;
    loading.value = true;
    hydrating = true;
    lastError.value = null;
    try {
      const payload = await invoke<PersistedSettings>('load_settings');
      console.log('[settings] loaded from backend:', payload);
      const normalized = normalizePayload(payload);
      console.log('[settings] normalized:', normalized);
      state.quality = normalized.quality;
      state.convertToWebp = normalized.convertToWebp;
      state.forceAnimatedWebp = normalized.forceAnimatedWebp;
      state.pngCompressionMode = normalized.pngCompressionMode;
      state.pngOptimization = normalized.pngOptimization;
      console.log('[settings] state after load:', { ...state });
    } catch (err) {
      console.error('[settings] load failed', err);
      lastError.value = err instanceof Error ? err.message : String(err);
      // 加载失败时回退到默认值
      Object.assign(state, DEFAULTS);
    } finally {
      loading.value = false;
      hydrating = false;
      ready.value = true;
    }
  }

  async function persist() {
    const payload: PersistedSettings = {
      quality: sanitizeQuality(state.quality),
      convertToWebp: state.convertToWebp,
      forceAnimatedWebp: state.forceAnimatedWebp,
      pngCompressionMode: sanitizePngMode(state.pngCompressionMode),
      pngOptimization: sanitizePngOptimization(state.pngOptimization),
    };
    try {
      await invoke('save_settings', { settings: payload });
      lastError.value = null;
    } catch (err) {
      console.error('[settings] save failed', err);
      lastError.value = err instanceof Error ? err.message : String(err);
    }
  }

  function schedulePersist() {
    if (!ready.value || hydrating) return;
    if (persistTimer) clearTimeout(persistTimer);
    persistTimer = setTimeout(() => {
      persistTimer = null;
      void persist();
    }, 400);
  }

  // 独立监听每个字段，避免循环触发
  watch(
    () => state.quality,
    (newVal) => {
      console.log('[settings] quality changed:', newVal);
      // 实时校验并更新
      const sanitized = sanitizeQuality(newVal);
      if (sanitized !== newVal) {
        console.log('[settings] quality sanitized:', sanitized);
        state.quality = sanitized;
      }
      schedulePersist();
    }
  );

  watch(
    () => state.convertToWebp,
    (enabled) => {
      console.log('[settings] convertToWebp changed:', enabled);
      // 关闭 WebP 时自动取消动图强制转换
      if (!enabled && state.forceAnimatedWebp) {
        console.log('[settings] auto-disabling forceAnimatedWebp');
        state.forceAnimatedWebp = false;
      }
      schedulePersist();
    }
  );

  watch(
    () => state.forceAnimatedWebp,
    (enabled) => {
      console.log('[settings] forceAnimatedWebp changed:', enabled);
      schedulePersist();
    }
  );

  watch(
    () => state.pngCompressionMode,
    (mode) => {
      const sanitized = sanitizePngMode(mode);
      if (sanitized !== mode) {
        state.pngCompressionMode = sanitized;
        return;
      }
      schedulePersist();
    }
  );

  watch(
    () => state.pngOptimization,
    (opt) => {
      const sanitized = sanitizePngOptimization(opt);
      if (sanitized !== opt) {
        state.pngOptimization = sanitized;
        return;
      }
      schedulePersist();
    }
  );

  void load();

  const refs = toRefs(state);

  return {
    quality: refs.quality,
    convertToWebp: refs.convertToWebp,
    forceAnimatedWebp: refs.forceAnimatedWebp,
    pngCompressionMode: refs.pngCompressionMode,
    pngOptimization: refs.pngOptimization,
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
