<script setup lang="ts">
import { ref, computed, onMounted, onBeforeUnmount } from 'vue';
import { Info, ZoomIn, ZoomOut, RotateCcw, X } from 'lucide-vue-next';
import type { GalleryItem } from '../types/gallery';

interface Props {
  item: GalleryItem | null;
  isOpen: boolean;
}

interface Emits {
  (e: 'close'): void;
}

const props = defineProps<Props>();
const emit = defineEmits<Emits>();

// 图片信息展示相关
const showInfo = ref(true);

// 缩放相关
const scale = ref(1);
const MIN_SCALE = 0.5;
const MAX_SCALE = 4;
const SCALE_STEP = 0.1;

// 拖动相关
const isDragging = ref(false);
let dragStartX = 0;
let dragStartY = 0;
const dragOffsetX = ref(0);
const dragOffsetY = ref(0);

// 缩放控制条显示相关
const showZoomControls = ref(false);
let zoomControlsHideTimeout: ReturnType<typeof setTimeout> | null = null;
let isZoomControlsFocused = false;
let mouseMoveTimeout: ReturnType<typeof setTimeout> | null = null;

// 图片元素引用
const imgRef = ref<HTMLImageElement | null>(null);
const overlayRef = ref<HTMLDivElement | null>(null);

// 格式化时间
function formatDate(timestamp: string | undefined): string {
  if (!timestamp) return '未知';
  try {
    const date = new Date(timestamp);
    return date.toLocaleString('zh-CN');
  } catch {
    return '未知';
  }
}

// 格式化文件大小
function formatSize(bytes: number | undefined | null): string {
  if (!bytes || bytes === 0) return '未知';
  const units = ['B', 'KB', 'MB', 'GB'];
  let size = bytes;
  let unitIndex = 0;
  while (size >= 1024 && unitIndex < units.length - 1) {
    size /= 1024;
    unitIndex++;
  }
  return `${size.toFixed(2)} ${units[unitIndex]}`;
}

// 缩放处理
function handleZoom(delta: number) {
  const newScale = scale.value + delta;
  if (newScale >= MIN_SCALE && newScale <= MAX_SCALE) {
    scale.value = newScale;
  }
}

// 鼠标滚轮缩放
function handleWheel(e: WheelEvent) {
  const delta = e.deltaY > 0 ? -SCALE_STEP : SCALE_STEP;
  handleZoom(delta);

  // 显示缩放控制条
  showZoomControls.value = true;
  if (mouseMoveTimeout) {
    clearTimeout(mouseMoveTimeout);
  }

  mouseMoveTimeout = setTimeout(() => {
    showZoomControls.value = false;
  }, 3000);
}

// 鼠标按下 - 开始拖动
function handleMouseDown(e: MouseEvent) {
  isDragging.value = true;
  dragStartX = e.clientX - dragOffsetX.value;
  dragStartY = e.clientY - dragOffsetY.value;
}

// 在预览对话框鼠标移动 - 拖动
function handleDialogMouseMove(e: MouseEvent) {
  if (!isDragging.value) return;
  dragOffsetX.value = e.clientX - dragStartX;
  dragOffsetY.value = e.clientY - dragStartY;

  // 显示缩放控制条
  showZoomControls.value = true;
  if (mouseMoveTimeout) {
    clearTimeout(mouseMoveTimeout);
  }

  mouseMoveTimeout = setTimeout(() => {
    showZoomControls.value = false;
  }, 3000);
}

// 鼠标释放 - 停止拖动
function handleMouseUp() {
  isDragging.value = false;
}

// 重置图片（缩放和位置）
function resetImage() {
  scale.value = 1;
  dragOffsetX.value = 0;
  dragOffsetY.value = 0;
}

// 处理缩放控制条鼠标进入
function handleZoomControlsMouseEnter() {
  if (zoomControlsHideTimeout) {
    clearTimeout(zoomControlsHideTimeout);
    zoomControlsHideTimeout = null;
  }
  showZoomControls.value = true;
  isZoomControlsFocused = true;
}

// 处理缩放控制条鼠标离开
function handleZoomControlsMouseLeave() {
  isZoomControlsFocused = false;
  zoomControlsHideTimeout = setTimeout(() => {
    showZoomControls.value = false;
  }, 300);
}

// 处理预览区域鼠标移动 - 显示控制条，不移动3秒后隐藏
function handlePreviewMouseMove() {
  // 仅在非拖动状态且非焦点在控制条上时显示
  if (isDragging.value) return;

  showZoomControls.value = true;

  if (mouseMoveTimeout) {
    clearTimeout(mouseMoveTimeout);
  }

  mouseMoveTimeout = setTimeout(() => {
    showZoomControls.value = false;
  }, 3000);
}

