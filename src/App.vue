<script setup lang="ts">
import { computed, ref, watch, onMounted } from 'vue';
import Sidebar from './components/Sidebar.vue';
import SettingsView from './views/SettingsView.vue';
import CompressView from './views/CompressView.vue';
import UploadView from './views/UploadView.vue';
import GalleryView from './views/GalleryView.vue';
import ImageHostSettingsView from './views/ImageHostSettingsView.vue';
import { useImageHostStore } from './stores/imageHosts';
import type { LoadedPlugin } from './plugins/registry';

type ViewKey = 'compress' | 'upload' | 'gallery' | 'hosts' | 'settings';

const VIEWS: Record<ViewKey, any> = {
  compress: CompressView,
  upload: UploadView,
  gallery: GalleryView,
  settings: SettingsView,
  hosts: ImageHostSettingsView,
};

const imageHostStore = useImageHostStore();

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
</template>

<style scoped>
.layout {
  display: flex;
  height: 100vh;
  background: linear-gradient(135deg, #f4f6fb 0%, #dde2f3 40%, #f4f6fb 100%);
}

.content {
  flex: 1;
  overflow: auto;
  padding: 32px 40px;
  display: flex;
  flex-direction: column;
}

.content > * {
  flex: 1;
  width: 100%;
  display: flex;
}
</style>

<style>
:root {
  color: #0f0f0f;
  background-color: #f6f6f6;
}
@media (prefers-color-scheme: dark) {
  :root {
    color: #f6f6f6;
    background-color: #2f2f2f;
  }
}
html,
body,
#app {
  margin: 0;
  padding: 0;
  height: 100%;
  overflow: hidden;
}

body {
  background: linear-gradient(135deg, #f4f6fb 0%, #dde2f3 40%, #f4f6fb 100%);
  color: inherit;
}

*,
*::before,
*::after {
  box-sizing: border-box;
}

button {
  cursor: pointer;
}
</style>
