import { reactive, ref, readonly, watch } from 'vue';
import { invoke } from '@tauri-apps/api/core';
import {
  info,
  warn as logWarn,
  error as logError,
} from '@tauri-apps/plugin-log';
import {
  createPluginRuntimeContext,
  type PluginParameterDescriptor,
} from '../types/imageHostPlugin';
import {
  loadPlugin,
  getPluginEntries,
  type LoadedPlugin,
} from '../plugins/registry';

const STORAGE_PREFIX = 'image-host-settings:';

interface PluginSettingsState {
  values: Record<string, unknown>;
  saving: boolean;
  lastSavedAt: number | null;
  error: string | null;
}

let singleton: ReturnType<typeof createStore> | null = null;

function applyDefaults(
  descriptors: PluginParameterDescriptor[],
  stored: Record<string, unknown> | null
): Record<string, unknown> {
  const result: Record<string, unknown> = {};
  for (const descriptor of descriptors) {
    const key = descriptor.key;
    if (stored && key in stored) {
      result[key] = stored[key];
      continue;
    }
    if (descriptor.defaultValue !== undefined) {
      result[key] = descriptor.defaultValue;
    } else {
      switch (descriptor.type) {
        case 'number':
          result[key] = 0;
          break;
        case 'boolean':
          result[key] = false;
          break;
        default:
          result[key] = '';
      }
    }
  }
  if (stored) {
    for (const [key, value] of Object.entries(stored)) {
      if (!(key in result)) {
        result[key] = value;
      }
    }
  }
  return result;
}

function parseStored(raw: string | null): Record<string, unknown> | null {
  if (!raw) return null;
  try {
    const parsed = JSON.parse(raw);
    if (parsed && typeof parsed === 'object' && !Array.isArray(parsed)) {
      return parsed as Record<string, unknown>;
    }
  } catch (error) {
    void logError(`[imageHosts] 存储解析失败: ${error}`);
  }
  return null;
}

function normalizeBackendPayload(
  payload: unknown
): Record<string, unknown> | null {
  if (payload && typeof payload === 'object' && !Array.isArray(payload)) {
    return payload as Record<string, unknown>;
  }
  if (payload !== null && payload !== undefined) {
    void logWarn(
      `[imageHosts] 后端返回的配置不是对象，将忽略: ${JSON.stringify(payload)}`
    );
  }
  return null;
}

function cloneValues(values: Record<string, unknown>): Record<string, unknown> {
  return JSON.parse(JSON.stringify(values));
}

function assignValues(
  target: Record<string, unknown>,
  source: Record<string, unknown>
) {
  for (const key of Object.keys(target)) {
    if (!(key in source)) {
      delete target[key];
    }
  }
  for (const [key, value] of Object.entries(source)) {
    target[key] = value;
  }
}

