<script setup lang="ts">
import { onMounted, ref, computed, onBeforeUnmount, watch } from 'vue';
import { invoke } from '@tauri-apps/api/core';
import GlobalSelect from '../components/GlobalSelect.vue';
import GalleryItemCard from '../components/GalleryItemCard.vue';
import ImagePreviewModal from '../components/ImagePreviewModal.vue';
import { vRegisterCard } from '../directives/vRegisterCard';
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
import { useSettingsStore } from '../stores/settings';
import { useBatchSelectStore } from '../stores/batchSelect';
import { retryAsync } from '../utils/retry';

// ========== 使用 Pinia Store ==========
const batchSelectStore = useBatchSelectStore();

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
const confirmTarget = ref<any>(null);
const confirmError = ref('');
const deleteLoading = ref(false);
const LOCALSTORAGE_KEY_FORMAT = 'yana.upload.lastFormat';
let initialCopyFormat: 'link' | 'html' | 'bbcode' | 'markdown' = 'link';
try {
  const raw = localStorage.getItem(LOCALSTORAGE_KEY_FORMAT);
  if (
    raw === 'link' ||
    raw === 'html' ||
    raw === 'bbcode' ||
    raw === 'markdown'
  ) {
    initialCopyFormat = raw;
  }
} catch (e) {
  // ignore
}
const copyFormat = ref<'link' | 'html' | 'bbcode' | 'markdown'>(
  initialCopyFormat
);

const copyFormatOptions: Array<{
  value: typeof copyFormat.value;
  label: string;
}> = [
  { value: 'link', label: '纯链接' },
  { value: 'html', label: 'HTML' },
  { value: 'markdown', label: 'Markdown' },
  { value: 'bbcode', label: 'BBCode' },
];

// persist copyFormat changes to localStorage so UploadView can share the same setting
watch(
  () => copyFormat.value,
  (val, prev) => {
    try {
      localStorage.setItem(LOCALSTORAGE_KEY_FORMAT, val);
      void logInfo(`[gallery] 保存复制格式: ${val} (之前: ${prev})`);
    } catch (e) {
      void logWarn(`[gallery] 无法保存复制格式: ${String(e)}`);
    }
  }
);

// options for host select
const hostOptions = computed(() =>
  hosts.value.map((h) => ({ value: h, label: h }))
);

// ========== 拖拽相关 ==========

/** 拖拽起始坐标 */
let dragStartX = 0;
let dragStartY = 0;

/**
 * 自定义 throttle 函数用于优化 mousemove 事件处理
 */
function createThrottle(fn: Function, interval: number) {
  let lastTime = 0;
  let timeoutId: ReturnType<typeof setTimeout> | null = null;

  return function throttled(...args: any[]) {
    const now = Date.now();
    const remaining = interval - (now - lastTime);

    if (remaining <= 0) {
      lastTime = now;
      fn.apply(null, args);
    } else if (!timeoutId) {
      timeoutId = setTimeout(() => {
        lastTime = Date.now();
        timeoutId = null;
        fn.apply(null, args);
      }, remaining);
    }
  };
}

/**
 * 处理卡片按下事件，开始拖拽
 */
function handleCardMouseDown(event: MouseEvent, itemId: number) {
  if (!batchSelectStore.batchMode) return;
  if (!event.ctrlKey || event.button !== 0) return;

  event.preventDefault();
  dragStartX = event.clientX;
  dragStartY = event.clientY;

  batchSelectStore.startCtrlDrag(itemId);
}

/**
 * 处理卡片点击事件（Ctrl+Click）
 */
function handleCardClick(event: MouseEvent, itemId: number) {
  if (!batchSelectStore.batchMode) return;
  if (!event.ctrlKey) return;

  event.preventDefault();
  event.stopPropagation();
  batchSelectStore.toggleSelectItem(itemId);
}

/**
 * 处理文档鼠标移动事件
 * 使用矩形相交检测找到被拖拽覆盖的卡片
 */
