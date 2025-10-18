<script setup lang="ts">
import { computed, ref, watch, onBeforeUnmount, onMounted } from 'vue';
import GlobalSelect from '../components/GlobalSelect.vue';
import { useSettingsStore } from '../stores/settings';
import { invoke } from '@tauri-apps/api/core';
import { getVersion } from '@tauri-apps/api/app';
import { openUrl } from '@tauri-apps/plugin-opener';
import { fetch } from '@tauri-apps/plugin-http';
import { info, error as logError } from '@tauri-apps/plugin-log';
import { open } from '@tauri-apps/plugin-dialog';
import { clearPluginCache } from '../plugins/registry';

const settings = useSettingsStore();

const pngModeDescription = computed(() => {
  if (settings.convertToWebp.value) {
    return '当目标图床不支持 WebP 时，会按所选策略重新压缩 PNG，以便自动回退。';
  }
  return settings.pngCompressionMode.value === 'lossy'
    ? '进行颜色量化以尽量保持观感的前提下显著减小 PNG 体积。适合截图、插画等对绝对无损要求不高的场景。'
    : '保持像素原样，仅对 DEFLATE 压缩器进行调优，画质完全无损。适合对素材要求无损还原的场景。';
});

const pngOptimizationDescription = computed(() => {
  if (settings.convertToWebp.value) {
    return '回退到 PNG 时使用的优化级别，上传前压缩会按该选项调整编码速度与压缩率。';
  }
  switch (settings.pngOptimization.value) {
    case 'best':
      return '优先压缩率，编码时间较长。适合离线批量压缩。';
    case 'fast':
      return '偏重速度，压缩率略有牺牲。适合快速试跑。';
    default:
      return '兼顾速度与压缩率，适用于大多数情况。';
  }
});
const pngOptions = [
  { value: 'best', label: '最佳压缩（最慢）' },
  { value: 'default', label: '标准（推荐）' },
  { value: 'fast', label: '快速（体积略大）' },
];

const persistenceMessage = computed(() => {
  if (!settings.ready.value) return '正在读取本地配置…';
  if (settings.loading.value) return '同步中…';
  if (settings.error.value)
    return `保存或读取时出现问题：${settings.error.value}`;
  return '设置会自动保存到本地配置目录。';
});

async function openLogDir() {
  try {
    await invoke('open_log_dir');
    info('Triggered backend to open log directory');
  } catch (e) {
    logError(`Failed to open log directory: ${e}`);
  }
}

function restoreDefaults() {
  settings.quality.value = 80;
  settings.convertToWebp.value = false;
  settings.pngCompressionMode.value = 'lossless';
  settings.pngOptimization.value = 'default';
  settings.enableUploadCompression.value = false;
  settings.maxConcurrentUploads.value = 5;
}

/**
 * 弹窗选择本地 JS 文件并添加为图床插件
 */
async function loadPlugin() {
  try {
    const selected = await open({
      multiple: false,
      filters: [{ name: 'JS Files', extensions: ['js', 'mjs'] }],
    });
    if (typeof selected !== 'string') return;
    await invoke('add_image_host_plugin', { source: selected });
    clearPluginCache();
    window.location.reload();
  } catch (e) {
    logError(`[settings] load plugin failed: ${e}`);
  }
}

async function reloadPlugins() {
  // 清除插件加载缓存，并刷新界面以重新加载脚本
  clearPluginCache();
  window.location.reload();
}

