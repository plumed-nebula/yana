<script setup lang="ts">
import { computed, ref, watch, onMounted, onBeforeUnmount } from 'vue';
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
import { warn, info, error as logError } from '@tauri-apps/plugin-log';
import { getVersion } from '@tauri-apps/api/app';
import { fetch } from '@tauri-apps/plugin-http';
import { openUrl } from '@tauri-apps/plugin-opener';

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

// ========== Version check state and logic ==========
const updateModalOpen = ref(false);
const localVersion = ref<string | null>(null);
const latestRelease = ref<any>(null);
const updateError = ref<string | null>(null);
const checking = ref(false);
const dismissCurrentVersion = ref(false);

// LocalStorage 相关常量
const STORAGE_KEY_DISMISSED_VERSION = 'yana.settings.dismissedVersion';
const STORAGE_KEY_CACHED_RELEASE = 'yana.settings.cachedRelease';
const STORAGE_KEY_CACHE_TIMESTAMP = 'yana.settings.cacheTimestamp';
const STORAGE_KEY_RATE_LIMIT_RESET = 'yana.settings.rateLimitReset';
const CACHE_DURATION_MS = 1 * 60 * 60 * 1000; // 1 小时

/**
 * 获取被用户忽略的版本
 */
function getDismissedVersion(): string {
  try {
    return localStorage.getItem(STORAGE_KEY_DISMISSED_VERSION) || '';
  } catch {
    return '';
  }
}

/**
 * 忽略某个版本
 */
function dismissVersion(version: string): void {
  try {
    localStorage.setItem(STORAGE_KEY_DISMISSED_VERSION, version);
  } catch (e) {
    logError(`Failed to save dismissed version: ${e}`);
  }
}

/**
 * 从缓存中获取 release 信息
 */
function getCachedRelease(): any {
  try {
    const cached = localStorage.getItem(STORAGE_KEY_CACHED_RELEASE);
    const timestamp = localStorage.getItem(STORAGE_KEY_CACHE_TIMESTAMP);

    if (!cached || !timestamp) return null;

    const cacheTime = parseInt(timestamp, 10);
    const now = Date.now();

    // 检查缓存是否还未过期
    if (now - cacheTime < CACHE_DURATION_MS) {
      info(
        `[App] Using cached release info (${Math.round(
          (now - cacheTime) / 1000 / 60
        )} minutes old)`
      );
      return JSON.parse(cached);
    }

    // 缓存已过期，清除
    localStorage.removeItem(STORAGE_KEY_CACHED_RELEASE);
    localStorage.removeItem(STORAGE_KEY_CACHE_TIMESTAMP);
    return null;
  } catch (e) {
    logError(`Failed to get cached release: ${e}`);
    return null;
  }
}

/**
 * 保存 release 信息到缓存
 */
function setCachedRelease(release: any): void {
  try {
    localStorage.setItem(STORAGE_KEY_CACHED_RELEASE, JSON.stringify(release));
    localStorage.setItem(STORAGE_KEY_CACHE_TIMESTAMP, Date.now().toString());
  } catch (e) {
    logError(`Failed to save cached release: ${e}`);
  }
}

/**
 * 检查是否处于速率限制期间
 */
function isRateLimited(): boolean {
  try {
    const resetTime = localStorage.getItem(STORAGE_KEY_RATE_LIMIT_RESET);
    if (!resetTime) return false;

    const now = Date.now();
    const reset = parseInt(resetTime, 10);

    if (now < reset) {
      logError(`[App] Rate limited until ${new Date(reset).toISOString()}`);
      return true;
    }

    // 限制期已过，清除
    localStorage.removeItem(STORAGE_KEY_RATE_LIMIT_RESET);
    return false;
  } catch (e) {
    logError(`Failed to check rate limit: ${e}`);
    return false;
  }
}

/**
 * 设置速率限制恢复时间
 */
function setRateLimitReset(seconds: number): void {
  try {
    const resetTime = Date.now() + seconds * 1000;
    localStorage.setItem(STORAGE_KEY_RATE_LIMIT_RESET, resetTime.toString());
  } catch (e) {
    logError(`Failed to set rate limit reset: ${e}`);
  }
}

