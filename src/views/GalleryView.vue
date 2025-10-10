<script setup lang="ts">
import { onMounted, ref, computed, onBeforeUnmount, watch } from 'vue';
import GalleryItemCard from '../components/GalleryItemCard.vue';
import type { GalleryItem, GalleryQuery } from '../types/gallery';
import {
  listGalleryHosts,
  queryGalleryItems,
  deleteGalleryItem,
} from '../types/gallery';
import {
  error as logError,
  info as logInfo,
  warn as logWarn,
} from '@tauri-apps/plugin-log';
import { useImageHostStore } from '../stores/imageHosts';

const keyword = ref('');
const selectedHost = ref('');
const startDate = ref('');
const endDate = ref('');
const minSize = ref('');
const maxSize = ref('');

const showAdvanced = ref(false);
const previewItem = ref<GalleryItem | null>(null);

const hosts = ref<string[]>([]);
const items = ref<GalleryItem[]>([]);
const loading = ref(false);
const hostLoading = ref(false);
const errorMessage = ref('');
const toast = ref<{ message: string; kind: 'success' | 'error' } | null>(null);
const confirmTarget = ref<GalleryItem | null>(null);
const confirmError = ref('');
const deleteLoading = ref(false);
const copyFormat = ref<'link' | 'html' | 'bbcode' | 'markdown'>('link');

const copyFormatOptions: Array<{
  value: typeof copyFormat.value;
  label: string;
}> = [
  { value: 'link', label: '纯链接' },
  { value: 'html', label: 'HTML' },
  { value: 'markdown', label: 'Markdown' },
  { value: 'bbcode', label: 'BBCode' },
];

function extractFileName(item: GalleryItem): string {
  if (item.file_name) return item.file_name;
  try {
    const url = new URL(item.url);
    const parts = url.pathname.split('/').filter(Boolean);
    if (parts.length) return parts[parts.length - 1] ?? item.url;
  } catch (error) {
    // ignore malformed url
  }
  const segments = item.url.split('/').filter(Boolean);
  return segments.length ? segments[segments.length - 1] ?? item.url : item.url;
}

function buildCopyText(item: GalleryItem): string {
  const url = item.url;
  const name = extractFileName(item) || 'image';
  switch (copyFormat.value) {
    case 'html':
      return `<img src="${url}" alt="${name}" />`;
    case 'markdown':
      return `![${name}](${url})`;
    case 'bbcode':
      return `[img]${url}[/img]`;
    default:
      return url;
  }
}

const imageHostStore = useImageHostStore();
void imageHostStore.ensureLoaded();

let toastTimer: ReturnType<typeof setTimeout> | null = null;

const advancedActive = computed(() =>
  Boolean(startDate.value || endDate.value || minSize.value || maxSize.value)
);

function toIso(value: string): string | undefined {
  if (!value) return undefined;
  const date = new Date(value);
  if (Number.isNaN(date.getTime())) return undefined;
  return date.toISOString();
}

function toNumber(value: string): number | undefined {
  if (!value) return undefined;
  const parsed = Number(value);
  if (!Number.isFinite(parsed)) return undefined;
  return parsed;
}

async function loadHosts() {
  hostLoading.value = true;
  try {
    const list = await listGalleryHosts();
    hosts.value = list;
    logInfo(`Loaded ${list.length} hosts from gallery database`);
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err);
    errorMessage.value = `加载图床列表失败：${message}`;
    logError(`Failed to load gallery hosts: ${message}`);
  } finally {
    hostLoading.value = false;
  }
}

async function fetchItems() {
  loading.value = true;
  errorMessage.value = '';
  try {
    const query: GalleryQuery = {};
    if (keyword.value.trim()) {
      query.file_name = keyword.value.trim();
    }
    if (selectedHost.value) {
      query.host = selectedHost.value;
    }
    const startIso = toIso(startDate.value);
    if (startIso) {
      query.start_utc = startIso;
    }
    const endIso = toIso(endDate.value);
    if (endIso) {
      query.end_utc = endIso;
    }
    const min = toNumber(minSize.value);
    if (typeof min === 'number') {
      query.min_filesize = min;
    }
    const max = toNumber(maxSize.value);
    if (typeof max === 'number') {
      query.max_filesize = max;
    }

    const result = await queryGalleryItems(query);
    items.value = result;
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err);
    errorMessage.value = `加载图片失败：${message}`;
    logError(`Failed to fetch gallery items: ${message}`);
  } finally {
    loading.value = false;
  }
}