// 键盘快捷键 - 需要排除焦点在控制条或表单元素上的情况
function handleKeyDown(e: KeyboardEvent) {
  // 如果焦点在缩放控制条上，不处理快捷键
  if (isZoomControlsFocused) {
    return;
  }

  // 如果是在输入框或按钮上，不处理
  const target = e.target as HTMLElement;
  if (target.tagName === 'BUTTON' || target.tagName === 'INPUT') {
    return;
  }

  switch (e.key) {
    case 'Escape':
      closePreview();
      break;
    case '+':
    case '=':
      e.preventDefault();
      handleZoom(SCALE_STEP);
      break;
    case '-':
      e.preventDefault();
      handleZoom(-SCALE_STEP);
      break;
    case '0':
      e.preventDefault();
      resetImage();
      break;
  }
}
function closePreview() {
  resetImage();
  emit('close');
}

onMounted(() => {
  // 全局监听键盘事件
  document.addEventListener('keydown', handleKeyDown);
});

onBeforeUnmount(() => {
  // 移除全局键盘事件
  document.removeEventListener('keydown', handleKeyDown);
  // 清理定时器
  if (zoomControlsHideTimeout) {
    clearTimeout(zoomControlsHideTimeout);
  }
  if (mouseMoveTimeout) {
    clearTimeout(mouseMoveTimeout);
  }
  isZoomControlsFocused = false;
});

// 计算图片容器的样式
const imageContainerStyle = computed(() => ({
  transform: `translate(${dragOffsetX.value}px, ${dragOffsetY.value}px) scale(${scale.value})`,
  cursor: isDragging.value ? 'grabbing' : 'grab',
}));

// 图片信息
const imageInfo = computed(() => {
  if (!props.item) return null;
  return {
    fileName: props.item.file_name || '未知',
    url: props.item.url,
    size: formatSize(props.item.filesize ?? undefined),
    uploadTime: formatDate(props.item.inserted_at),
    imageHost: props.item.host || '未知',
  };
});
</script>

<template>
  <teleport to="body">
    <transition name="preview-fade">
      <div
        ref="overlayRef"
        v-if="isOpen && item"
        class="preview-overlay"
        @click.self="closePreview"
        @mousemove="handlePreviewMouseMove"
        @mouseup="handleMouseUp"
      >
        <!-- 图片详细信息面板（左上角，可折叠） -->
        <div class="info-panel" :class="{ collapsed: !showInfo }">
          <button
            type="button"
            class="info-toggle"
            :title="showInfo ? '隐藏信息' : '显示信息'"
            @click="showInfo = !showInfo"
          >
            <Info :size="20" />
          </button>
          <div v-if="showInfo" class="info-content">
            <div class="info-item">
              <span class="label">文件名</span>
              <span class="value">{{ imageInfo?.fileName }}</span>
            </div>
            <div class="info-item">
              <span class="label">文件大小</span>
              <span class="value">{{ imageInfo?.size }}</span>
            </div>
            <div class="info-item">
              <span class="label">上传时间</span>
              <span class="value">{{ imageInfo?.uploadTime }}</span>
            </div>
            <div class="info-item">
              <span class="label">图床</span>
              <span class="value">{{ imageInfo?.imageHost }}</span>
            </div>
          </div>
        </div>

        <!-- 预览对话框 -->
        <div
          class="preview-dialog"
          @mousedown="handleMouseDown"
          @mousemove="handleDialogMouseMove"
          @wheel.prevent="handleWheel"
        >
          <div class="image-container" :style="imageContainerStyle">
            <img
              ref="imgRef"
              :src="item.url"
              :alt="item.file_name || item.url"
              class="preview-image"
              draggable="false"
              @dragstart.prevent
            />
          </div>

          <!-- 缩放控制条（右下角） -->
          <div
            class="zoom-controls"
            :class="{ hidden: !showZoomControls }"
            @mouseenter="handleZoomControlsMouseEnter"
            @mouseleave="handleZoomControlsMouseLeave"
          >
            <button
              type="button"
              class="zoom-btn"
              :disabled="scale <= MIN_SCALE"
              title="缩小 (−)"
              @click="handleZoom(-SCALE_STEP)"
            >
              <ZoomOut :size="18" />
            </button>
            <span class="zoom-value">{{ (scale * 100).toFixed(0) }}%</span>
            <button
              type="button"
              class="zoom-btn"
              :disabled="scale >= MAX_SCALE"
              title="放大 (+)"
              @click="handleZoom(SCALE_STEP)"
            >
              <ZoomIn :size="18" />
            </button>
            <button
              type="button"
              class="zoom-btn reset"
              title="重置 (0)"
              @click="resetImage"
            >
              <RotateCcw :size="18" />
            </button>
          </div>
        </div>

        <!-- 关闭按钮 -->
        <button
          type="button"
          class="preview-close"
          title="关闭 (Esc)"
          @click="closePreview"
        >
          <X :size="24" />
        </button>
      </div>
    </transition>
  </teleport>
