<script setup lang="ts">
import { computed, ref, type Component } from 'vue';
import {
  Sparkles,
  SquareStack,
  UploadCloud,
  GalleryHorizontal,
  Image as ImageIcon,
  Cog,
  ChevronsLeft,
  ChevronsRight,
  ChevronDown,
  Loader2,
  Check,
} from 'lucide-vue-next';
import type { LoadedPlugin } from '../plugins/registry';

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

const collapsed = ref(false);

const items: Array<{ key: ViewKey; label: string; icon: Component }> = [
  {
    key: 'upload',
    label: '上传',
    icon: UploadCloud,
  },
  {
    key: 'compress',
    label: '压缩',
    icon: SquareStack,
  },
  {
    key: 'gallery',
    label: '图库',
    icon: GalleryHorizontal,
  },
  {
    key: 'hosts',
    label: '图床',
    icon: ImageIcon,
  },
  {
    key: 'settings',
    label: '设置',
    icon: Cog,
  },
];

function toggleCollapsed() {
  collapsed.value = !collapsed.value;
}

function onSelect(key: ViewKey) {
  if (key === 'hosts' && collapsed.value) {
    collapsed.value = false;
  }
  emit('navigate', key);
}

const pluginListVisible = computed(
  () => props.current === 'hosts' && !collapsed.value
);

function handlePluginClick(id: string) {
  emit('select-plugin', { id, navigate: true });
}

const sidebarWidth = computed(() => (collapsed.value ? '64px' : '240px'));
</script>

<template>
  <aside
    class="sidebar"
    :class="{ collapsed }"
    :style="{ width: sidebarWidth }"
  >
    <div class="brand" @click="collapsed = false">
      <Sparkles class="brand-icon" :size="22" />
      <transition name="fade">
        <span v-if="!collapsed" class="brand-title">Yana</span>
      </transition>
    </div>

    <nav class="nav">
      <div v-for="item in items" :key="item.key" class="nav-group">
        <button
          type="button"
          :class="['nav-item', { active: props.current === item.key }]"
          @click="onSelect(item.key)"
          :title="collapsed ? item.label : undefined"
        >
          <component :is="item.icon" class="icon" :size="20" />
          <transition name="fade">
            <span v-if="!collapsed" class="label">{{ item.label }}</span>
          </transition>
          <ChevronDown
            v-if="item.key === 'hosts' && !collapsed"
            class="chevron"
            :size="18"
            :class="{ open: pluginListVisible }"
          />
        </button>

        <transition name="slide-fade">
          <div
            v-if="item.key === 'hosts' && pluginListVisible"
            class="plugin-menu"
          >
            <div
              v-if="props.pluginLoading && !props.plugins.length"
              class="plugin-empty"
            >
              <Loader2 class="spinner" :size="18" />
              <span>正在加载插件…</span>
            </div>
            <template v-else>
              <button
                v-for="plugin in props.plugins"
                :key="plugin.id"
                type="button"
                :class="[
                  'plugin-item',
                  { selected: plugin.id === props.selectedPluginId },
                ]"
                @click="handlePluginClick(plugin.id)"
              >
                <div class="item-main">
                  <span class="name">{{ plugin.name }}</span>
                  <span class="meta">{{ plugin.author ?? '官方提供' }}</span>
                </div>
                <Check
                  v-if="plugin.id === props.selectedPluginId"
                  :size="18"
                  class="check"
                />
              </button>
              <div v-if="!props.plugins.length" class="plugin-empty">
                <span>暂无可用插件</span>
              </div>
            </template>
          </div>
        </transition>
      </div>
    </nav>

    <button class="collapse" type="button" @click="toggleCollapsed">
      <component :is="collapsed ? ChevronsRight : ChevronsLeft" :size="18" />
      <transition name="fade">
        <span v-if="!collapsed" class="label">{{
          collapsed ? '展开' : '折叠'
        }}</span>
      </transition>
    </button>
  </aside>
</template>

<style scoped>
.sidebar {
  display: flex;
  flex-direction: column;
  border-right: 1px solid var(--sidebar-border);
  background: var(--sidebar-background);
  color: var(--sidebar-text);
  transition: width 0.2s ease, box-shadow 0.2s ease;
  box-shadow: inset -1px 0 0 rgba(255, 255, 255, 0.06);
  position: relative;
  backdrop-filter: blur(24px) saturate(1.15);
}