function resetFilters() {
  keyword.value = '';
  selectedHost.value = '';
  startDate.value = '';
  endDate.value = '';
  minSize.value = '';
  maxSize.value = '';
  showAdvanced.value = false;
  void fetchItems();
}

function handleSubmit() {
  void fetchItems();
}

function toggleAdvanced() {
  showAdvanced.value = !showAdvanced.value;
}

function openPreview(item: GalleryItem) {
  previewItem.value = item;
}

function closePreview() {
  previewItem.value = null;
}

function handleKeydown(event: KeyboardEvent) {
  if (event.key === 'Escape') {
    if (confirmTarget.value) {
      closeConfirm();
      return;
    }
    if (previewItem.value) {
      closePreview();
    }
  }
}

function showToast(message: string, kind: 'success' | 'error' = 'success') {
  toast.value = { message, kind };
  if (toastTimer) {
    clearTimeout(toastTimer);
  }
  toastTimer = setTimeout(() => {
    toast.value = null;
    toastTimer = null;
  }, 3200);
}

function closeConfirm() {
  confirmTarget.value = null;
  confirmError.value = '';
  deleteLoading.value = false;
}

async function handleCopy(item: GalleryItem) {
  try {
    if (!navigator.clipboard || !navigator.clipboard.writeText) {
      throw new Error('当前环境不支持剪贴板写入');
    }
    const payload = buildCopyText(item);
    await navigator.clipboard.writeText(payload);
    const label = copyFormatOptions.find(
      (option) => option.value === copyFormat.value
    )?.label;
    showToast(`${label ?? '已'}复制到剪贴板。`, 'success');
    void logInfo(
      `[gallery] 复制成功 (id=${item.id}, format=${copyFormat.value})`
    );
  } catch (error) {
    const message =
      error instanceof Error ? error.message : String(error ?? '未知错误');
    showToast(`复制失败：${message}`, 'error');
    void logError(`[gallery] 复制链接失败 (${item.id}): ${message}`);
  }
}

function requestDelete(item: GalleryItem) {
  confirmError.value = '';
  deleteLoading.value = false;
  confirmTarget.value = item;
  if (previewItem.value) {
    closePreview();
  }
}

async function confirmDeletion() {
  if (!confirmTarget.value) return;
  deleteLoading.value = true;
  confirmError.value = '';

  const target = confirmTarget.value;

  const plugin = imageHostStore.getPluginById(target.host);

  if (!plugin) {
    showToast('图床插件不存在，已仅删除本地记录。', 'error');
    void logError(
      `[gallery] 插件 ${target.host} 不存在，跳过远程删除 (id=${target.id})`
    );
  } else if (!target.delete_marker) {
    showToast('该记录缺少删除标识，已跳过图床删除。', 'error');
    void logWarn(
      `[gallery] 记录 ${target.id} 缺少 delete_marker，无法执行远程删除`
    );
  } else {
    try {
      const result = await plugin.remove(
        target.delete_marker,
        imageHostStore.runtime
      );
      if (!result?.success) {
        const message = result?.message ?? '未知错误';
        showToast(`图床删除失败：${message}`, 'error');
        void logWarn(
          `[gallery] 调用插件 ${target.host} 删除失败 (id=${target.id}): ${message}`
        );
      } else {
        const successMessage = result.message ?? '图床已删除该图片。';
        showToast(successMessage, 'success');
        void logInfo(
          `[gallery] 调用插件 ${target.host} 删除成功 (id=${target.id})`
        );
      }
    } catch (error) {
      const message =
        error instanceof Error ? error.message : String(error ?? '未知错误');
      showToast(`图床删除异常：${message}`, 'error');
      void logError(
        `[gallery] 调用插件 ${target.host} 删除异常 (id=${target.id}): ${message}`
      );
    }
  }

  try {
    await deleteGalleryItem(target.id);
    items.value = items.value.filter((entry) => entry.id !== target.id);
    if (!toast.value) {
      showToast('已从图库移除记录。', 'success');
    }
    void logInfo(`[gallery] 图库记录删除成功 (id=${target.id})`);
    closeConfirm();
  } catch (error) {
    const message =
      error instanceof Error ? error.message : String(error ?? '未知错误');
    confirmError.value = `删除数据库记录失败：${message}`;
    void logError(`[gallery] 删除数据库记录失败 (id=${target.id}): ${message}`);
  } finally {
    deleteLoading.value = false;
  }
}

