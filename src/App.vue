<script setup lang="ts">
import { computed, ref, watch, onMounted } from 'vue';
import Sidebar from './components/Sidebar.vue';
import Titlebar from './components/Titlebar.vue';
import SettingsView from './views/SettingsView.vue';
import CompressView from './views/CompressView.vue';
import UploadView from './views/UploadView.vue';
import GalleryView from './views/GalleryView.vue';
import ImageHostSettingsView from './views/ImageHostSettingsView.vue';
import { useImageHostStore } from './stores/imageHosts';
import { useThemeStore } from './stores/theme';
import type { LoadedPlugin } from './plugins/registry';
import { platform } from '@tauri-apps/plugin-os';
import { warn } from '@tauri-apps/plugin-log';

type ViewKey = 'compress' | 'upload' | 'gallery' | 'hosts' | 'settings';

const VIEWS: Record<ViewKey, any> = {
  compress: CompressView,
  upload: UploadView,
  gallery: GalleryView,
  settings: SettingsView,
  hosts: ImageHostSettingsView,
};

const imageHostStore = useImageHostStore();
const themeStore = useThemeStore();

const shellClasses = computed(() => ({
  'theme-dark': themeStore.isDark.value,
  'theme-light': !themeStore.isDark.value,
}));

const MOBILE_PLATFORMS = new Set(['android', 'ios']);
const isMobile = ref(false);

const determinePlatform = async () => {
  if (typeof window === 'undefined') {
    return;
  }

  if (!('__TAURI__' in window)) {
    isMobile.value = false;
    return;
  }

  try {
    const currentPlatform = await platform();
    if (currentPlatform) {
      isMobile.value = MOBILE_PLATFORMS.has(currentPlatform.toLowerCase());
      return;
    }
  } catch (error) {
    // console.warn('Failed to detect platform via Tauri OS plugin', error);
    await warn(
      `[App] Failed to detect platform via Tauri OS plugin: ${String(error)}`
    );
  }

  isMobile.value = false;
};

const current = ref<ViewKey>('upload');
function onNavigate(key: ViewKey) {
  current.value = key;
}
const activeComponent = computed(() => VIEWS[current.value]);

const selectedPluginId = ref<string | null>(null);

type SelectPluginPayload = { id: string; navigate?: boolean } | string;

const pluginList = computed(
  () => imageHostStore.plugins.value as readonly LoadedPlugin[]
);
const pluginLoading = computed(() => imageHostStore.loading.value);

onMounted(() => {
  void determinePlatform();
  void imageHostStore.ensureLoaded();
});

watch(
  pluginList,
  (list) => {
    const entries = list ?? [];
    if (!entries.length) {
      selectedPluginId.value = null;
      return;
    }
    if (!selectedPluginId.value) {
      selectedPluginId.value = entries[0]?.id ?? null;
      return;
    }
    const exists = entries.some(
      (plugin) => plugin.id === selectedPluginId.value
    );
    if (!exists) {
      selectedPluginId.value = entries[0]?.id ?? null;
    }
  },
  { immediate: true }
);

function onSelectPlugin(payload: SelectPluginPayload) {
  const normalized =
    typeof payload === 'string' ? { id: payload, navigate: true } : payload;
  selectedPluginId.value = normalized.id;
  if (normalized.navigate ?? true) {
    current.value = 'hosts';
  }
}

const viewProps = computed(() => {
  if (current.value === 'hosts') {
    return { pluginId: selectedPluginId.value };
  }
  if (current.value === 'upload') {
    return {
      pluginId: selectedPluginId.value,
      onSelectPlugin,
    };
  }
  return {};
});
</script>

<template>
  <div class="app-shell" :class="shellClasses">
    <Titlebar v-if="!isMobile" />
    <div class="layout">
      <Sidebar
        :current="current"
        :plugins="pluginList"
        :selected-plugin-id="selectedPluginId"
        :plugin-loading="pluginLoading"
        @navigate="onNavigate"
        @select-plugin="onSelectPlugin"
      />
      <section class="content">
        <component :is="activeComponent" v-bind="viewProps" />
      </section>
    </div>
  </div>
</template>

<style scoped>
.app-shell {
  height: 100vh;
  display: flex;
  flex-direction: column;
  background: var(--app-background);
  color: var(--text-primary);
  backdrop-filter: blur(28px);
  transition: background 0.3s ease, color 0.3s ease;
}

.layout {
  flex: 1;
  display: flex;
  min-height: 0;
}

.content {
  flex: 1;
  overflow: auto;
  padding: 32px 40px;
  display: flex;
  flex-direction: column;
  backdrop-filter: blur(18px);
}