function handleDocumentMouseMove(event: MouseEvent) {
  if (!batchSelectStore.isCtrlDragging) return;

  // 使用 Store 方法计算与拖拽矩形相交的所有卡片
  const intersectingCardIds = batchSelectStore.getIntersectingCards(
    dragStartX,
    dragStartY,
    event.clientX,
    event.clientY
  );

  // 批量选择这些卡片
  batchSelectStore.selectMultiple(intersectingCardIds);
}

// 创建节流版本的 mousemove 处理（16ms = ~60fps）
const handleDocumentMouseMoveThrottled = createThrottle(
  handleDocumentMouseMove,
  16
);

/**
 * 处理文档鼠标松开事件
 */
function handleDocumentMouseUp() {
  if (!batchSelectStore.isCtrlDragging) return;
  batchSelectStore.endCtrlDrag();
}

/**
 * 切换批量模式
 */
function toggleBatchMode() {
  batchSelectStore.toggleBatchMode();
}

/**
 * 清空选择
 */
function clearBatchSelection() {
  batchSelectStore.clearSelection();
}

/**
 * 导出选中项的链接
 */
async function exportLinksOfSelection() {
  const selectedIds = batchSelectStore.getSelectedIds();
  const selectedItems = selectedIds
    .map((id) => items.value.find((it) => it.id === id))
    .filter(Boolean) as typeof items.value;
  const text = selectedItems.map((it) => buildCopyText(it)).join('\n');
  try {
    if (!navigator.clipboard || !navigator.clipboard.writeText)
      throw new Error('不支持剪贴板');
    await navigator.clipboard.writeText(text);
    showToast(`已复制 ${selectedItems.length} 条链接到剪贴板`, 'success');
  } catch (err) {
    showToast('复制链接失败', 'error');
    void logError(`[gallery] export links failed: ${String(err)}`);
  }
}

/**
 * 删除选中的项
 */
async function deleteSelectedItems() {
  const selectedIds = batchSelectStore.getSelectedIds();
  if (!selectedIds.length) return;
  confirmTarget.value = {
    batchIds: selectedIds,
    message: `确定要删除选中的 ${selectedIds.length} 张图片吗？`,
  };
}

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

    // 批量预加载缩略图（不阻塞主流程）
    const settings = useSettingsStore();
    if (settings.enableThumbnailCache.value && result.length > 0) {
      const urls = result.map((item) => item.url);
      // 分批处理以避免单次请求过大
      const batchSize = 20;
      for (let i = 0; i < urls.length; i += batchSize) {
        const batch = urls.slice(i, i + batchSize);
        void invoke('generate_thumbnails', { urls: batch }).catch(
          (err: any) => {
            void logWarn(
              `[gallery] 批量缩略图生成失败 (batch ${i}-${
                i + batchSize
              }): ${String(err)}`
            );
          }
        );
      }
    }
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

  // handle batch deletion
  if (
    confirmTarget.value.batchIds &&
    Array.isArray(confirmTarget.value.batchIds)
  ) {
    const ids: number[] = confirmTarget.value.batchIds.slice();
    const settings = useSettingsStore();
    const concurrency = Math.max(1, settings.maxConcurrentUploads.value ?? 5);
    let deleted = 0;
    const idQueue = ids.slice();

    // worker 池并发处理队列
    const workers = Array.from({ length: concurrency }).map(async () => {
      while (idQueue.length) {
        const id = idQueue.shift();
        if (id == null) break;
        const target = items.value.find((it) => it.id === id);
        if (!target) continue;
        try {
          const plugin = imageHostStore.getPluginById(target.host);
          if (plugin && target.delete_marker) {
            const deleteMarker = target.delete_marker;
            const res = await retryAsync(
              async () => {
                return await plugin.remove(
                  deleteMarker,
                  imageHostStore.runtime
                );
              },
              { maxRetries: 1 }
            );
            if (!res?.success) {
              void logWarn(
                `[gallery] batch plugin delete failed (id=${id}): ${res?.message}`
              );
            }
          }
        } catch (err) {
          void logError(
            `[gallery] batch plugin delete exception (id=${id}): ${String(err)}`
          );
        }
        try {
          await deleteGalleryItem(id);
          // 以原子方式更新 items
          items.value = items.value.filter((it) => it.id !== id);
          deleted++;
        } catch (err) {
          void logError(
            `[gallery] batch db delete failed (id=${id}): ${String(err)}`
          );
        }
      }
    });

    await Promise.all(workers);
    showToast(`已删除 ${deleted} 张图片`, 'success');
    // 清空选择并退出批量模式
    batchSelectStore.clearSelection();
    batchSelectStore.batchMode = false;
    closeConfirm();
    deleteLoading.value = false;
    return;
  }

  // single item deletion (existing flow)
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
      const result = await retryAsync(
        async () => {
          return await plugin.remove(
            target.delete_marker,
            imageHostStore.runtime
          );
        },
        { maxRetries: 1 }
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
  // 使用节流版本的 mousemove 处理以改进性能
  document.addEventListener(
    'mousemove',
    handleDocumentMouseMoveThrottled as EventListener
  );
  document.addEventListener('mouseup', handleDocumentMouseUp);
  await loadHosts();
  await fetchItems();
  if (advancedActive.value) {
    showAdvanced.value = true;
  }
});