onMounted(async () => {
  window.addEventListener('keydown', handleKeydown);
  await loadHosts();
  await fetchItems();
  if (advancedActive.value) {
    showAdvanced.value = true;
  }
});

onBeforeUnmount(() => {
  window.removeEventListener('keydown', handleKeydown);
  document.body.style.overflow = '';
  if (toastTimer) {
    clearTimeout(toastTimer);
    toastTimer = null;
  }
});

watch(
  () => [previewItem.value, confirmTarget.value],
  ([preview, confirm]) => {
    document.body.style.overflow = preview || confirm ? 'hidden' : '';
  }
);
</script>

<template>
  <div class="gallery-view">
    <div class="gallery-inner">
      <section class="filters">
        <form class="filter-stack" @submit.prevent="handleSubmit">
          <div class="pair">
            <label class="filter-field field-left wide">
              <span class="filter-title">文件名</span>
              <input
                v-model="keyword"
                type="text"
                placeholder="支持模糊搜索"
                autocomplete="off"
                class="control"
              />
            </label>

            <label class="filter-field field-right compact">
              <span class="filter-title">图床</span>
              <select
                v-model="selectedHost"
                :disabled="hostLoading"
                class="control"
              >
                <option value="">全部图床</option>
                <option v-for="host in hosts" :key="host" :value="host">
                  {{ host }}
                </option>
              </select>
            </label>
          </div>

          <transition name="fold">
            <div v-show="showAdvanced" class="advanced-block">
              <div class="pair">
                <label class="filter-field field-left">
                  <span class="filter-title">开始时间</span>
                  <input
                    v-model="startDate"
                    type="datetime-local"
                    class="control"
                  />
                </label>

                <label class="filter-field field-right">
                  <span class="filter-title">结束时间</span>
                  <input
                    v-model="endDate"
                    type="datetime-local"
                    class="control"
                  />
                </label>
              </div>

              <div class="pair">
                <label class="filter-field field-left">
                  <span class="filter-title">最小大小 (Bytes)</span>
                  <input
                    v-model="minSize"
                    type="number"
                    min="0"
                    step="1"
                    class="control"
                  />
                </label>

                <label class="filter-field field-right">
                  <span class="filter-title">最大大小 (Bytes)</span>
                  <input
                    v-model="maxSize"
                    type="number"
                    min="0"
                    step="1"
                    class="control"
                  />
                </label>
              </div>
            </div>
          </transition>

          <button
            type="button"
            class="advanced-toggle"
            :class="{ active: advancedActive }"
            @click="toggleAdvanced"
          >
            {{ showAdvanced ? '收起高级搜索' : '高级搜索' }}
          </button>

          <div class="action-row">
            <button
              type="button"
              class="ghost"
              @click="resetFilters"
              :disabled="loading"
            >
              重置
            </button>
            <button type="submit" class="primary" :disabled="loading">
              查询
            </button>
          </div>
        </form>
      </section>

      <section class="results">
        <header class="results-head">
          <span class="summary">
            <template v-if="loading">加载中…</template>
            <template v-else>共 {{ items.length }} 张图片</template>
          </span>
          <label class="copy-format" title="复制时使用的格式">
            <span class="label">链接选项</span>
            <select v-model="copyFormat">
              <option
                v-for="option in copyFormatOptions"
                :key="option.value"
                :value="option.value"
              >
                {{ option.label }}
              </option>
            </select>
          </label>
        </header>

        <span v-if="errorMessage" class="error">{{ errorMessage }}</span>

        <div v-if="toast" :class="['action-toast', toast.kind]">
          {{ toast.message }}
        </div>

        <div v-if="!loading && !items.length && !errorMessage" class="empty">
          <p>暂无符合条件的图片</p>
        </div>

        <div v-else class="grid">
          <GalleryItemCard
            v-for="item in items"
            :key="item.id"
            :item="item"
            @preview="openPreview"
            @copy="handleCopy"
            @delete="requestDelete"
          />
        </div>
      </section>

      <teleport to="body">
        <transition name="preview-fade">
          <div
            v-if="confirmTarget"
            class="confirm-overlay"
            @click.self="closeConfirm"
          >
            <div class="confirm-dialog">
              <h3>确认删除</h3>
              <p class="message">
                确定要删除
                <strong>{{
                  confirmTarget.file_name || confirmTarget.url
                }}</strong>
                吗？
              </p>
              <p class="sub">
                将调用
                {{ confirmTarget.host }} 图床删除接口，并从图库移除此记录。
              </p>
              <p v-if="confirmError" class="confirm-error">
                {{ confirmError }}
              </p>
              <div class="confirm-actions">
                <button
                  type="button"
                  class="ghost"
                  @click="closeConfirm"
                  :disabled="deleteLoading"
                >
                  取消
                </button>
                <button
                  type="button"
                  class="danger"
                  @click="confirmDeletion"
                  :disabled="deleteLoading"
                >
                  {{ deleteLoading ? '正在删除…' : '删除' }}
                </button>
              </div>
            </div>
          </div>
        </transition>
      </teleport>

      <teleport to="body">
        <transition name="preview-fade">
          <div
            v-if="previewItem"
            class="preview-overlay"
            @click.self="closePreview"
          >
            <div class="preview-dialog">
              <img
                :src="previewItem.url"
                :alt="previewItem.file_name || previewItem.url"
              />
            </div>
            <button type="button" class="preview-close" @click="closePreview">
              ×
            </button>
          </div>
        </transition>
      </teleport>
    </div>
  </div>
