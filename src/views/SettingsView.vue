<script setup lang="ts">
import { computed, ref, onMounted } from 'vue';
import GlobalSelect from '../components/GlobalSelect.vue';
import { useSettingsStore } from '../stores/settings';
import { invoke } from '@tauri-apps/api/core';
import { error as logError } from '@tauri-apps/plugin-log';
import { open } from '@tauri-apps/plugin-dialog';
import { clearPluginCache } from '../plugins/registry';

interface Props {
  onCheckUpdateClick?: () => void;
}

defineProps<Props>();

const settings = useSettingsStore();

const cacheSizeInBytes = ref(0);
const isLoadingCacheSize = ref(false);
const isClearingCache = ref(false);

const cacheSizeDisplay = computed(() => {
  const bytes = cacheSizeInBytes.value;
  if (bytes === 0) return '0 B';
  const units = ['B', 'KB', 'MB', 'GB'];
  let size = bytes;
  let unitIndex = 0;
  while (size >= 1024 && unitIndex < units.length - 1) {
    size /= 1024;
    unitIndex++;
  }
  return `${size.toFixed(2)} ${units[unitIndex]}`;
});

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

async function loadThumbnailCacheSize() {
  try {
    isLoadingCacheSize.value = true;
    cacheSizeInBytes.value = await invoke<number>('get_thumbnail_cache_size');
  } catch (e) {
    logError(`[settings] Failed to get thumbnail cache size: ${e}`);
  } finally {
    isLoadingCacheSize.value = false;
  }
}

async function clearThumbnailCache() {
  try {
    isClearingCache.value = true;
    await invoke('clear_thumbnail_cache');
    cacheSizeInBytes.value = 0;
  } catch (e) {
    logError(`[settings] Failed to clear thumbnail cache: ${e}`);
  } finally {
    isClearingCache.value = false;
  }
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

onMounted(() => {
  void loadThumbnailCacheSize();
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

      <section class="group-title">
        <h2>图片缓存</h2>
        <p>配置缩略图缓存以加速图库加载和图片预览。</p>
      </section>

      <section class="field">
        <div class="toggle">
          <label>
            <input
              type="checkbox"
              v-model="settings.enableThumbnailCache.value"
            />
            <span class="title">启用缩略图缓存</span>
          </label>
          <p class="help">
            启用后，图片上传和图库加载时会自动生成 320×225px 的 WebP
            缓略图，显著加速界面响应速度。缓存文件存储在应用数据目录，可随时清理。
          </p>
        </div>
      </section>

      <section class="field">
        <div class="field-head">
          <label>缓存占用空间</label>
          <span class="value">{{ cacheSizeDisplay }}</span>
        </div>
        <p class="help">
          当前缩略图缓存占用的磁盘空间。点击下方按钮可清理所有缓存文件。
        </p>
        <div class="cache-actions">
          <button
            type="button"
            @click="loadThumbnailCacheSize"
            :disabled="isLoadingCacheSize"
          >
            {{ isLoadingCacheSize ? '刷新中...' : '刷新缓存大小' }}
          </button>
          <button
            type="button"
            @click="clearThumbnailCache"
            :disabled="isClearingCache || cacheSizeInBytes === 0"
            class="danger"
          >
            {{ isClearingCache ? '清理中...' : '清理缓存' }}
          </button>
        </div>
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
            @click="onCheckUpdateClick"
            :disabled="!onCheckUpdateClick"
          >
            检查更新
          </button>
        </div>
      </footer>
    </div>
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

.cache-actions {
  display: flex;
  gap: 12px;
  margin-top: 12px;
}

.cache-actions button {
  flex: 1;
  border: 1px solid var(--surface-border);
  background: var(--surface-acrylic);
  color: var(--text-primary);
  border-radius: 12px;
  padding: 10px 16px;
  font-weight: 600;
  transition: background 0.2s ease, transform 0.2s ease, border-color 0.2s ease;
  cursor: pointer;
}

.cache-actions button:hover:not(:disabled) {
  background: var(--accent-soft);
  border-color: var(--accent);
  transform: translateY(-1px);
}

.cache-actions button:active:not(:disabled) {
  transform: translateY(1px);
}

.cache-actions button:disabled {
  opacity: 0.5;
  cursor: not-allowed;
}

.cache-actions button.danger:hover:not(:disabled) {
  background: var(--danger-soft, rgba(255, 68, 68, 0.1));
  border-color: var(--danger, #ff4444);
  color: var(--danger, #ff4444);
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
