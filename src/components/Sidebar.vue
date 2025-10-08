<script setup lang="ts">
import { computed, ref, type Component } from 'vue';
import {
  Sparkles,
  SquareStack,
  UploadCloud,
  Image as ImageIcon,
  Cog,
  ChevronsLeft,
  ChevronsRight,
  ChevronDown,
  Loader2,
  Check,
} from 'lucide-vue-next';
import type { LoadedPlugin } from '../plugins/registry';

type ViewKey = 'compress' | 'upload' | 'settings' | 'hosts';

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
    key: 'compress',
    label: '压缩',
    icon: SquareStack,
  },
  {
    key: 'upload',
    label: '上传',
    icon: UploadCloud,
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
  border-right: 1px solid rgba(0, 0, 0, 0.06);
  background: linear-gradient(
    180deg,
    rgba(21, 30, 63, 0.92),
    rgba(6, 10, 19, 0.94)
  );
  color: #fff;
  transition: width 0.2s ease, box-shadow 0.2s ease;
  box-shadow: inset -1px 0 0 rgba(255, 255, 255, 0.05);
  position: relative;
  backdrop-filter: blur(12px);
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
}

.brand-icon {
  color: #f2f5ff;
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
  justify-content: space-between;
  border: none;
  padding: 10px 12px;
  gap: 12px;
  border-radius: 10px;
  color: rgba(255, 255, 255, 0.88);
  background: transparent;
  transition: background 0.15s ease, transform 0.15s ease, color 0.15s ease;
}

.nav-item .icon {
  flex-shrink: 0;
}

.chevron {
  margin-left: auto;
  transition: transform 0.2s ease;
}

.chevron.open {
  transform: rotate(180deg);
}

.nav-item:hover {
  background: rgba(255, 255, 255, 0.12);
  transform: translateX(2px);
}

.nav-item.active {
  background: rgba(255, 255, 255, 0.2);
  color: #fff;
  box-shadow: 0 6px 14px rgba(0, 0, 0, 0.18);
}

.plugin-menu {
  display: flex;
  flex-direction: column;
  gap: 6px;
  background: rgba(12, 22, 44, 0.6);
  border-radius: 12px;
  padding: 10px 10px 12px;
  border: 1px solid rgba(255, 255, 255, 0.12);
  box-shadow: inset 0 0 0 1px rgba(255, 255, 255, 0.05);
}

.plugin-item {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 12px;
  border: none;
  padding: 8px 10px;
  border-radius: 10px;
  background: transparent;
  color: rgba(255, 255, 255, 0.88);
  transition: background 0.15s ease, transform 0.15s ease;
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
}

.plugin-item .meta {
  font-size: 12px;
  color: rgba(255, 255, 255, 0.6);
}

.plugin-item .check {
  color: #8ec5ff;
}

.plugin-item:hover {
  background: rgba(255, 255, 255, 0.14);
  transform: translateX(2px);
}

.plugin-item.selected {
  background: rgba(255, 255, 255, 0.22);
  box-shadow: 0 8px 18px rgba(0, 0, 0, 0.2);
}

.plugin-empty {
  display: flex;
  align-items: center;
  gap: 8px;
  font-size: 13px;
  color: rgba(255, 255, 255, 0.68);
  padding: 10px 4px;
}

.spinner {
  animation: spin 1s linear infinite;
}

.collapse {
  margin: 12px;
  border: 1px solid rgba(255, 255, 255, 0.18);
  background: rgba(255, 255, 255, 0.08);
  color: #fff;
  border-radius: 10px;
  padding: 10px 12px;
  display: flex;
  align-items: center;
  gap: 10px;
  transition: background 0.15s ease, border-color 0.15s ease;
}

.collapse:hover {
  background: rgba(255, 255, 255, 0.16);
  border-color: rgba(255, 255, 255, 0.34);
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
