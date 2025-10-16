<script setup lang="ts">
import { ref, onMounted, onBeforeUnmount } from 'vue';
import { getCurrentWindow } from '@tauri-apps/api/window';
import { invoke } from '@tauri-apps/api/core';
import type { UnlistenFn } from '@tauri-apps/api/event';
import { useThemeStore } from '../stores/theme';
import {
  Moon,
  SunMedium,
  Minus,
  Maximize2,
  Minimize2,
  X as CloseIcon,
} from 'lucide-vue-next';

const appWindow = getCurrentWindow();
const themeStore = useThemeStore();
const isDarkTheme = themeStore.isDark;

const isMaximized = ref(false);

async function refreshState() {
  isMaximized.value = await appWindow.isMaximized();
}

async function handleMinimize() {
  // await logInfo('[titlebar] minimize clicked');
  await appWindow.minimize();
}

async function handleToggleMaximize() {
  // await logInfo('[titlebar] toggle maximize clicked');
  if (isMaximized.value) {
    await appWindow.unmaximize();
  } else {
    await appWindow.maximize();
  }
  await refreshState();
}

async function handleClose() {
  // attempt to clean app temp dir before closing
  try {
    await invoke('clean_app_temp_dir');
  } catch (e) {
    // ignore cleanup errors, optionally could log
  }
  await appWindow.close();
}

function handleToggleTheme() {
  themeStore.toggleTheme();
}

let unlistenResize: UnlistenFn | null = null;

onMounted(async () => {
  await refreshState();
  unlistenResize = await appWindow.onResized(refreshState);
});

onBeforeUnmount(() => {
  if (unlistenResize) {
    unlistenResize();
    unlistenResize = null;
  }
});
</script>

<template>
  <header
    class="titlebar"
    data-tauri-drag-region
    @dblclick="handleToggleMaximize"
  >
    <div class="title-meta" data-tauri-drag-region>
      <span class="app-icon" aria-hidden="true">🖼️</span>
      <div class="titles">
        <span class="name">Yana</span>
      </div>
    </div>
    <div class="drag-spacer" data-tauri-drag-region />
    <div class="window-actions" data-tauri-drag-region="false">
      <button
        type="button"
        class="action theme-toggle"
        :aria-label="isDarkTheme ? '切换为浅色模式' : '切换为深色模式'"
        @click="handleToggleTheme"
      >
        <Moon v-if="isDarkTheme" class="icon" :size="18" :stroke-width="1.6" />
        <SunMedium v-else class="icon" :size="18" :stroke-width="1.6" />
      </button>
      <button
        type="button"
        class="action"
        aria-label="最小化"
        @click="handleMinimize"
      >
        <Minus class="icon" :size="16" :stroke-width="2.2" />
      </button>
      <button
        type="button"
        class="action"
        :aria-label="isMaximized ? '还原窗口' : '最大化'"
        @click="handleToggleMaximize"
      >
        <Maximize2
          v-if="!isMaximized"
          class="icon"
          :size="16"
          :stroke-width="2"
        />
        <Minimize2 v-else class="icon" :size="16" :stroke-width="2" />
      </button>
      <button
        type="button"
        class="action danger"
        aria-label="关闭窗口"
        @click="handleClose"
      >
        <CloseIcon class="icon" :size="16" :stroke-width="2.1" />
      </button>
    </div>
  </header>
</template>

<style scoped>
.titlebar {
  width: 100%;
  height: 40px;
  display: flex;
  align-items: center;
  justify-content: flex-start;
  padding: 0 12px;
  gap: 12px;
  background: var(--surface-acrylic-strong);
  color: var(--text-primary);
  font-family: 'Segoe UI', 'Inter', 'PingFang SC', sans-serif;
  user-select: none;
  position: relative;
  border-bottom: 1px solid var(--surface-border-strong);
  backdrop-filter: blur(24px) saturate(1.2);
  box-shadow: inset 0 1px 0 rgba(255, 255, 255, 0.08);
}

.title-meta {
  display: flex;
  align-items: center;
  gap: 8px;
  min-width: 0;
}

.drag-spacer {
  flex: 1;
  height: 100%;
}

.app-icon {
  font-size: 18px;
  line-height: 1;
}

.titles {
  display: flex;
  flex-direction: column;
  min-width: 0;
}

.name {
  font-size: 13px;
  font-weight: 600;
  color: var(--text-primary);
}

.window-actions {
  display: flex;
  align-items: center;
  gap: 2px;
  flex-shrink: 0;
}

.action {
  width: 42px;
  height: 36px;
  border: none;
  border-radius: 10px;
  background: transparent;
  color: inherit;
  display: inline-flex;
  align-items: center;
  justify-content: center;
  transition: background 0.2s ease, color 0.2s ease, transform 0.2s ease;
  cursor: pointer;
  padding: 0;
}

.action:hover {
  background: var(--action-hover);
}

.action:active {
  background: var(--action-active);
  transform: translateY(1px);
}

.action :deep(svg) {
  width: 16px;
  height: 16px;
  fill: none;
  stroke: currentColor;
  stroke-width: 1.8;
  stroke-linecap: round;
  stroke-linejoin: round;
  pointer-events: none;
}

.action.theme-toggle :deep(svg) {
  width: 18px;
  height: 18px;
}

.action.danger {
  color: var(--danger);
}

.action.danger:hover {
  background: var(--danger-soft);
}

.action.danger:active {
  background: rgba(255, 121, 121, 0.32);
}

:global([data-tauri-drag-region]) {
  -webkit-app-region: drag;
}

:global([data-tauri-drag-region='false']) {
  -webkit-app-region: no-drag;
}

@supports (padding-top: env(safe-area-inset-top)) {
  .titlebar {
    padding-top: max(0px, env(safe-area-inset-top));
  }
}

@media (max-width: 640px) {
  .titlebar {
    height: 44px;
    padding: 0 8px;
  }

  .action {
    width: 38px;
    height: 32px;
  }
}
</style>
