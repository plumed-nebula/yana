<script setup lang="ts">
import { ref, onMounted, onBeforeUnmount } from 'vue';
import { getCurrentWindow } from '@tauri-apps/api/window';
// import { info as logInfo } from '@tauri-apps/plugin-log';
import type { UnlistenFn } from '@tauri-apps/api/event';

const appWindow = getCurrentWindow();

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
        <span class="name">Yana Studio</span>
        <span class="subtitle">Unified Image Toolkit</span>
      </div>
    </div>
    <div class="drag-spacer" data-tauri-drag-region />
    <div class="window-actions" data-tauri-drag-region="false">
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
        :class="{ active: isMaximized }"
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
          <path
            d="M5 5h6a1 1 0 0 1 1 1v5H6a1 1 0 0 1-1-1z"
            fill="none"
            stroke-width="1.4"
            stroke-linejoin="round"
          />
          <path
            d="M4 10V6a2 2 0 0 1 2-2h4"
            fill="none"
            stroke-width="1.4"
            stroke-linejoin="round"
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
  background: rgba(248, 250, 255, 0.86);
  color: #1a2030;
  font-family: 'Segoe UI', 'Inter', 'PingFang SC', sans-serif;
  user-select: none;
  position: relative;
  border-bottom: 1px solid rgba(22, 32, 56, 0.08);
  backdrop-filter: blur(12px);
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
  color: #131928;
}

.subtitle {
  font-size: 10px;
  color: rgba(19, 25, 40, 0.6);
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
  border-radius: 4px;
  background: transparent;
  color: inherit;
  display: inline-flex;
  align-items: center;
  justify-content: center;
  transition: background 0.15s ease, color 0.15s ease;
  cursor: pointer;
  padding: 0;
}

.action svg {
  width: 12px;
  height: 12px;
  fill: none;
  stroke: currentColor;
  stroke-width: 1.4;
  stroke-linecap: round;
  stroke-linejoin: round;
  pointer-events: none;
}

.action:hover {
  background: rgba(22, 32, 56, 0.08);
}

.action:active {
  background: rgba(22, 32, 56, 0.16);
}

.action.active {
  background: rgba(22, 32, 56, 0.12);
}

.action.danger:hover {
  background: rgba(230, 70, 70, 0.14);
  color: #d92c2c;
}

.action.danger:active {
  background: rgba(230, 70, 70, 0.22);
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

  .subtitle {
    display: none;
  }

  .action {
    width: 38px;
    height: 32px;
  }
}
</style>