</template>

<style scoped>
.gallery-view {
  width: 100%;
  display: flex;
  flex-direction: column;
  gap: 24px;
  color: var(--text-primary);
}

.gallery-inner {
  width: 100%;
  display: flex;
  flex-direction: column;
  gap: 24px;
}

.gallery-inner > section {
  width: 100%;
}

.filters {
  background: var(--surface-panel);
  border-radius: 20px;
  padding: 24px;
  box-shadow: var(--shadow-strong);
  border: 1px solid var(--surface-border);
  backdrop-filter: blur(22px) saturate(1.06);
  transition: border-color 0.2s ease, box-shadow 0.2s ease;
}

.filters:hover {
  border-color: var(--surface-border);
  border-color: color-mix(in srgb, var(--surface-border) 60%, var(--accent));
  box-shadow: 0 28px 52px rgba(6, 12, 24, 0.3);
}

.filter-stack {
  display: flex;
  flex-direction: column;
  gap: 18px;
}

.pair {
  display: grid;
  grid-template-columns: repeat(2, minmax(0, 1fr));
  gap: 0;
  align-items: stretch;
}

.advanced-block {
  display: flex;
  flex-direction: column;
  gap: 16px;
}

.action-row {
  display: flex;
  justify-content: flex-end;
  gap: 12px;
}

.filter-field {
  display: flex;
  flex-direction: column;
  gap: 8px;
  height: 100%;
}

.filter-title {
  font-size: 14px;
  font-weight: 600;
  color: var(--text-secondary);
}