</template>

<style scoped>
.preview-overlay {
  position: fixed;
  inset: 0;
  background: rgba(5, 8, 18, 0.65);
  backdrop-filter: blur(4px);
  display: flex;
  justify-content: center;
  align-items: center;
  padding: 40px;
  z-index: 1200;
  overflow: hidden;
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
  align-items: center;
  justify-content: center;
  position: relative;
}

.image-container {
  display: flex;
  align-items: center;
  justify-content: center;
  width: 100%;
  height: 100%;
  transform-origin: center;
  transition: transform 0.05s ease-out;
}

.preview-image {
  display: block;
  max-width: 100%;
  max-height: 100%;
  object-fit: contain;
  background: var(--surface-acrylic-strong);
  user-select: none;
  pointer-events: none;
  -webkit-user-drag: none;
  -webkit-touch-callout: none;
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
  z-index: 1201;
}

.preview-close:hover {
  color: var(--accent);
  transform: translateY(-1px);
}

/* 图片信息面板 */
.info-panel {
  position: fixed;
  top: 32px;
  left: 32px;
  background: var(--surface-panel);
  border: 1px solid var(--surface-border);
  border-radius: 12px;
  box-shadow: 0 12px 30px rgba(6, 10, 22, 0.35);
  z-index: 1201;
  transition: all 0.3s ease;
  max-width: 280px;
}

.info-panel.collapsed {
  width: 44px;
  height: 44px;
  border-radius: 50%;
  overflow: hidden;
  display: flex;
  align-items: center;
  justify-content: center;
}

.info-toggle {
  width: 100%;
  padding: 12px;
  border: none;
  background: transparent;
  color: var(--accent);
  font-size: 16px;
  cursor: pointer;
  display: flex;
  align-items: center;
  justify-content: center;
  transition: transform 0.2s ease, color 0.2s ease;
  flex-shrink: 0;
}

.info-panel.collapsed .info-toggle {
  padding: 12px;
  width: 44px;
  height: 44px;
}

.info-toggle:hover {
  color: var(--text-primary);
  transform: scale(1.1);
}

.info-toggle .icon {
  display: inline-block;
  font-size: 18px;
  font-weight: bold;
}

.info-content {
  border-top: 1px solid var(--surface-border);
  padding: 12px;
  display: flex;
  flex-direction: column;
  gap: 10px;
}

.info-item {
  display: flex;
  flex-direction: column;
  gap: 4px;
  font-size: 12px;
}

.info-item .label {
  color: var(--text-secondary);
  font-weight: 600;
  text-transform: uppercase;
  letter-spacing: 0.5px;
}

.info-item .value {
  color: var(--text-primary);
  font-size: 13px;
  word-break: break-all;
  max-height: 60px;
  overflow-y: auto;
  font-family: 'Courier New', monospace;
}

/* 缩放控制条 */
.zoom-controls {
  position: absolute;
  bottom: 20px;
  right: 20px;
  display: flex;
  align-items: center;
  gap: 8px;
  background: var(--surface-panel);
  backdrop-filter: blur(8px);
  padding: 8px 12px;
  border-radius: 10px;
  border: 1px solid var(--surface-border);
  transition: opacity 0.3s ease;
  opacity: 1;
}

.zoom-controls.hidden {
  opacity: 0;
  pointer-events: none;
}

.zoom-btn {
  width: 32px;
  height: 32px;
  border-radius: 6px;
  border: 1px solid var(--surface-border);
  background: var(--surface-acrylic);
  color: var(--text-primary);
  font-size: 16px;
  font-weight: 600;
  cursor: pointer;
  display: flex;
  align-items: center;
  justify-content: center;
  transition: all 0.15s ease;
}

.zoom-btn:hover:not(:disabled) {
  background: var(--accent);
  color: white;
  transform: translateY(-1px);
  box-shadow: 0 4px 12px rgba(122, 163, 255, 0.3);
}

.zoom-btn:disabled {
  opacity: 0.5;
  cursor: not-allowed;
}

.zoom-btn.reset {
  font-size: 14px;
}

.zoom-value {
  min-width: 50px;
  text-align: center;
  color: var(--text-secondary);
  font-size: 12px;
  font-weight: 600;
}

/* 过渡动画 */
.preview-fade-enter-active,
.preview-fade-leave-active {
  transition: opacity 0.2s ease;
}

.preview-fade-enter-from,
.preview-fade-leave-to {
  opacity: 0;
}
</style>