onBeforeUnmount(() => {
  window.removeEventListener('keydown', handleKeydown);
  document.removeEventListener(
    'mousemove',
    handleDocumentMouseMoveThrottled as EventListener
  );
  document.removeEventListener('mouseup', handleDocumentMouseUp);
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
              <GlobalSelect
                v-model="selectedHost"
                :options="[{ value: '', label: '全部图床' }, ...hostOptions]"
                :disabled="hostLoading"
                class="gallery-select"
              />
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
            <div class="select-wrapper">
              <GlobalSelect
                v-model="copyFormat"
                :options="copyFormatOptions"
                class="control"
              />
            </div>
          </label>
          <button
            type="button"
            class="ghost batch-toggle-btn"
            :class="{ active: batchSelectStore.batchMode }"
            @click="toggleBatchMode"
          >
            {{ batchSelectStore.batchMode ? '退出批量选择' : '批量选择' }}
          </button>
        </header>

        <span v-if="errorMessage" class="error">{{ errorMessage }}</span>

        <div v-if="toast" :class="['action-toast', toast.kind]">
          {{ toast.message }}
        </div>

        <div v-if="!loading && !items.length && !errorMessage" class="empty">
          <p>暂无符合条件的图片</p>
        </div>

        <div v-else class="grid">
          <div
            v-for="item in items"
            v-register-card="item.id"
            :key="item.id"
            class="card-wrapper"
            :class="{
              'batch-active': batchSelectStore.batchMode,
              'is-dragging': batchSelectStore.isCtrlDragging,
              selected: batchSelectStore.isSelected(item.id),
            }"
            :data-item-id="item.id"
            @click.stop="
              batchSelectStore.batchMode
                ? batchSelectStore.toggleSelectItem(item.id)
                : null
            "
            @mousedown.stop="(e: any) => handleCardMouseDown(e, item.id)"
            @click.ctrl.stop="(e: any) => handleCardClick(e, item.id)"
          >
            <GalleryItemCard
              :item="item"
              :showSelection="batchSelectStore.batchMode"
              :selectedIndex="
                batchSelectStore.batchMode
                  ? batchSelectStore.selectedIndex(item.id)
                  : null
              "
              :isDragging="batchSelectStore.isCtrlDragging"
              :batchMode="batchSelectStore.batchMode"
              @preview="openPreview"
              @copy="handleCopy"
              @delete="requestDelete"
              @toggle-select="() => batchSelectStore.toggleSelectItem(item.id)"
            />
          </div>
        </div>
        <!-- 批量操作底部横条 -->
        <div v-if="batchSelectStore.batchMode" class="batch-action-bar">
          <div class="bar-content">
            <div class="left">
              已选 {{ batchSelectStore.selectionCount }} 张
            </div>
            <div class="center">
              <button
                class="ghost"
                @click="exportLinksOfSelection"
                :disabled="!batchSelectStore.selectionCount"
              >
                导出链接
              </button>
              <button
                class="danger"
                @click="deleteSelectedItems"
                :disabled="!batchSelectStore.selectionCount"
              >
                删除
              </button>
              <button
                class="ghost"
                @click="
                  () => {
                    clearBatchSelection();
                    toggleBatchMode();
                  }
                "
              >
                取消
              </button>
            </div>
            <div class="right"></div>
          </div>
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
              <p class="message" v-if="confirmTarget && confirmTarget.batchIds">
                {{
                  confirmTarget.message ||
                  `确定要删除选中的 ${confirmTarget.batchIds.length} 张图片吗？`
                }}
              </p>
              <p class="message" v-else>
                确定要删除
                <strong>{{
                  confirmTarget?.file_name || confirmTarget?.url
                }}</strong>
                吗？
              </p>
              <p class="sub" v-if="confirmTarget && confirmTarget.batchIds">
                将调用对应图床删除接口，并从图库中移除这些记录。
              </p>
              <p class="sub" v-else>
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

      <!-- 新的预览组件 -->
      <ImagePreviewModal
        :item="previewItem"
        :is-open="!!previewItem"
        @close="closePreview"
      />
    </div>
  </div>
</template>

<style scoped>
/* 批量选择开关样式（应用于按钮） */
.batch-toggle-btn {
  /* 默认与页面 ghost 风格一致 */
  display: inline-flex;
  align-items: center;
  gap: 8px;
  padding: 10px 16px;
  border-radius: 12px;
  background: var(--surface-acrylic);
  color: var(--text-secondary);
  border: 1px solid var(--surface-border);
  cursor: pointer;
  transition: transform 0.12s ease, background 0.12s ease, color 0.12s ease;
  font-weight: 700;
}
.batch-toggle-btn:hover {
  transform: translateY(-1px);
  color: var(--text-primary);
  border-color: var(--accent);
}
.batch-toggle-btn.active {
  /* active 时借用 primary 的高亮渐变与阴影 */
  background: linear-gradient(
    135deg,
    var(--accent),
    color-mix(in srgb, var(--accent) 65%, #b794ff 35%)
  );
  color: #fff;
  box-shadow: 0 12px 30px
    color-mix(in srgb, var(--accent) 32%, rgba(0, 0, 0, 0.38));
  border-color: transparent;
}

/* 卡片遮罩样式（覆盖整个卡片） */
.card-overlay {
  position: absolute;
  inset: 0;
  border-radius: 12px;
  background: linear-gradient(180deg, rgba(0, 0, 0, 0), rgba(0, 0, 0, 0.25));
  opacity: 0;
  transition: opacity 0.12s ease, backdrop-filter 0.12s ease;
  pointer-events: none;
}
.card-wrapper.selected .card-overlay {
  opacity: 1;
  backdrop-filter: blur(4px) saturate(1.05);
}

.selection-badge {
  position: absolute;
  top: 10px;
  left: 10px;
  width: 32px;
  height: 32px;
  border-radius: 50%;
  display: inline-grid;
  place-items: center;
  background: var(--accent);
  color: var(--on-accent, #fff);
  font-weight: 600;
  font-size: 13px;
  box-shadow: 0 6px 18px rgba(2, 6, 23, 0.6);
  border: 2px solid rgba(255, 255, 255, 0.06);
  transition: transform 0.12s ease, opacity 0.12s ease;
}
.selection-badge.small {
  width: 18px;
  height: 18px;
  font-size: 10px;
  top: 8px;
  left: 8px;
}

/* 底部动作栏：更贴近底部并微调样式 */
.batch-action-bar {
  position: fixed;
  left: 0;
  right: 0;
  bottom: 12px; /* 原来可能更大，改为 12px 更贴近底部 */
  display: flex;
  justify-content: center;
  pointer-events: none; /* 使外层不捕获事件，内层按钮仍可交互 */
  z-index: 60;
}
.batch-action-bar .bar-content {
  pointer-events: auto;
  width: min(1100px, calc(100% - 48px));
  display: flex;
  align-items: center;
  gap: 12px;
  padding: 12px 18px;
  border-radius: 14px;
  background: rgba(20, 26, 34, 0.56);
  backdrop-filter: blur(8px) saturate(1.08);
  box-shadow: 0 6px 30px rgba(2, 6, 23, 0.6);
  border: 1px solid rgba(255, 255, 255, 0.03);
}
.batch-action-bar .bar-content .count {
  color: var(--text-secondary);
  margin-right: auto;
}
.batch-action-bar .bar-content .btn {
  margin-left: 6px;
}

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

/* override GlobalSelect in gallery view filter */
.filter-field .gallery-select {
  width: 100%;
  box-sizing: border-box;
}
.filter-field .gallery-select .select-trigger {
  width: 100%;
  padding: 10px 12px;
  border-radius: 12px;
  border: 1px solid var(--surface-border) !important;
  background: var(--surface-acrylic) !important;
  color: var(--text-primary) !important;
  font-size: 14px;
  min-height: 42px;
  display: flex;
  align-items: center;
  box-shadow: inset 0 1px 0 rgba(255, 255, 255, 0.06) !important;
}
.filter-field .gallery-select .select-trigger .icon {
  color: var(--text-secondary) !important;
}

/* 图床选择器：匹配右侧字段的完整样式（圆角、边框、高度），适配两种主题 */
.filter-field.field-right.compact ::v-deep(.gallery-select .select-trigger) {
  border-radius: 0 12px 12px 0 !important;
  border-top-left-radius: 0 !important;
  border-bottom-left-radius: 0 !important;
  border: 1px solid var(--surface-border) !important;
  border-left: none !important;
  border-top: 1px solid var(--surface-border) !important;
  border-right: 1px solid var(--surface-border) !important;
  border-bottom: 1px solid var(--surface-border) !important;
  min-height: 42px !important;
  height: 42px !important;
  box-sizing: border-box !important;
  /* 内阴影高光效果，与其他输入框一致 */
  box-shadow: inset 0 1px 0 rgba(255, 255, 255, 0.06) !important;
  /* 保持与右侧 control 一致的过渡效果 */
  transition: border-color 0.2s ease, box-shadow 0.2s ease !important;
}

/* 图床选择器聚焦/激活状态，匹配其他输入框 */
.filter-field.field-right.compact
  ::v-deep(.gallery-select.open .select-trigger),
.filter-field.field-right.compact
  ::v-deep(.gallery-select .select-trigger:focus-within) {
  border-color: var(--accent) !important;
  border-color: color-mix(in srgb, var(--accent) 70%, transparent) !important;
  box-shadow: 0 0 0 2px color-mix(in srgb, var(--accent) 18%, transparent) !important;
}

.grid {
  display: grid;
  grid-template-columns: repeat(auto-fill, minmax(220px, 1fr));
  gap: 16px;
}

.card-wrapper {
  position: relative;
  cursor: pointer;
  /* CSS containment: 隔离该元素的渲染，提高性能（不包含 layout 以保持 border-radius） */
  contain: paint style;
  /* will-change: 提示浏览器该元素将变化 */
  will-change: transform;
  /* 确保卡片下部圆角显示 */
  border-radius: 18px;
  overflow: hidden;
}
.card-wrapper .selection-badge {
  position: absolute;
  top: 8px;
  left: 8px;
  width: 28px;
  height: 28px;
  border-radius: 14px;
  display: flex;
  align-items: center;
  justify-content: center;
  background: rgba(0, 0, 0, 0.45);
  color: #fff;
  font-weight: 700;
  font-size: 12px;
}
.card-wrapper .selection-badge .dot {
  width: 8px;
  height: 8px;
  border-radius: 50%;
  background: rgba(255, 255, 255, 0.8);
}
.card-wrapper.selected {
  box-shadow: 0 8px 24px rgba(0, 0, 0, 0.12);
  transform: translateY(-4px);
  border-radius: 12px;
  outline: 0;
}

/* 拖拽中禁用所有过渡动画以改进性能 */
.card-wrapper.is-dragging {
  pointer-events: none;
}
.card-wrapper.is-dragging :deep(.card) {
  transition: none !important;
}

/* 批量模式下禁用 hover 动画，避免鼠标悬停时的视觉干扰 */
.card-wrapper.batch-active :deep(.card) {
  transition: none !important;
}
.card-wrapper.batch-active :deep(.card):hover {
  transform: none !important;
  box-shadow: var(--shadow-soft) !important;
}

/* 底部批量操作条：居中圆角半透明矩形，主题自适应 */
.batch-action-bar {
  position: sticky;
  bottom: 18px;
  width: 100%;
  display: flex;
  justify-content: center;
  pointer-events: none; /* 外层不接收点击，内部 .bar-content 接收 */
  z-index: 30;
}
.batch-action-bar .bar-content {
  pointer-events: auto;
  max-width: 1100px;
  width: calc(100% - 40px);
  margin: 0 auto;
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 12px;
  background: color-mix(in srgb, var(--surface-panel) 88%, transparent);
  border-radius: 14px;
  padding: 10px 14px;
  border: 1px solid color-mix(in srgb, var(--surface-border) 72%, transparent);
  box-shadow: 0 8px 20px rgba(6, 12, 24, 0.16);
  backdrop-filter: blur(8px) saturate(1.05);
  color: var(--text-primary);
}

/* 按钮统一样式（底栏） */
.batch-action-bar button {
  border-radius: 10px;
  padding: 8px 12px;
  font-weight: 700;
  border: 1px solid transparent;
  cursor: pointer;
  transition: transform 0.12s ease, background 0.12s ease, opacity 0.12s ease;
}
.batch-action-bar button:hover {
  transform: translateY(-2px);
}

.batch-action-bar .ghost {
  background: transparent;
  border-color: color-mix(in srgb, var(--surface-border) 72%, transparent);
  color: var(--text-primary);
}
.batch-action-bar .ghost:active {
  opacity: 0.9;
}

.batch-action-bar .danger {
  background: linear-gradient(
    180deg,
    var(--danger),
    color-mix(in srgb, var(--danger) 90%, black 10%)
  );
  color: #fff;
  border-color: transparent;
}
.batch-action-bar .danger:active {
  transform: translateY(0);
  opacity: 0.95;
}

/* 徽章颜色适配主题 */
.card-wrapper .selection-badge {
  position: absolute;
  top: 8px;
  left: 8px;
  min-width: 28px;
  height: 28px;
  border-radius: 14px;
  display: flex;
  align-items: center;
  justify-content: center;
  color: #fff;
  font-weight: 700;
  font-size: 12px;
  background: var(--accent);
  box-shadow: 0 6px 14px rgba(0, 0, 0, 0.18);
}
.card-wrapper .selection-badge .dot {
  background: rgba(255, 255, 255, 0.9);
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

.copy-format .select-wrapper {
  position: relative;
  display: inline-flex;
  align-items: center;
}

.copy-format select {
  appearance: none;
  padding: 6px 34px 6px 12px;
  border-radius: 10px;
  border: 1px solid var(--surface-border);
  background: var(--surface-acrylic);
  color: var(--text-primary);
  font-size: 13px;
  transition: border-color 0.2s ease, box-shadow 0.2s ease;
}

.copy-format .select-icon {
  position: absolute;
  right: 12px;
  top: 50%;
  transform: translateY(-50%);
  color: var(--text-secondary);
  pointer-events: none;
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

/* 预览样式已迁移到 ImagePreviewModal 组件 */

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

/* 覆盖图库界面中图床 GlobalSelect 样式，使其与外层 filter control 一致 */
.gallery-select .select-trigger {
  width: 100%;
  padding: 10px 12px;
  border-radius: 12px;
  border: 1px solid var(--surface-border) !important;
  background: var(--surface-acrylic) !important;
  color: var(--text-primary) !important;
  font-size: 14px;
  min-height: 42px;
  box-shadow: inset 0 1px 0 rgba(255, 255, 255, 0.06) !important;
}
.gallery-select .select-trigger .icon {
  color: var(--text-secondary) !important;
}

/* Global styles for teleported modals (ensure they are fixed to viewport and block interactions) */
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