.filter-field .control {
  width: 100%;
  box-sizing: border-box;
  padding: 10px 12px;
  border-radius: 12px;
  border: 1px solid var(--surface-border);
  background: var(--surface-acrylic);
  color: var(--text-primary);
  font-size: 14px;
  min-height: 42px;
  box-shadow: inset 0 1px 0 rgba(255, 255, 255, 0.06);
  transition: border-color 0.2s ease, box-shadow 0.2s ease;
}

.filter-field .control:focus {
  outline: none;
  border-color: var(--accent);
  border-color: color-mix(in srgb, var(--accent) 70%, transparent);
  box-shadow: 0 0 0 2px rgba(122, 163, 255, 0.18);
  box-shadow: 0 0 0 2px color-mix(in srgb, var(--accent) 18%, transparent);
}

.filter-field.field-left .control {
  border-top-right-radius: 0;
  border-bottom-right-radius: 0;
  border-right: 1px solid var(--surface-border);
}

.filter-field.field-right .control {
  border-top-left-radius: 0;
  border-bottom-left-radius: 0;
  border-left: none;
}

.filter-field.field-left.wide .control {
  border-radius: 12px 0 0 12px;
}

.filter-field.field-right.compact .control {
  border-radius: 0 12px 12px 0;
}

.filter-field.field-right.compact select.control {
  min-width: 180px;
  height: 42px;
}

.action-row .primary,
.action-row .ghost {
  padding: 10px 18px;
  border-radius: 12px;
  font-weight: 600;
  transition: transform 0.2s ease, box-shadow 0.2s ease, opacity 0.18s ease,
    background 0.2s ease;
}

