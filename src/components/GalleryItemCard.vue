<script setup lang="ts">
import { computed, toRefs } from 'vue';
import type { GalleryItem } from '../types/gallery';
import { Link2, Trash2 } from 'lucide-vue-next';

const props = defineProps<{
  item: GalleryItem;
}>();

const { item } = toRefs(props);

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
}>();

function handlePreview() {
  emit('preview', item.value);
}

function handleCopy() {
  emit('copy', item.value);
}

function handleDelete() {
  emit('delete', item.value);
}
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
      <img :src="item.url" :alt="displayName" loading="lazy" />
    </div>
    <figcaption class="card-overlay">
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
          <button
            type="button"
            class="icon-btn danger"
            @click.stop="handleDelete"
            aria-label="删除"
            title="删除"
          >
            <Trash2 :size="18" />
          </button>
        </div>
      </div>
    </figcaption>
  </figure>
</template>

<style scoped>
.card {
  margin: 0;
  background: rgba(255, 255, 255, 0.85);
  border-radius: 16px;
  overflow: hidden;
  box-shadow: 0 12px 30px rgba(15, 27, 53, 0.12);
  transition: transform 0.2s ease, box-shadow 0.2s ease;
  display: flex;
  cursor: pointer;
  outline: none;
  position: relative;
}

.card:hover {
  transform: translateY(-4px);
  box-shadow: 0 18px 38px rgba(15, 27, 53, 0.16);
}

.card:focus-visible {
  transform: translateY(-4px);
  box-shadow: 0 18px 38px rgba(26, 70, 160, 0.3);
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
  background: #f6f8fb;
  pointer-events: none;
}

.card-overlay {
  position: absolute;
  left: 0;
  right: 0;
  bottom: 0;
  padding: 14px 16px 16px;
  background: linear-gradient(
    180deg,
    rgba(12, 18, 36, 0),
    rgba(12, 18, 36, 0.88)
  );
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
  background: rgba(255, 255, 255, 0.18);
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
  background: rgba(244, 63, 94, 0.24);
  color: #fff5f7;
}

.icon-btn.danger:hover {
  background: rgba(244, 63, 94, 0.35);
}
</style>