.content > * {
  flex: 1;
  width: 100%;
  display: flex;
}
</style>

<style>
:root {
  color-scheme: light;
  --app-background: radial-gradient(
      circle at 10% 20%,
      rgba(255, 255, 255, 0.92) 0%,
      rgba(224, 232, 255, 0.88) 35%,
      rgba(243, 246, 255, 0.95) 100%
    ),
    linear-gradient(135deg, rgba(238, 245, 255, 0.72), rgba(212, 221, 255, 0.7));
  --surface-acrylic: rgba(255, 255, 255, 0.55);
  --surface-acrylic-strong: rgba(255, 255, 255, 0.8);
  --surface-panel: rgba(255, 255, 255, 0.78);
  --surface-border: rgba(170, 184, 214, 0.45);
  --surface-border-strong: rgba(30, 42, 70, 0.14);
  --text-primary: #12192f;
  --text-secondary: rgba(18, 25, 47, 0.65);
  --icon-muted: rgba(18, 25, 47, 0.55);
  --accent: #4f7cff;
  --accent-soft: rgba(79, 124, 255, 0.15);
  --sidebar-background: linear-gradient(
    180deg,
    rgba(255, 255, 255, 0.38),
    rgba(233, 238, 255, 0.42)
  );
  --sidebar-border: rgba(132, 146, 182, 0.32);
  --sidebar-hover: rgba(79, 124, 255, 0.16);
  --sidebar-active: rgba(79, 124, 255, 0.24);
  --sidebar-text: rgba(18, 25, 47, 0.86);
  --sidebar-text-muted: rgba(18, 25, 47, 0.62);
  --danger: #e16464;
  --danger-soft: rgba(225, 100, 100, 0.22);
  --action-hover: rgba(46, 62, 105, 0.12);
  --action-active: rgba(46, 62, 105, 0.18);
  --shadow-soft: 0 18px 38px rgba(15, 27, 53, 0.12);
  --shadow-strong: 0 24px 52px rgba(15, 27, 53, 0.16);
}

:root[data-theme='dark'] {
  color-scheme: dark;
  --app-background: radial-gradient(
      circle at 12% 18%,
      rgba(36, 43, 68, 0.92) 0%,
      rgba(19, 24, 38, 0.94) 45%,
      rgba(8, 10, 18, 0.95) 100%
    ),
    linear-gradient(140deg, rgba(17, 21, 33, 0.9), rgba(10, 12, 22, 0.92));
  --surface-acrylic: rgba(24, 30, 48, 0.58);
  --surface-acrylic-strong: rgba(28, 34, 56, 0.74);
  --surface-panel: rgba(26, 32, 52, 0.72);
  --surface-border: rgba(108, 126, 170, 0.25);
  --surface-border-strong: rgba(0, 0, 0, 0.6);
  --text-primary: #e7ecff;
  --text-secondary: rgba(216, 222, 255, 0.7);
  --icon-muted: rgba(214, 221, 255, 0.7);
  --accent: #7aa3ff;
  --accent-soft: rgba(122, 163, 255, 0.22);
  --sidebar-background: linear-gradient(
    180deg,
    rgba(32, 37, 58, 0.82),
    rgba(12, 15, 28, 0.88)
  );
  --sidebar-border: rgba(120, 140, 190, 0.28);
  --sidebar-hover: rgba(122, 163, 255, 0.18);
  --sidebar-active: rgba(122, 163, 255, 0.28);
  --sidebar-text: rgba(231, 236, 255, 0.92);
  --sidebar-text-muted: rgba(214, 221, 255, 0.7);
  --danger: #ff7979;
  --danger-soft: rgba(255, 121, 121, 0.26);
  --action-hover: rgba(255, 255, 255, 0.12);
  --action-active: rgba(255, 255, 255, 0.18);
  --shadow-soft: 0 24px 44px rgba(4, 8, 16, 0.45);
  --shadow-strong: 0 30px 66px rgba(2, 4, 10, 0.66);
}

html,
body,
#app {
  margin: 0;
  padding: 0;
  height: 100%;
}

body {
  background: var(--app-background);
  color: var(--text-primary);
  font-family: 'Inter', 'Segoe UI', 'PingFang SC', -apple-system,
    BlinkMacSystemFont, 'Helvetica Neue', Arial, sans-serif;
  transition: background 0.3s ease, color 0.3s ease;
  position: relative;
  overflow: hidden;
}

body::before {
  content: '';
  position: fixed;
  inset: 0;
  backdrop-filter: blur(40px) saturate(1.15);
  pointer-events: none;
  z-index: -1;
}

* {
  box-sizing: border-box;
}

button {
  cursor: pointer;
  font-family: inherit;
}
</style>
