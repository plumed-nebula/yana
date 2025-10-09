<script setup lang="ts">
import { ref, onMounted, onBeforeUnmount } from 'vue';
import { getCurrentWindow } from '@tauri-apps/api/window';
import type { UnlistenFn } from '@tauri-apps/api/event';
import { useThemeStore } from '../stores/theme';

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
  // await logInfo('[titlebar] close clicked');
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
        <svg
          v-if="isDarkTheme"
          viewBox="0 0 24 24"
          focusable="false"
          aria-hidden="true"
        >
          <path
            d="M12 4.5a7.5 7.5 0 1 0 7.5 7.5 5.5 5.5 0 0 1-7.5-7.5Z"
            fill="none"
            stroke="currentColor"
            stroke-width="1.6"
            stroke-linecap="round"
            stroke-linejoin="round"
          />
        </svg>
        <svg v-else viewBox="0 0 24 24" focusable="false" aria-hidden="true">
          <circle
            cx="12"
            cy="12"
            r="5.5"
            fill="none"
            stroke="currentColor"
            stroke-width="1.6"
          />
          <path
            d="M12 3v2.5m0 13V21m9-9h-2.5M5.5 12H3m16.45 6.45-1.77-1.77M7.32 7.32 5.55 5.55m0 12.9 1.77-1.77m10.36-10.36 1.77-1.77"
            fill="none"
            stroke="currentColor"
            stroke-width="1.6"
            stroke-linecap="round"
          />
        </svg>
      </button>
      <button
        type="button"
        class="action"
        aria-label="最小化"
        @click="handleMinimize"
      >
        <svg viewBox="0 0 16 16" focusable="false" aria-hidden="true">
          <rect x="3" y="7.5" width="10" height="1" rx="0.5" />
        </svg>
      </button>
      <button
        type="button"
        class="action"
        :aria-label="isMaximized ? '还原窗口' : '最大化'"
        @click="handleToggleMaximize"
      >
        <svg
          v-if="!isMaximized"
          viewBox="0 0 16 16"
          focusable="false"
          aria-hidden="true"
        >
          <rect x="3" y="3" width="10" height="10" rx="1.6" ry="1.6" />
        </svg>
        <svg v-else viewBox="0 0 16 16" focusable="false" aria-hidden="true">
          <rect
            x="2"
            y="2"
            width="12"
            height="12"
            rx="1"
            fill="none"
            stroke="currentColor"
            stroke-width="1.4"
          />
          <rect
            x="4"
            y="4"
            width="8"
            height="8"
            rx="0.5"
            fill="none"
            stroke="currentColor"
            stroke-width="1.4"
          />
        </svg>
      </button>
      <button
        type="button"
        class="action danger"
        aria-label="关闭窗口"
        @click="handleClose"
      >
        <svg viewBox="0 0 16 16" focusable="false" aria-hidden="true">
          <path d="M4 4l8 8m0-8L4 12" />
        </svg>
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

.action svg {
  width: 14px;
  height: 14px;
  fill: none;
  stroke: currentColor;
  stroke-width: 1.4;
  stroke-linecap: round;
  stroke-linejoin: round;
  pointer-events: none;
}

.action:hover {
  background: var(--action-hover);
}

.action:active {
  background: var(--action-active);
  transform: translateY(1px);
}

.action.theme-toggle svg {
  width: 16px;
  height: 16px;
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