async function checkForUpdates(autoCheck = false, force = false) {
  updateError.value = null;
  latestRelease.value = null;
  checking.value = true;
  dismissCurrentVersion.value = false;
  try {
    localVersion.value = await getVersion();
  } catch (e) {
    updateError.value = `获取本地版本失败：${e}`;
    checking.value = false;
    if (!autoCheck) {
      updateModalOpen.value = true;
    } else {
      logError(`[App] Auto check version failed: ${e}`);
    }
    return;
  }

  try {
    const owner = 'plumed-nebula';
    const repo = 'yana';
    const cached = getCachedRelease();

    // 自动检查：优先使用缓存
    if (autoCheck && cached) {
      info(`[App] Auto check: using cached release`);
      latestRelease.value = cached;
      checking.value = false;
      processReleaseInfo(autoCheck, force, false);
      return;
    }

    // 检查是否处于速率限制期间
    if (isRateLimited()) {
      logError(`[App] Currently rate limited`);
      // 尝试使用缓存
      if (cached) {
        latestRelease.value = cached;
        updateError.value = '(使用缓存信息，API 暂时受限)';
        checking.value = false;
        processReleaseInfo(autoCheck, force, false);
      } else {
        updateError.value = 'GitHub API 请求过于频繁，请稍后再试';
        if (!autoCheck) {
          updateModalOpen.value = true;
        } else {
          logError(`[App] Rate limited, no cache available`);
        }
        checking.value = false;
      }
      return;
    }

    // 手动检查或自动检查无缓存：尝试请求最新数据
    const resp = await fetch(
      `https://api.github.com/repos/${owner}/${repo}/releases/latest`,
      {
        method: 'GET',
        headers: {
          Accept: 'application/vnd.github.v3+json',
          // 如果有缓存，使用 ETag 进行条件请求（不消耗速率限制）
          ...(cached && cached.etag ? { 'If-None-Match': cached.etag } : {}),
        },
      }
    );

    // 处理 HTTP 304（未修改，缓存有效）
    if (resp.status === 304 && cached) {
      info(`[App] Using cached release (HTTP 304 Not Modified)`);
      latestRelease.value = cached;
      checking.value = false;
      processReleaseInfo(autoCheck, force, false);
      return;
    }

    if (resp.ok) {
      const release = await resp.json();
      // 保存 ETag 和数据到缓存
      (release as any).etag = resp.headers.get('etag');
      setCachedRelease(release);
      latestRelease.value = release;
      checking.value = false;
      processReleaseInfo(autoCheck, force, false);
    } else if (resp.status === 403) {
      // 速率限制
      const resetTime = resp.headers.get('X-RateLimit-Reset');
      if (resetTime) {
        const reset = parseInt(resetTime, 10) * 1000;
        const now = Date.now();
        const seconds = Math.ceil((reset - now) / 1000);
        setRateLimitReset(Math.max(seconds, 60));
      } else {
        // 没有 Reset 头，默认 1 小时
        setRateLimitReset(3600);
      }

      // 请求失败，回退到缓存
      if (cached) {
        logError(`[App] Rate limited (403), falling back to cache`);
        latestRelease.value = cached;
        updateError.value = '(使用缓存信息，API 暂时受限)';
      } else {
        updateError.value = 'GitHub API 请求过于频繁，请稍后再试';
        if (!autoCheck) {
          updateModalOpen.value = true;
        } else {
          logError(`[App] Rate limited (403), no cache available`);
        }
      }
      checking.value = false;
    } else {
      // 其他 HTTP 错误
      updateError.value = `请求 GitHub Release 失败：${resp.status}`;

      // 手动检查失败时回退到缓存
      if (!autoCheck && cached) {
        logError(`[App] HTTP ${resp.status}, falling back to cache`);
        latestRelease.value = cached;
        updateError.value = `(请求失败 HTTP ${resp.status}，使用缓存信息)`;
      } else if (!autoCheck) {
        updateModalOpen.value = true;
      } else {
        logError(`[App] Failed to fetch latest release: HTTP ${resp.status}`);
      }
      checking.value = false;
    }
  } catch (e) {
    updateError.value = `请求 GitHub Release 失败：${e}`;

    // 请求异常，回退到缓存
    const cached = getCachedRelease();
    if (!autoCheck && cached) {
      logError(`[App] Request error, falling back to cache`);
      latestRelease.value = cached;
      updateError.value = `(请求出错，使用缓存信息)`;
    } else if (!autoCheck) {
      updateModalOpen.value = true;
    } else {
      logError(`[App] Failed to fetch latest release: ${e}`);
    }
    checking.value = false;
  }
}

/**
 * 处理获取到的 release 信息
 */
