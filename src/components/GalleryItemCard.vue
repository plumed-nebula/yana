<script setup lang="ts">
import { computed, toRefs, onMounted, ref } from 'vue';
import type { GalleryItem } from '../types/gallery';
import { Link2, Trash2 } from 'lucide-vue-next';
import { invoke, convertFileSrc } from '@tauri-apps/api/core';

const props = defineProps<{
  item: GalleryItem;
  showSelection?: boolean;
  selectedIndex?: number | null;
  isDragging?: boolean;
  batchMode?: boolean;
}>();

const { item, showSelection, selectedIndex, isDragging, batchMode } = toRefs(
  props as any
);

const displayName = computed(() => item.value.file_name ?? item.value.url);

const localTimestamp = computed(() => {
  const value = item.value.inserted_at;
  if (!value) return '';
  const date = new Date(value);
  if (Number.isNaN(date.getTime())) {
    return value;
  }
  return date.toLocaleString();
});

const emit = defineEmits<{
  (e: 'preview', item: GalleryItem): void;
  (e: 'copy', item: GalleryItem): void;
  (e: 'delete', item: GalleryItem): void;
  (e: 'toggle-select'): void;
}>();

const thumbnailPath = ref<string>('');

onMounted(async () => {
  try {
    // 从后端查询缩略图路径（后端负责所有路径生成逻辑）
    const result = await invoke<string | null>('get_thumbnail_path', {
      url: item.value.url,
    });
    if (result) {
      thumbnailPath.value = result;
    }
  } catch {
    // 查询失败，继续使用原始 URL
  }
});

function handlePreview(e?: Event) {
  // 如果正在进行 Ctrl 拖拽，不触发预览
  if (isDragging?.value) return;

  try {
    // only inspect for badge when this is a MouseEvent (keyboard events shouldn't check)
    if (e instanceof MouseEvent) {
      const target = e.target as HTMLElement | null;
      if (target && target.closest('.select-badge')) {
        // click originated from the badge area — ignore preview here
        return;
      }
    }
  } catch (err) {
    // ignore errors and continue
  }
  emit('preview', item.value);
}

function handleCopy() {
  // 批量模式下禁用复制
  if (batchMode?.value) return;
  emit('copy', item.value);
}

function handleDelete() {
  // 批量模式下禁用删除
  if (batchMode?.value) return;
  emit('delete', item.value);
}

function handleBadgeClick() {
  // emit toggle-select so parent can toggle selection while we stop propagation in template
  emit('toggle-select');
}

const imageSrc = computed(() => {
  // 优先使用缩略图，如果路径已计算且非空
  if (thumbnailPath.value) {
    return convertFileSrc(thumbnailPath.value);
  }

  const raw = item.value.url;
  // correct double-colon protocol typo
  return raw.replace('https:://', 'https://').replace('http:://', 'http://');
});
</script>

<template>
  <figure
    class="card"
    :title="displayName"
    role="button"
    tabindex="0"
    @click="handlePreview"
    @keydown.enter.prevent="handlePreview"
    @keydown.space.prevent="handlePreview"
  >
    <div class="image-wrapper">
      <img :src="imageSrc" :alt="displayName" loading="lazy" />
      <!-- selection badge (shown when parent enables batch selection) -->
      <div
        v-if="showSelection && selectedIndex !== null"
        class="select-badge"
        :class="{ dot: selectedIndex === -1 }"
        @click.stop="handleBadgeClick"
      >
        <span v-if="selectedIndex > 0">{{ selectedIndex }}</span>
        <span v-else class="dot-inner"></span>
      </div>
      <button
        v-if="!batchMode"
        type="button"
        class="icon-btn danger delete-btn"
        @click.stop="handleDelete"
        aria-label="删除"
        title="删除"
      >
        <Trash2 :size="18" />
      </button>
    </div>
    <figcaption v-if="!batchMode" class="card-overlay">
      <div class="overlay-content">
        <div class="meta">
          <span class="name" :title="displayName">{{ displayName }}</span>
          <span class="time" :title="localTimestamp">{{ localTimestamp }}</span>
        </div>
        <div class="actions">
          <button
            type="button"
            class="icon-btn"
            @click.stop="handleCopy"
            aria-label="复制链接"
            title="复制链接"
          >
            <Link2 :size="18" />
          </button>
        </div>
      </div>
    </figcaption>
  </figure>