.action-row .primary {
  border: none;
  background: linear-gradient(135deg, var(--accent), rgba(183, 148, 255, 0.92));
  background: linear-gradient(
    135deg,
    var(--accent),
    color-mix(in srgb, var(--accent) 65%, #b794ff 35%)
  );
  color: #fff;
  box-shadow: 0 12px 30px rgba(122, 163, 255, 0.28);
  box-shadow: 0 12px 30px
    color-mix(in srgb, var(--accent) 32%, rgba(0, 0, 0, 0.38));
}

.action-row .primary:hover:not(:disabled) {
  transform: translateY(-2px);
  box-shadow: 0 18px 44px rgba(122, 163, 255, 0.32);
  box-shadow: 0 18px 44px
    color-mix(in srgb, var(--accent) 40%, rgba(0, 0, 0, 0.4));
}

.action-row .ghost {
  border: 1px solid var(--surface-border);
  background: var(--surface-acrylic);
  color: var(--text-secondary);
}

.action-row .ghost:hover:not(:disabled) {
  color: var(--text-primary);
  border-color: var(--accent);
  border-color: color-mix(in srgb, var(--accent) 55%, transparent);
  transform: translateY(-1px);
}

.action-row .primary:disabled,
.action-row .ghost:disabled {
  opacity: 0.6;
  cursor: not-allowed;
  transform: none;
  box-shadow: none;
}

.advanced-toggle {
  border: none;
  background: transparent;
  color: var(--text-secondary);
  font-size: 14px;
  font-weight: 600;
  cursor: pointer;
  padding: 0;
  transition: color 0.2s ease;
  align-self: flex-start;
}

.advanced-toggle:hover,
.advanced-toggle.active {
  color: var(--accent);
}

.fold-enter-active,
.fold-leave-active {
  transition: opacity 0.2s ease, transform 0.2s ease;
  transform-origin: top;
}

.fold-enter-from,
.fold-leave-to {
  opacity: 0;
  transform: translateY(-8px);
}

.results {
  display: flex;
  flex-direction: column;
  gap: 18px;
}

.results-head {
  display: flex;
  justify-content: space-between;
  align-items: center;
  padding: 0 4px;
}

.summary {
  font-size: 15px;
  color: var(--text-secondary);
}

.copy-format {
  display: flex;
  align-items: center;
  gap: 10px;
  font-size: 13px;
  color: var(--text-secondary);
}

.copy-format .label {
  font-weight: 600;
  white-space: nowrap;
}

.copy-format select {
  appearance: none;
  padding: 6px 28px 6px 12px;
  border-radius: 10px;
  border: 1px solid var(--surface-border);
  background: var(--surface-acrylic)
    url('data:image/svg+xml;utf8,<svg xmlns="http://www.w3.org/2000/svg" width="14" height="14" viewBox="0 0 24 24"><path fill="%23aab6d3" d="M7 10l5 5 5-5z"/></svg>')
    no-repeat right 10px center;
  color: var(--text-primary);
  font-size: 13px;
  transition: border-color 0.2s ease, box-shadow 0.2s ease;
}

.copy-format select:focus {
  outline: none;
  border-color: var(--accent);
  border-color: color-mix(in srgb, var(--accent) 65%, transparent);
  box-shadow: 0 0 0 2px rgba(122, 163, 255, 0.16);
  box-shadow: 0 0 0 2px color-mix(in srgb, var(--accent) 16%, transparent);
}

.error {
  color: var(--danger);
  font-size: 14px;
  margin: 6px 4px 0;
}

.action-toast {
  align-self: flex-start;
  margin: 0 4px 12px;
  padding: 10px 14px;
  border-radius: 12px;
  font-size: 13px;
  font-weight: 500;
  display: inline-flex;
  align-items: center;
  gap: 8px;
  background: var(--accent-soft);
  border: 1px solid var(--surface-border);
  border-color: color-mix(in srgb, var(--accent) 28%, transparent);
  color: var(--accent);
}

.action-toast.success {
  background: rgba(44, 187, 126, 0.18);
  border-color: rgba(44, 187, 126, 0.32);
  color: #2cbb7e;
}

.action-toast.error {
  background: var(--danger-soft);
  border-color: var(--danger);
  border-color: color-mix(in srgb, var(--danger) 32%, transparent);
  color: var(--danger);
}

.empty {
  display: flex;
  justify-content: center;
  align-items: center;
  min-height: 200px;
  border-radius: 20px;
  background: var(--surface-acrylic);
  border: 1px dashed var(--surface-border);
  color: var(--text-secondary);
}

.grid {
  display: grid;
  gap: 18px;
  grid-template-columns: repeat(auto-fill, minmax(200px, 1fr));
}

.preview-overlay {
  position: fixed;
  inset: 0;
  background: transparent;
  display: flex;
  justify-content: center;
  align-items: center;
  padding: 40px;
  z-index: 1200;
}

.preview-dialog {
  max-width: 90vw;
  max-height: 90vh;
  border-radius: 18px;
  overflow: hidden;
  background: var(--surface-panel);
  border: 1px solid var(--surface-border);
  box-shadow: 0 30px 60px rgba(5, 8, 18, 0.45);
  display: flex;
}

.preview-dialog img {
  display: block;
  max-width: 100%;
  max-height: 100%;
  object-fit: contain;
  background: var(--surface-acrylic-strong);
}

.preview-close {
  position: fixed;
  top: 32px;
  right: 32px;
  width: 40px;
  height: 40px;
  border-radius: 50%;
  border: 1px solid var(--surface-border);
  background: var(--surface-panel);
  color: var(--text-primary);
  font-size: 24px;
  font-weight: 600;
  display: flex;
  align-items: center;
  justify-content: center;
  cursor: pointer;
  box-shadow: 0 12px 30px rgba(6, 10, 22, 0.35);
  transition: background 0.2s ease, transform 0.2s ease, color 0.2s ease;
}

.preview-close:hover {
  color: var(--accent);
  transform: translateY(-1px);
}

.preview-fade-enter-active,
.preview-fade-leave-active {
  transition: opacity 0.2s ease;
}

.preview-fade-enter-from,
.preview-fade-leave-to {
  opacity: 0;
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

.confirm-dialog {
  width: min(420px, 90vw);
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

.confirm-error {
  margin: 0;
  font-size: 13px;
  color: var(--danger);
  background: var(--danger-soft);
  border: 1px solid var(--danger);
  border: 1px solid color-mix(in srgb, var(--danger) 26%, transparent);
  border-radius: 12px;
  padding: 10px 12px;
}

.confirm-actions {
  display: flex;
  justify-content: flex-end;
  gap: 12px;
  margin-top: 8px;
}

.confirm-actions .ghost,
.confirm-actions .danger {
  padding: 10px 18px;
  border-radius: 12px;
  font-weight: 600;
  font-size: 14px;
  cursor: pointer;
  transition: background 0.2s ease, transform 0.2s ease, opacity 0.2s ease,
    border-color 0.2s ease;
}

.confirm-actions .ghost {
  border: 1px solid var(--surface-border);
  background: var(--surface-acrylic);
  color: var(--text-secondary);
}

.confirm-actions .ghost:hover:not(:disabled) {
  color: var(--text-primary);
  border-color: var(--accent);
  border-color: color-mix(in srgb, var(--accent) 55%, transparent);
  transform: translateY(-1px);
}

.confirm-actions .danger {
  border: none;
  background: var(--danger);
  color: #fff;
  box-shadow: 0 14px 32px rgba(229, 62, 62, 0.26);
}

.confirm-actions .danger:hover:not(:disabled) {
  transform: translateY(-1px);
  box-shadow: 0 18px 42px rgba(229, 62, 62, 0.32);
}

.confirm-actions button:disabled {
  opacity: 0.65;
  cursor: not-allowed;
  transform: none;
  box-shadow: none;
}

@media (max-width: 640px) {
  .filters {
    padding: 18px;
  }

  .pair {
    grid-template-columns: 1fr;
    gap: 16px;
  }

  .action-row {
    flex-direction: column-reverse;
    align-items: stretch;
  }

  .action-row .primary,
  .action-row .ghost,
  .advanced-toggle {
    width: 100%;
    text-align: center;
  }

  .results-head {
    flex-direction: column;
    align-items: flex-start;
    gap: 12px;
  }

  .copy-format {
    width: 100%;
    justify-content: space-between;
  }

  .copy-format select {
    width: 100%;
    background-position: right 12px center;
  }

  .filter-field.field-left .control,
  .filter-field.field-right .control,
  .filter-field.field-left.wide .control,
  .filter-field.field-right.compact .control {
    border-radius: 12px;
    border-left: 1px solid var(--surface-border);
  }

  .confirm-dialog {
    padding: 24px 18px;
    gap: 14px;
  }

  .confirm-dialog h3 {
    font-size: 18px;
  }

  .confirm-actions {
    flex-direction: column-reverse;
    align-items: stretch;
  }

  .confirm-actions .ghost,
  .confirm-actions .danger {
    width: 100%;
  }
}
</style>

<!-- Global styles for teleported modals (ensure they are fixed to viewport and block interactions) -->
<style>
/* Ensure teleported overlays sit above everything and are centered in viewport */
.preview-overlay,
.confirm-overlay {
  position: fixed;
  inset: 0;
  display: flex;
  justify-content: center;
  align-items: center;
  background: rgba(
    0,
    0,
    0,
    0
  ); /* background handled inside classes, keep transparent by default */
  z-index: 9999;
  pointer-events: auto;
}

/* Backdrop colors are already set in scoped styles (.preview-overlay/.confirm-overlay) but
   ensure teleported overlay background covers viewport and blocks clicks to underlying content */
.preview-overlay::before,
.confirm-overlay::before {
  content: '';
  position: fixed;
  inset: 0;
  background: var(--modal-backdrop);
  z-index: -1; /* sit behind dialog but above page */
}

/* Prevent underlying elements from receiving pointer events while overlay is visible */
body.modal-open *:not(.preview-overlay):not(.confirm-overlay) {
  pointer-events: none;
}

/* But allow interactions with the overlay itself and its children */
.preview-overlay,
.confirm-overlay,
.preview-overlay *,
.confirm-overlay * {
  pointer-events: auto;
}

/* Ensure preview close button sits above dialog */
.preview-close {
  z-index: 10001;
}
</style>