function processReleaseInfo(
  autoCheck = false,
  force = false,
  fromError = false
) {
  if (!latestRelease.value) return;

  const normalizedLocal = normalizeVersion(localVersion.value);
  const normalizedLatest = normalizeVersion(latestRelease.value.tag_name);
  const dismissedVersion = getDismissedVersion();

  // 根据是否是强制检查或手动检查决定是否显示弹窗
  const shouldShowModal =
    // force 为 true 时无论如何都显示弹窗
    force ||
    // 版本不一致且该版本未被用户忽略，则显示弹窗
    (normalizedLocal !== normalizedLatest &&
      normalizedLatest !== dismissedVersion) ||
    // 手动检查时版本相同也要显示结果（只在非错误情况下且为手动检查）
    (!fromError && !autoCheck && normalizedLocal === normalizedLatest);

  if (shouldShowModal) {
    updateModalOpen.value = true;
    // 检查该版本是否已被忽略，若已忽略则 checkbox 为选中状态
    if (normalizedLatest === dismissedVersion) {
      dismissCurrentVersion.value = true;
    }
  }
}

/**
 * 手动触发版本检查（总是显示弹窗）
 */
function manualCheckForUpdates() {
  checkForUpdates(false, true);
}

/**
 * 关闭弹窗
 */
function closeUpdateModal() {
  if (dismissCurrentVersion.value && latestRelease.value) {
    // 用户勾选了"不再提醒此版本"，保存到 localStorage
    const normalizedVersion = normalizeVersion(latestRelease.value.tag_name);
    dismissVersion(normalizedVersion);
  } else if (!dismissCurrentVersion.value && latestRelease.value) {
    // 用户取消了勾选，清除之前的忽略记录
    try {
      localStorage.removeItem(STORAGE_KEY_DISMISSED_VERSION);
    } catch (e) {
      logError(`Failed to clear dismissed version: ${e}`);
    }
  }
  updateModalOpen.value = false;
  dismissCurrentVersion.value = false;
}

function openReleasePage() {
  if (!latestRelease.value) return;
  const url =
    latestRelease.value.html_url ||
    `https://github.com/${'plumed-nebula'}/${'yana'}/releases`;
  openUrl(url);
}

/**
 * 规范化版本号，去掉前缀的 'v'
 */
function normalizeVersion(version: string | null | undefined): string {
  if (!version) return '未知';
  return version.startsWith('v') ? version.substring(1) : version;
}

// ========== End of version check state and logic ==========

onMounted(() => {
  void determinePlatform();
  void imageHostStore.ensureLoaded();
  // 自动检查版本
  void checkForUpdates(true);
});

// 当弹窗打开时禁用滚动条，关闭时恢复
watch(
  () => updateModalOpen.value,
  (isOpen) => {
    document.body.style.overflow = isOpen ? 'hidden' : '';
  }
);