function createStore() {
  const plugins = ref<LoadedPlugin[]>([]);
  const loading = ref(false);
  const ready = ref(false);
  const error = ref<string | null>(null);
  const settings = reactive<Record<string, PluginSettingsState>>({});

  const runtime = createPluginRuntimeContext();
  const timers = new Map<string, ReturnType<typeof setTimeout>>();
  const hydrating = new Set<string>();
  const loadErrors = new Map<string, string>();
  let hasLoaded = false;
  let loadPromise: Promise<void> | null = null;

  async function hydrateSettings(plugin: LoadedPlugin) {
    const state = settings[plugin.id];
    if (!state) return;

    hydrating.add(plugin.id);
    let stored: Record<string, unknown> | null = null;
    let migratedFromLegacy = false;
    let backendError: string | null = null;

    try {
      try {
        const payload = await invoke<Record<string, unknown> | null>(
          'load_image_host_settings',
          { pluginId: plugin.id }
        );
        stored = normalizeBackendPayload(payload);
      } catch (err) {
        backendError = err instanceof Error ? err.message : String(err);
        await logError(
          `[imageHosts] 读取后端配置失败 (${plugin.id}): ${backendError}`
        );
      }

      if (!stored) {
        const legacyRaw = localStorage.getItem(STORAGE_PREFIX + plugin.id);
        const legacy = parseStored(legacyRaw);
        if (legacy) {
          stored = legacy;
          migratedFromLegacy = true;
          await info(`[imageHosts] 迁移旧版配置到后端: ${plugin.id}`);
        } else if (legacyRaw) {
          await logWarn(
            `[imageHosts] 遗留的本地配置无效，已忽略 (${plugin.id})`
          );
          localStorage.removeItem(STORAGE_PREFIX + plugin.id);
        }
      }

      const normalized = applyDefaults(plugin.parameters ?? [], stored);
      assignValues(state.values, normalized);

      if (stored) {
        state.lastSavedAt = Date.now();
      }
      state.error = null;

      if (backendError) {
        loadErrors.set(plugin.id, backendError);
        error.value = backendError;
      } else {
        loadErrors.delete(plugin.id);
        if (loadErrors.size === 0) {
          error.value = null;
        } else {
          const first = loadErrors.values().next().value ?? null;
          error.value = first;
        }
      }

      if (migratedFromLegacy) {
        try {
          await persist(plugin.id);
        } catch {
          // persist 已记录错误，继续抛出由 watcher 处理
        }
      }
    } finally {
      hydrating.delete(plugin.id);
    }
  }

  async function persist(id: string) {
    const target = settings[id];
    if (!target) return;
    try {
      target.saving = true;
      const payload = cloneValues(target.values);
      await invoke('save_image_host_settings', {
        pluginId: id,
        values: payload,
      });
      target.lastSavedAt = Date.now();
      target.error = null;
      localStorage.removeItem(STORAGE_PREFIX + id);
      await info(`[imageHosts] 保存 ${id} 成功`);
    } catch (err) {
      target.error = err instanceof Error ? err.message : String(err);
      await logError(`[imageHosts] 保存 ${id} 失败: ${target.error}`);
    } finally {
      target.saving = false;
    }
  }

  function schedulePersist(id: string) {
    const timer = timers.get(id);
    if (timer) clearTimeout(timer);
    const handle = setTimeout(() => {
      timers.delete(id);
      void persist(id);
    }, 400);
    timers.set(id, handle);
  }

  async function ensureSettings(plugin: LoadedPlugin) {
    if (!settings[plugin.id]) {
      const defaults = applyDefaults(plugin.parameters ?? [], null);
      const values = reactive({ ...defaults });

      const state: PluginSettingsState = reactive({
        values,
        saving: false,
        lastSavedAt: null,
        error: null,
      });

      hydrating.add(plugin.id);
      watch(
        values,
        () => {
          if (hydrating.has(plugin.id)) return;
          schedulePersist(plugin.id);
        },
        { deep: true }
      );
      hydrating.delete(plugin.id);

      settings[plugin.id] = state;
    }

    await hydrateSettings(plugin);
  }

  async function loadAll(force = false) {
    if (hasLoaded && !force) {
      return;
    }
    if (loadPromise) {
      return loadPromise;
    }

    loadPromise = (async () => {
      try {
        loading.value = true;
        error.value = null;
        const entries = await getPluginEntries(force);
        const loaded: LoadedPlugin[] = [];
        for (const entry of entries) {
          try {
            const plugin = await loadPlugin(entry);
            loaded.push(plugin);
          } catch (err) {
            const message = err instanceof Error ? err.message : String(err);
            await logError(
              `[imageHosts] 加载插件 ${entry.id} 失败: ${message}`
            );
          }
        }
        plugins.value = loaded;
        await Promise.all(loaded.map((plugin) => ensureSettings(plugin)));
      } catch (err) {
        error.value = err instanceof Error ? err.message : String(err);
      } finally {
        loading.value = false;
        ready.value = true;
        hasLoaded = error.value === null;
        loadPromise = null;
      }
    })();

    return loadPromise;
  }

  async function ensureLoaded() {
    await loadAll();
  }

  function getPluginById(id: string): LoadedPlugin | undefined {
    return plugins.value.find((plugin) => plugin.id === id);
  }

  function getSettingsState(id: string): PluginSettingsState | undefined {
    return settings[id];
  }

  function updateSetting(id: string, key: string, value: unknown) {
    const target = settings[id];
    if (!target) return;
    target.values[key] = value;
    schedulePersist(id);
  }

  function saveNow(id: string) {
    void persist(id);
  }

  void ensureLoaded();

  return {
    plugins: readonly(plugins),
    loading: readonly(loading),
    ready: readonly(ready),
    error: readonly(error),
    runtime,
    ensureLoaded,
    reload: () => loadAll(true),
    getPluginById,
    getSettingsState,
    updateSetting,
    saveNow,
  };
}

export type ImageHostStore = ReturnType<typeof createStore>;

export function useImageHostStore(): ImageHostStore {
  if (!singleton) singleton = createStore();
  return singleton;
}