.sidebar.collapsed {
  box-shadow: inset -1px 0 0 rgba(255, 255, 255, 0.08);
}

.brand {
  display: flex;
  align-items: center;
  gap: 12px;
  padding: 24px 18px 16px;
  font-size: 18px;
  font-weight: 600;
  letter-spacing: 0.8px;
  cursor: pointer;
  user-select: none;
  color: var(--sidebar-text);
}

.brand-icon {
  color: var(--accent);
}

.brand-title {
  text-transform: uppercase;
}

.nav {
  display: flex;
  flex-direction: column;
  gap: 8px;
  padding: 8px 10px;
  flex: 1;
}

.nav-group {
  display: flex;
  flex-direction: column;
  gap: 6px;
}

.nav-item {
  display: flex;
  align-items: center;
  justify-content: flex-start;
  border: none;
  padding: 10px 12px;
  gap: 12px;
  border-radius: 12px;
  color: var(--sidebar-text-muted);
  background: transparent;
  transition: background 0.18s ease, transform 0.18s ease, color 0.18s ease;
}

.nav-item .icon {
  flex-shrink: 0;
  color: var(--icon-muted);
}

.nav-item .label {
  flex: 1;
  text-align: left;
}

.chevron {
  margin-left: auto;
  transition: transform 0.2s ease;
  color: inherit;
}

.chevron.open {
  transform: rotate(180deg);
}

.nav-item:hover {
  background: var(--sidebar-hover);
  transform: translateX(2px);
  color: var(--sidebar-text);
}

.nav-item.active {
  background: var(--sidebar-active);
  color: var(--sidebar-text);
  box-shadow: var(--shadow-soft);
}

.nav-item.active .icon,
.nav-item:hover .icon {
  color: var(--accent);
}

.plugin-menu {
  display: flex;
  flex-direction: column;
  gap: 6px;
  background: var(--surface-acrylic);
  border-radius: 16px;
  padding: 10px 10px 12px;
  border: 1px solid var(--sidebar-border);
  box-shadow: inset 0 0 0 1px rgba(255, 255, 255, 0.04);
}

.plugin-item {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 12px;
  border: none;
  padding: 8px 10px;
  border-radius: 12px;
  background: transparent;
  color: var(--sidebar-text-muted);
  transition: background 0.18s ease, transform 0.18s ease, color 0.18s ease;
}

.plugin-item .item-main {
  display: flex;
  flex-direction: column;
  gap: 4px;
  text-align: left;
}

.plugin-item .name {
  font-size: 14px;
  font-weight: 600;
  color: var(--sidebar-text);
}

.plugin-item .meta {
  font-size: 12px;
  color: var(--sidebar-text-muted);
}

.plugin-item .check {
  color: var(--accent);
}

.plugin-item:hover {
  background: var(--sidebar-hover);
  color: var(--sidebar-text);
  transform: translateX(2px);
}

.plugin-item.selected {
  background: var(--accent-soft);
  box-shadow: 0 8px 18px rgba(0, 0, 0, 0.15);
  color: var(--sidebar-text);
}

.plugin-empty {
  display: flex;
  align-items: center;
  gap: 8px;
  font-size: 13px;
  color: var(--sidebar-text-muted);
  padding: 10px 4px;
}

.spinner {
  animation: spin 1s linear infinite;
  color: var(--accent);
}

.collapse {
  margin: 12px;
  border: 1px solid var(--sidebar-border);
  background: var(--surface-acrylic);
  color: var(--sidebar-text);
  border-radius: 12px;
  padding: 10px 12px;
  display: flex;
  align-items: center;
  gap: 10px;
  transition: background 0.2s ease, border-color 0.2s ease, transform 0.2s ease;
}

.collapse:hover {
  background: var(--sidebar-hover);
  border-color: var(--accent);
  transform: translateY(-1px);
}

.fade-enter-active,
.fade-leave-active {
  transition: opacity 0.15s ease;
}

.fade-enter-from,
.fade-leave-to {
  opacity: 0;
}

.slide-fade-enter-active,
.slide-fade-leave-active {
  transition: opacity 0.2s ease, transform 0.2s ease;
}

.slide-fade-enter-from,
.slide-fade-leave-to {
  opacity: 0;
  transform: translateY(-4px);
}

.label {
  font-size: 14px;
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