// 卸载时清理滚动条状态
onBeforeUnmount(() => {
  document.body.style.overflow = '';
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
  if (current.value === 'settings') {
    return {
      onCheckUpdateClick: manualCheckForUpdates,
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

    <!-- Update modal (teleported-style overlay) -->
    <teleport to="body">
      <div
        v-if="updateModalOpen"
        class="confirm-overlay"
        role="dialog"
        aria-modal="true"
      >
        <div class="confirm-dialog">
          <h3>检查更新</h3>
          <div class="message">
            <p v-if="checking">正在检查更新…</p>
            <p v-else>
              本地版本： <strong>{{ normalizeVersion(localVersion) }}</strong>
            </p>
            <p v-if="latestRelease">
              最新 Release：
              <strong>{{ normalizeVersion(latestRelease.tag_name) }}</strong>
              <br />发布日期：<span>{{ latestRelease.published_at }}</span>
            </p>
            <p v-if="!latestRelease && !checking && !updateError">
              未能获取到最新版信息。
            </p>
            <p v-if="updateError" class="sub">错误：{{ updateError }}</p>
            <p
              class="sub"
              style="margin-top: 12px; color: var(--text-secondary)"
            >
              若需查看完整 Release 页面，请点击下方"打开 Release 页面"。
            </p>
            <label
              style="
                margin-top: 16px;
                display: flex;
                align-items: center;
                gap: 8px;
                cursor: pointer;
              "
            >
              <input
                type="checkbox"
                v-model="dismissCurrentVersion"
                style="cursor: pointer"
              />
              <span style="font-size: 14px; color: var(--text-secondary)"
                >不再提醒此版本</span
              >
            </label>
          </div>
          <div class="confirm-actions">
            <button class="ghost" type="button" @click="closeUpdateModal">
              关闭
            </button>
            <button
              type="button"
              @click="openReleasePage"
              :disabled="!latestRelease"
            >
              打开 Release 页面
            </button>
          </div>
        </div>
      </div>
    </teleport>
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

.confirm-overlay {
  position: fixed;
  inset: 0;
  background: transparent;
  display: flex;
  justify-content: center;
  align-items: center;
  padding: 32px;
  z-index: 1300;
}

.confirm-overlay::before {
  content: '';
  position: fixed;
  inset: 0;
  background: var(--modal-backdrop);
  z-index: -1;
}

.confirm-dialog {
  width: min(520px, 90vw);
  background: var(--surface-panel);
  border-radius: 20px;
  padding: 28px 24px;
  border: 1px solid var(--surface-border);
  box-shadow: 0 24px 60px rgba(5, 8, 18, 0.42);
  display: flex;
  flex-direction: column;
  gap: 16px;
  text-align: left;
}

.confirm-dialog h3 {
  margin: 0;
  font-size: 20px;
  font-weight: 700;
  color: var(--text-primary);
}

.confirm-dialog .message {
  margin: 0;
  font-size: 14px;
  color: var(--text-secondary);
  line-height: 1.6;
}

.confirm-dialog .message strong {
  font-weight: 700;
  color: var(--accent);
  word-break: break-all;
}

.confirm-dialog .sub {
  margin: -8px 0 0;
  font-size: 12px;
  color: var(--text-secondary);
  line-height: 1.6;
  opacity: 0.8;
}

.confirm-actions {
  display: flex;
  justify-content: flex-end;
  gap: 12px;
  margin-top: 8px;
}

.confirm-actions .ghost,
.confirm-actions button {
  padding: 10px 18px;
  border-radius: 12px;
  font-weight: 600;
  font-size: 14px;
  cursor: pointer;
  transition: background 0.2s ease, transform 0.2s ease, opacity 0.2s ease,
    border-color 0.2s ease;
  border: 1px solid var(--surface-border);
  background: var(--surface-acrylic);
  color: var(--text-primary);
}

.confirm-actions .ghost {
  color: var(--text-secondary);
}

.confirm-actions button:hover:not(:disabled) {
  background: var(--accent-soft);
  border-color: var(--accent);
  transform: translateY(-1px);
}

.confirm-actions .ghost:hover:not(:disabled) {
  color: var(--text-primary);
  border-color: var(--accent);
  border-color: color-mix(in srgb, var(--accent) 55%, transparent);
  transform: translateY(-1px);
}

.confirm-actions button:disabled {
  opacity: 0.65;
  cursor: not-allowed;
  transform: none;
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
  /* scrollbar colors for light theme */
  --scrollbar-bg: rgba(0, 0, 0, 0.06);
  --scrollbar-thumb: rgba(0, 0, 0, 0.18);
  --scrollbar-thumb-hover: rgba(0, 0, 0, 0.28);
  /* modal/backdrop for light theme */
  --modal-backdrop: rgba(6, 10, 22, 0.6);
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
  /* scrollbar colors for dark theme */
  --scrollbar-bg: rgba(255, 255, 255, 0.03);
  --scrollbar-thumb: rgba(255, 255, 255, 0.12);
  --scrollbar-thumb-hover: rgba(255, 255, 255, 0.2);
  /* modal/backdrop for dark theme */
  --modal-backdrop: rgba(6, 10, 22, 0.78);
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

/* Global thin scrollbar styles (WebKit/Chromium + Firefox) */
/* Uses variables defined per theme above */
html,
body,
#app {
  scrollbar-width: thin; /* Firefox */
  scrollbar-color: var(--scrollbar-thumb) var(--scrollbar-bg);
}

/* WebKit-based browsers */
::-webkit-scrollbar {
  width: 6px;
  height: 6px;
}

::-webkit-scrollbar-track {
  background: var(--scrollbar-bg);
  border-radius: 999px;
}

::-webkit-scrollbar-thumb {
  background-color: var(--scrollbar-thumb);
  border-radius: 999px;
  border: 2px solid transparent;
  background-clip: content-box;
  transition: background-color 120ms ease;
}

::-webkit-scrollbar-thumb:hover {
  background-color: var(--scrollbar-thumb-hover);
}

button {
  cursor: pointer;
  font-family: inherit;
}
</style>
