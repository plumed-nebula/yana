import { reactive, ref, readonly, watch } from 'vue';
import { info, error as logError } from '@tauri-apps/plugin-log';
import {
  createPluginRuntimeContext,
  type PluginParameterDescriptor,
} from '../types/imageHostPlugin';
import {
  loadPlugin,
  PLUGIN_ENTRIES,
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

function createStore() {
  const plugins = ref<LoadedPlugin[]>([]);
  const loading = ref(true);
  const ready = ref(false);
  const error = ref<string | null>(null);
  const settings = reactive<Record<string, PluginSettingsState>>({});

  const runtime = createPluginRuntimeContext();
  const timers = new Map<string, ReturnType<typeof setTimeout>>();
  const hydrating = new Set<string>();

  async function persist(id: string) {
    const target = settings[id];
    if (!target) return;
    try {
      target.saving = true;
      localStorage.setItem(STORAGE_PREFIX + id, JSON.stringify(target.values));
      target.lastSavedAt = Date.now();
      target.error = null;
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

  function ensureSettings(plugin: LoadedPlugin) {
    if (settings[plugin.id]) return;

    const stored = parseStored(
      localStorage.getItem(STORAGE_PREFIX + plugin.id)
    );
    const values = reactive(applyDefaults(plugin.parameters ?? [], stored));

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

  async function loadAll() {
    try {
      loading.value = true;
      error.value = null;
      const loaded: LoadedPlugin[] = [];
      for (const entry of PLUGIN_ENTRIES) {
        try {
          const plugin = await loadPlugin(entry);
          loaded.push(plugin);
        } catch (err) {
          const message = err instanceof Error ? err.message : String(err);
          await logError(`[imageHosts] 加载插件 ${entry.id} 失败: ${message}`);
        }
      }
      plugins.value = loaded;
      for (const plugin of loaded) {
        ensureSettings(plugin);
      }
    } catch (err) {
      error.value = err instanceof Error ? err.message : String(err);
    } finally {
      loading.value = false;
      ready.value = true;
    }
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

  void loadAll();

  return {
    plugins: readonly(plugins),
    loading: readonly(loading),
    ready: readonly(ready),
    error: readonly(error),
    runtime,
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