// Update check state
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
        `[settings] Using cached release info (${Math.round(
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
      logError(
        `[settings] Rate limited until ${new Date(reset).toISOString()}`
      );
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
      logError(`[settings] Auto check version failed: ${e}`);
    }
    return;
  }

  try {
    const owner = 'plumed-nebula';
    const repo = 'yana';
    const cached = getCachedRelease();

    // 自动检查：优先使用缓存
    if (autoCheck && cached) {
      info(`[settings] Auto check: using cached release`);
      latestRelease.value = cached;
      checking.value = false;
      processReleaseInfo(autoCheck, force, false);
      return;
    }

    // 检查是否处于速率限制期间
    if (isRateLimited()) {
      logError(`[settings] Currently rate limited`);
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
          logError(`[settings] Rate limited, no cache available`);
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
      info(`[settings] Using cached release (HTTP 304 Not Modified)`);
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
        logError(`[settings] Rate limited (403), falling back to cache`);
        latestRelease.value = cached;
        updateError.value = '(使用缓存信息，API 暂时受限)';
      } else {
        updateError.value = 'GitHub API 请求过于频繁，请稍后再试';
        if (!autoCheck) {
          updateModalOpen.value = true;
        } else {
          logError(`[settings] Rate limited (403), no cache available`);
        }
      }
      checking.value = false;
    } else {
      // 其他 HTTP 错误
      updateError.value = `请求 GitHub Release 失败：${resp.status}`;

      // 手动检查失败时回退到缓存
      if (!autoCheck && cached) {
        logError(`[settings] HTTP ${resp.status}, falling back to cache`);
        latestRelease.value = cached;
        updateError.value = `(请求失败 HTTP ${resp.status}，使用缓存信息)`;
      } else if (!autoCheck) {
        updateModalOpen.value = true;
      } else {
        logError(
          `[settings] Failed to fetch latest release: HTTP ${resp.status}`
        );
      }
      checking.value = false;
    }
  } catch (e) {
    updateError.value = `请求 GitHub Release 失败：${e}`;

    // 请求异常，回退到缓存
    const cached = getCachedRelease();
    if (!autoCheck && cached) {
      logError(`[settings] Request error, falling back to cache`);
      latestRelease.value = cached;
      updateError.value = `(请求出错，使用缓存信息)`;
    } else if (!autoCheck) {
      updateModalOpen.value = true;
    } else {
      logError(`[settings] Failed to fetch latest release: ${e}`);
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
  // 在 Tauri 中直接打开外部链接也可以使用 invoke 到后端，简单方案使用 window.open
  openUrl(url);
}

/**
 * 规范化版本号，去掉前缀的 'v'
 */
function normalizeVersion(version: string | null | undefined): string {
  if (!version) return '未知';
  return version.startsWith('v') ? version.substring(1) : version;
}

// 组件挂载时自动检查版本
onMounted(() => {
  checkForUpdates(true);
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
</script>

<template>
  <div class="wrapper">
    <div class="panel">
      <section class="group-title">
        <h2>上传选项</h2>
        <p>配置上传时的预处理流程与并发策略，确保与目标图床匹配。</p>
      </section>

      <section class="field">
        <div class="toggle">
          <label>
            <input
              type="checkbox"
              v-model="settings.enableUploadCompression.value"
            />
            <span class="title">上传前先执行压缩流程</span>
          </label>
          <p class="help">
            根据当前压缩参数预处理图片，再交由图床上传；若关闭则直接上传原始文件。
          </p>
        </div>
      </section>

      <section class="field">
        <div class="field-head">
          <label for="upload-concurrency">最大并发上传数</label>
          <span class="value">{{ settings.maxConcurrentUploads }}</span>
        </div>
        <div class="field-body">
          <input
            id="upload-concurrency"
            type="number"
            min="1"
            max="10"
            v-model.number="settings.maxConcurrentUploads.value"
          />
        </div>
        <p class="help">
          控制同时进行的上传任务数量。数值越大速度越快，但可能占用更多带宽和图床限速。
          此设置也会用于限制图库中批量删除操作的并发请求数，以避免瞬时请求过多导致目标图床或后端压力过大。
        </p>
      </section>

      <section class="group-title">
        <h2>压缩参数</h2>
        <p>调整图片压缩的基础策略，所有更改会自动持久化。</p>
      </section>

      <section class="field">
        <div class="field-head">
          <label for="quality">压缩比率（0-100）</label>
          <span class="value">{{ settings.quality }}</span>
        </div>
        <div class="field-body">
          <input
            id="quality"
            type="range"
            min="0"
            max="100"
            v-model.number="settings.quality.value"
          />
          <input
            type="number"
            min="0"
            max="100"
            v-model.number="settings.quality.value"
          />
        </div>
        <p class="help">
          数值越低压缩越狠（PNG 表示压缩强度，JPEG/WebP 表示画质等级）。
        </p>
      </section>

      <section class="field">
        <div class="toggle">
          <label>
            <input type="checkbox" v-model="settings.convertToWebp.value" />
            <span class="title">将图片统一转为 WebP</span>
          </label>
          <p class="help">开启后支持的格式会输出为 WebP，可显著降低体积。</p>
        </div>
      </section>

      <section class="field">
        <div class="field-head">
          <label>PNG 压缩策略</label>
          <span class="value">
            {{
              settings.pngCompressionMode.value === 'lossy'
                ? '有损压缩'
                : '无损优化'
            }}
          </span>
        </div>
        <div class="option-group">
          <label>
            <input
              type="radio"
              value="lossless"
              v-model="settings.pngCompressionMode.value"
            />
            <span class="title">无损优化</span>
            <span class="desc">保持像素数据不变，仅优化压缩级别。</span>
          </label>
          <label>
            <input
              type="radio"
              value="lossy"
              v-model="settings.pngCompressionMode.value"
            />
            <span class="title">有损压缩</span>
            <span class="desc">通过颜色量化进一步减小体积。</span>
          </label>
        </div>
        <p class="help">{{ pngModeDescription }}</p>
      </section>

      <section class="field">
        <div class="field-head">
          <label for="png-opt">PNG 优化级别</label>
          <span class="value">
            {{
              settings.pngOptimization.value === 'best'
                ? '最佳压缩'
                : settings.pngOptimization.value === 'fast'
                ? '快速'
                : '标准'
            }}
          </span>
        </div>
        <div class="field-body">
          <GlobalSelect
            v-model="settings.pngOptimization.value"
            :options="pngOptions"
          />
        </div>
        <p class="help">{{ pngOptimizationDescription }}</p>
      </section>

      <footer>
        <div class="status" :class="{ error: !!settings.error.value }">
          {{ persistenceMessage }}
        </div>
        <div class="buttons">
          <button type="button" @click="openLogDir">打开日志目录</button>
          <button type="button" @click="loadPlugin">加载插件</button>
          <button type="button" @click="restoreDefaults">恢复默认</button>
          <button type="button" @click="reloadPlugins">重载插件</button>
          <button
            type="button"
            @click="manualCheckForUpdates"
            :disabled="checking"
          >
            检查更新
          </button>
        </div>
      </footer>
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
.wrapper {
  width: 100%;
  display: flex;
  flex-direction: column;
  gap: 24px;
}

.panel {
  background: var(--surface-panel);
  border-radius: 26px;
  box-shadow: var(--shadow-strong);
  padding: 36px;
  backdrop-filter: blur(26px) saturate(1.18);
  border: 1px solid var(--surface-border);
  color: var(--text-primary);
}

.group-title {
  margin-top: 36px;
  display: flex;
  flex-direction: column;
  gap: 6px;
  color: var(--text-primary);
}

.group-title:first-of-type {
  margin-top: 0;
}

.group-title h2 {
  margin: 0;
  font-size: 24px;
  font-weight: 700;
}

.group-title p {
  margin: 0;
  color: var(--text-secondary);
}

.field {
  margin-top: 28px;
  display: flex;
  flex-direction: column;
  gap: 10px;
}

.field-head {
  display: flex;
  justify-content: space-between;
  align-items: baseline;
  gap: 12px;
}

.field-head label {
  font-weight: 600;
  color: var(--text-primary);
}

.field-head .value {
  font-family: 'Fira Code', 'Consolas', monospace;
  font-size: 15px;
  color: var(--text-secondary);
}

.field-body {
  display: flex;
  gap: 12px;
  align-items: center;
}

.field-body input[type='range'] {
  flex: 1;
}

.field-body input[type='number'] {
  width: 96px;
  padding: 8px 10px;
  border-radius: 12px;
  border: 1px solid var(--surface-border);
  background: var(--surface-acrylic);
  color: var(--text-primary);
}

.field-body select {
  flex: 1;
  min-width: 220px;
  padding: 10px 12px;
  border-radius: 12px;
  border: 1px solid var(--surface-border);
  background: var(--surface-acrylic);
  color: var(--text-primary);
}

.toggle {
  background: var(--surface-acrylic);
  border-radius: 18px;
  padding: 20px 22px;
  display: flex;
  flex-direction: column;
  gap: 8px;
  transition: opacity 0.2s ease;
  border: 1px solid var(--surface-border);
}

.toggle label {
  display: flex;
  align-items: center;
  gap: 12px;
  font-weight: 600;
  color: var(--text-primary);
}

.toggle .title {
  font-size: 16px;
}

.toggle.disabled {
  opacity: 0.5;
}

.help {
  margin: 0;
  color: var(--text-secondary);
  font-size: 14px;
}

.option-group {
  display: grid;
  gap: 12px;
  background: var(--surface-acrylic);
  border-radius: 18px;
  padding: 16px 18px;
  border: 1px solid var(--surface-border);
}

.option-group.disabled {
  opacity: 0.5;
}

.option-group label {
  display: grid;
  grid-template-columns: auto 1fr;
  gap: 10px 14px;
  align-items: center;
  font-weight: 600;
  color: var(--text-primary);
}

.option-group .title {
  font-size: 16px;
}

.option-group .desc {
  grid-column: 2 / 3;
  font-size: 13px;
  color: var(--text-secondary);
}

footer {
  margin-top: 40px;
  display: flex;
  justify-content: space-between;
  align-items: center;
  gap: 16px;
}

.status {
  font-size: 14px;
  color: var(--text-secondary);
}

.status.error {
  color: var(--danger);
}

.buttons {
  display: flex;
  gap: 12px;
}

.buttons button {
  border: 1px solid var(--surface-border);
  background: var(--surface-acrylic);
  color: var(--text-primary);
  border-radius: 12px;
  padding: 10px 18px;
  font-weight: 600;
  transition: background 0.2s ease, transform 0.2s ease, border-color 0.2s ease;
}

.buttons button:hover {
  background: var(--accent-soft);
  border-color: var(--accent);
  transform: translateY(-1px);
}

.buttons button:active {
  transform: translateY(1px);
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

@media (max-width: 640px) {
  .panel {
    padding: 26px;
  }
  footer {
    flex-direction: column;
    align-items: stretch;
  }
  footer .buttons {
    display: contents;
  }
  footer button {
    width: 100%;
  }
}
</style>
