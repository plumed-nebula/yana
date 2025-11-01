<script setup lang="ts">
import { defineEmits, defineProps, ref } from 'vue';
import {
  SquareStack,
  UploadCloud,
  GalleryHorizontal,
  Image as ImageIcon,
  Cog,
} from 'lucide-vue-next';
import type { Component } from 'vue';
import type { LoadedPlugin } from '../plugins/registry';
import PluginPicker from './PluginPicker.vue';

type ViewKey = 'compress' | 'upload' | 'gallery' | 'settings' | 'hosts';

const props = defineProps<{
  current: ViewKey;
  plugins: readonly LoadedPlugin[];
  selectedPluginId: string | null;
  pluginLoading: boolean;
}>();

const emit = defineEmits<{
  (e: 'navigate', key: ViewKey): void;
  (e: 'select-plugin', payload: { id: string; navigate?: boolean }): void;
}>();

const items: Array<{ key: ViewKey; label: string; icon: Component }> = [
  { key: 'upload', label: '上传', icon: UploadCloud },
  { key: 'compress', label: '压缩', icon: SquareStack },
  { key: 'gallery', label: '图库', icon: GalleryHorizontal },
  { key: 'hosts', label: '图床', icon: ImageIcon },
  { key: 'settings', label: '设置', icon: Cog },
];

// component-local state for picker visibility
const pickerVisible = ref(false);

// override onSelect to open plugin picker when hosts clicked
function onSelect(key: ViewKey) {
  if (key === 'hosts') {
    pickerVisible.value = true;
    return;
  }
  emit('navigate', key);
}
</script>

<template>
  <nav class="bottom-bar">
    <button
      v-for="item in items"
      :key="item.key"
      type="button"
      :class="['bar-item', { active: props.current === item.key }]"
      @click="onSelect(item.key)"
      :title="item.label"
    >
      <component :is="item.icon" class="icon" :size="20" />
      <span class="label">{{ item.label }}</span>
    </button>
  </nav>

  <PluginPicker
    :visible="pickerVisible"
    :plugins="props.plugins"
    :selectedPluginId="props.selectedPluginId"
    :loading="props.pluginLoading"
    @close="pickerVisible = false"
    @select="
      (id) => {
        pickerVisible = false;
        emit('select-plugin', { id, navigate: true });
      }
    "
  />
</template>

<style scoped>
.bottom-bar {
  position: fixed;
  left: 0;
  right: 0;
  bottom: 0;
  height: 64px;
  display: flex;
  justify-content: space-around;
  align-items: center;
  background: var(--surface-panel);
  border-top: 1px solid var(--surface-border);
  backdrop-filter: blur(12px) saturate(1.1);
  z-index: 1200;
}

.bar-item {
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  gap: 4px;
  border: none;
  background: transparent;
  color: var(--sidebar-text-muted);
  padding: 6px 8px;
  border-radius: 8px;
}

.bar-item .icon {
  color: var(--icon-muted);
}

.bar-item .label {
  font-size: 12px;
  color: var(--sidebar-text-muted);
}

.bar-item.active {
  color: var(--sidebar-text);
}

.bar-item.active .icon {
  color: var(--accent);
}

.bar-item:hover {
  background: var(--sidebar-hover);
  transform: translateY(-2px);
}
</style>
