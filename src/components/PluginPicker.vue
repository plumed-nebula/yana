<script setup lang="ts">
// intentionally lightweight: no reactive state here, parent controls visibility
import { Loader2, Check } from 'lucide-vue-next';
import type { LoadedPlugin } from '../plugins/registry';

const props = defineProps<{
  visible: boolean;
  plugins: readonly LoadedPlugin[];
  selectedPluginId: string | null;
  loading: boolean;
}>();

const emit = defineEmits<{
  (e: 'close'): void;
  (e: 'select', id: string): void;
}>();

function handleSelect(id: string) {
  emit('select', id);
}

function close() {
  emit('close');
}
</script>

<template>
  <teleport to="body">
    <div v-if="props.visible" class="plugin-picker-overlay" @click.self="close">
      <div class="plugin-picker-sheet">
        <div class="sheet-header">
          <h3>选择图床</h3>
          <button class="close-btn" type="button" @click="close">关闭</button>
        </div>

        <div class="sheet-list">
          <div
            v-if="props.loading && !props.plugins.length"
            class="plugin-empty"
          >
            <Loader2 class="spinner" :size="20" />
            <span>正在加载插件…</span>
          </div>
          <template v-else>
            <button
              v-for="plugin in props.plugins"
              :key="plugin.id"
              type="button"
              class="plugin-item"
              :class="{ selected: plugin.id === props.selectedPluginId }"
              @click="handleSelect(plugin.id)"
            >
              <div class="item-main">
                <div class="name">{{ plugin.name }}</div>
                <div class="meta">{{ plugin.author ?? '官方提供' }}</div>
              </div>
              <Check v-if="plugin.id === props.selectedPluginId" :size="18" />
            </button>
          </template>
        </div>
      </div>
    </div>
  </teleport>
</template>

<style scoped>
.plugin-picker-overlay {
  position: fixed;
  inset: 0;
  background: rgba(0, 0, 0, 0.35);
  display: flex;
  justify-content: center;
  align-items: flex-end;
  z-index: 1400;
}
.plugin-picker-sheet {
  width: 100%;
  max-width: 720px;
  background: var(--surface-panel);
  border-top-left-radius: 16px;
  border-top-right-radius: 16px;
  padding: 12px 16px 28px;
  box-shadow: 0 -20px 40px rgba(0, 0, 0, 0.24);
}
.sheet-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  margin-bottom: 8px;
}
.sheet-header h3 {
  margin: 0;
  font-size: 16px;
}
.close-btn {
  background: transparent;
  border: none;
  color: var(--text-secondary);
}
.sheet-list {
  max-height: 48vh;
  overflow: auto;
  display: flex;
  flex-direction: column;
  gap: 8px;
  padding-top: 8px;
}
.plugin-item {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 12px;
  padding: 10px 12px;
  border-radius: 10px;
  border: 1px solid var(--surface-border);
  background: transparent;
}
.plugin-item.selected {
  background: var(--accent-soft);
}
.plugin-item .item-main .name {
  font-weight: 600;
}
.plugin-item .item-main .meta {
  font-size: 12px;
  color: var(--text-secondary);
}
.plugin-empty {
  display: flex;
  align-items: center;
  gap: 8px;
  color: var(--text-secondary);
}
.spinner {
  animation: spin 1s linear infinite;
  color: var(--accent);
}

@keyframes spin {
  from {
    transform: rotate(0deg);
  }
  to {
    transform: rotate(360deg);
  }
}
</style>