</template>

<style scoped>
.card {
  margin: 0;
  background: var(--surface-acrylic);
  border-radius: 18px;
  overflow: hidden;
  box-shadow: var(--shadow-soft);
  transition: transform 0.2s ease, box-shadow 0.2s ease;
  display: flex;
  cursor: pointer;
  outline: none;
  position: relative;
}

.card:hover {
  transform: translateY(-4px);
  box-shadow: var(--shadow-strong);
}

.card:focus-visible {
  transform: translateY(-4px);
  box-shadow: var(--shadow-strong);
}

.image-wrapper {
  position: relative;
  width: 100%;
  padding-top: 70%;
  overflow: hidden;
}

.image-wrapper img {
  position: absolute;
  inset: 0;
  width: 100%;
  height: 100%;
  object-fit: cover;
  background: rgba(0, 0, 0, 0.1);
  pointer-events: none;
}

.delete-btn {
  position: absolute;
  top: 8px;
  right: 8px;
  z-index: 10;
  opacity: 0;
  transform: translateY(-4px);
  transition: opacity 0.2s ease, transform 0.2s ease;
}

.card:hover .delete-btn,
.card:focus-within .delete-btn {
  opacity: 1;
  transform: translateY(0);
}

.card-overlay {
  position: absolute;
  left: 0;
  right: 0;
  bottom: 0;
  padding: 14px 16px 16px;
  background: linear-gradient(180deg, rgba(6, 9, 18, 0), rgba(6, 9, 18, 0.78));
  opacity: 0;
  transform: translateY(10px);
  transition: opacity 0.2s ease, transform 0.2s ease;
  pointer-events: none;
}

.card:hover .card-overlay,
.card:focus-within .card-overlay {
  opacity: 1;
  transform: translateY(0);
}

.overlay-content {
  display: flex;
  align-items: flex-end;
  justify-content: space-between;
  gap: 12px;
  pointer-events: auto;
}

.meta {
  display: flex;
  flex-direction: column;
  gap: 4px;
  color: #fff;
  max-width: 70%;
}

.meta .name {
  font-size: 14px;
  font-weight: 600;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
  line-height: 1.2;
}

.meta .time {
  font-size: 12px;
  opacity: 0.85;
  white-space: nowrap;
}

.actions {
  display: flex;
  gap: 8px;
  flex-shrink: 0;
}

.icon-btn {
  pointer-events: auto;
  width: 34px;
  height: 34px;
  display: inline-flex;
  align-items: center;
  justify-content: center;
  border-radius: 50%;
  border: none;
  background: rgba(255, 255, 255, 0.22);
  color: #fff;
  cursor: pointer;
  transition: background 0.2s ease, transform 0.2s ease, color 0.2s ease;
}

.icon-btn:hover {
  background: rgba(255, 255, 255, 0.32);
  transform: translateY(-1px);
}

.icon-btn:active {
  transform: translateY(0);
}

.icon-btn.danger {
  background: rgba(244, 63, 94, 0.34);
  color: #ffe8ec;
}

.icon-btn.danger:hover {
  background: rgba(244, 63, 94, 0.42);
}

/* Selection badge for batch mode */
.select-badge {
  position: absolute;
  top: 10px;
  left: 10px;
  width: 30px;
  height: 30px;
  border-radius: 50%;
  display: inline-grid;
  place-items: center;
  font-weight: 700;
  font-size: 13px;
  color: #fff;
  background: var(--accent);
  border: 2px solid rgba(255, 255, 255, 0.08);
  box-shadow: 0 8px 20px rgba(2, 6, 23, 0.45);
}
.select-badge.dot {
  background: rgba(0, 0, 0, 0.45);
}
.select-badge .dot-inner {
  width: 8px;
  height: 8px;
  border-radius: 50%;
  background: rgba(255, 255, 255, 0.9);
}
</style>
