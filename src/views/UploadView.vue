<script setup lang="ts">
import { computed, ref, watch, reactive } from 'vue';
import { open } from '@tauri-apps/plugin-dialog';
import { invoke } from '@tauri-apps/api/core';
import {
  info as logInfo,
  error as logError,
  warn as logWarn,
} from '@tauri-apps/plugin-log';
import { useImageHostStore } from '../stores/imageHosts';
import { useSettingsStore } from '../stores/settings';
import type { LoadedPlugin } from '../plugins/registry';
import type { PluginUploadResult } from '../types/imageHostPlugin';
import { insertGalleryItem } from '../types/gallery';

type FormatKey = 'link' | 'html' | 'bbcode' | 'markdown';

interface UploadLine {
  id: number;
  filePath: string;
  url: string;
  deleteId: string;
}

interface SettingsState {
  values: Record<string, unknown>;
}

interface DialogFilter {
  name: string;
  extensions: string[];
}

type UploadSuccess = {
  index: number;
  originalPath: string;
  uploadFileName: string;
  result: PluginUploadResult;
};

type UploadFailure = {
  index: number;
  originalPath: string;
  uploadFileName: string;
  error: string;
};

type ProgressStage = 'idle' | 'compress' | 'upload' | 'save';

const progressStageLabels: Record<ProgressStage, string> = {
  idle: '待命',
  compress: '压缩中',
  upload: '上传中',
  save: '保存中',
};

const formatLabels: Record<FormatKey, string> = {
  link: '纯链接',
  html: 'HTML',
  bbcode: 'BBCode',
  markdown: 'Markdown',
};

const props = defineProps<{
  pluginId: string | null;
  onSelectPlugin?: (payload: { id: string; navigate?: boolean }) => void;
}>();

const store = useImageHostStore();
void store.ensureLoaded();
const globalSettings = useSettingsStore();

const plugins = store.plugins;
const loading = store.loading;
const ready = store.ready;
const errorRef = store.error;

const localPluginId = ref<string | null>(props.pluginId ?? null);
const uploading = ref(false);
const dragActive = ref(false);
const format = ref<FormatKey>('link');
const uploadLines = ref<UploadLine[]>([]);
const errorMessages = ref<string[]>([]);
const nextId = ref(1);

const progress = reactive({
  active: false,
  stage: 'idle' as ProgressStage,
  total: 0,
  completed: 0,
  detail: '',
});

let progressResetTimer: ReturnType<typeof setTimeout> | null = null;

const pluginList = computed(() => plugins.value as readonly LoadedPlugin[]);

const formatEntries = computed(
  () => Object.entries(formatLabels) as Array<[FormatKey, string]>
);

const activePlugin = computed<LoadedPlugin | null>(() => {
  const id = localPluginId.value;
  if (!id) return null;
  return pluginList.value.find((plugin) => plugin.id === id) ?? null;
});

const activeSettings = computed<SettingsState | null>(() => {
  const plugin = activePlugin.value;
  if (!plugin) return null;
  return (
    (store.getSettingsState(plugin.id) as SettingsState | undefined | null) ??
    null
  );
});

const formattedLines = computed(() =>
  uploadLines.value.map((line) => ({
    id: line.id,
    text: formatLine(line),
  }))
);

const progressPercent = computed(() => {
  if (!progress.total || progress.total <= 0) return 0;
  const ratio = progress.completed / progress.total;
  return Math.min(100, Math.max(0, Math.round(ratio * 100)));
});

const progressVisible = computed(() => progress.active || uploading.value);

const progressStageText = computed(
  () => progressStageLabels[progress.stage] ?? ''
);

const availableFilters = computed<DialogFilter[]>(() => {
  const plugin = activePlugin.value;
  const filters: DialogFilter[] = [];
  if (plugin?.supportedFileTypes?.length) {
    for (const type of plugin.supportedFileTypes) {
      const extensions = (type.extensions ?? []).filter(Boolean);
      if (!extensions.length) continue;
      filters.push({
        name: type.description ?? `类型 (${extensions.join(', ')})`,
        extensions: extensions.map((ext) => ext.replace(/^\./, '')),
      });
    }
  }
  if (!filters.length) {
    filters.push({
      name: 'Images',
      extensions: [
        'png',
        'jpg',
        'jpeg',
        'webp',
        'gif',
        'bmp',
        'tiff',
        'tif',
        'svg',
      ],
    });
  }
  return filters;
});

watch(
  () => props.pluginId,
  (value) => {
    if (value === localPluginId.value) return;
    localPluginId.value = value ?? null;
    resetState();
  }
);

watch(
  pluginList,
  (list) => {
    if (!list.length) {
      localPluginId.value = null;
      resetState();
      return;
    }
    const exists = localPluginId.value
      ? list.some((plugin) => plugin.id === localPluginId.value)
      : false;
    if (!localPluginId.value || !exists) {
      updateSelected(list[0]!.id);
    }
  },
  { immediate: true }
);

function resetState(options?: { keepResults?: boolean; keepFormat?: boolean }) {
  errorMessages.value = [];
  if (!options?.keepResults) {
    uploadLines.value = [];
  }
  if (!options?.keepFormat) {
    format.value = 'link';
  }
}

function updateSelected(id: string) {
  if (localPluginId.value === id) return;
  localPluginId.value = id;
  resetState();
  props.onSelectPlugin?.({ id, navigate: false });
}

function selectFormat(key: FormatKey) {
  format.value = key;
}

function extractName(path: string): string {
  const segments = path.split(/[/\\]/);
  return segments[segments.length - 1] || path;
}

function formatLine(line: UploadLine): string {
  const url = line.url;
  const name = extractName(line.filePath) || 'image';
  switch (format.value) {
    case 'html':
      return `<img src="${url}" alt="${name}" />`;
    case 'bbcode':
      return `[img]${url}[/img]`;
    case 'markdown':
      return `![${name}](${url})`;
    case 'link':
    default:
      return url;
  }
}

function uniquePaths(paths: Array<string | null | undefined>): string[] {
  const seen = new Set<string>();
  const result: string[] = [];
  for (const raw of paths) {
    const value = raw?.trim();
    if (!value || seen.has(value)) continue;
    seen.add(value);
    result.push(value);
  }
  return result;
}

function clampConcurrency(value: unknown): number {
  const fallback = 5;
  const max = 10;
  const min = 1;
  const parsed = Number(value);
  if (!Number.isFinite(parsed)) return fallback;
  const rounded = Math.round(parsed);
  if (rounded < min) return min;
  if (rounded > max) return max;
  return rounded;
}

function supportsWebp(plugin: LoadedPlugin): boolean {
  const types = plugin.supportedFileTypes;
  if (!types || !types.length) return true;
  for (const type of types) {
    const extensions = (type.extensions ?? []).map((ext) =>
      ext.replace(/^[.]/, '').toLowerCase()
    );
    if (extensions.includes('webp')) {
      return true;
    }
    const mimeTypes = (type.mimeTypes ?? []).map((mime) => mime.toLowerCase());
    if (mimeTypes.includes('image/webp')) {
      return true;
    }
  }
  return false;
}

function ensureWebpExtension(name: string): string {
  const dotIndex = name.lastIndexOf('.');
  if (dotIndex <= 0) {
    return `${name || 'image'}.webp`;
  }
  return `${name.slice(0, dotIndex)}.webp`;
}

function resolveFilesize(
  metadata: Record<string, unknown> | undefined
): number | undefined {
  if (!metadata) return undefined;
  const candidates = [metadata.filesize, metadata.size];
  for (const value of candidates) {
    if (typeof value === 'number' && Number.isFinite(value)) {
      return value;
    }
  }
  return undefined;
}

function canInteract(): boolean {
  return (
    !!activePlugin.value &&
    !!activeSettings.value &&
    ready.value &&
    globalSettings.ready.value
  );
}

async function selectFiles(event?: Event) {
  event?.stopPropagation();
  if (uploading.value) return;
  if (!ensurePluginReady()) return;

  try {
    const selection = await open({
      multiple: true,
      filters: availableFilters.value,
    });
    if (!selection) return;
    const paths = Array.isArray(selection) ? selection : [selection];
    await processPaths(paths);
  } catch (error) {
    const message =
      error instanceof Error ? error.message : String(error ?? '未知错误');
    errorMessages.value = [`选择文件失败：${message}`];
  }
}

function ensurePluginReady(): boolean {
  if (!activePlugin.value) {
    errorMessages.value = ['请先选择图床插件。'];
    return false;
  }
  if (!activeSettings.value) {
    errorMessages.value = ['插件配置尚未就绪，请稍候或检查设置。'];
    return false;
  }
  if (!ready.value) {
    errorMessages.value = ['插件仍在加载中，请稍候。'];
    return false;
  }
  if (!globalSettings.ready.value) {
    errorMessages.value = ['全局压缩设置仍在加载，请稍候。'];
    return false;
  }
  return true;
}

async function processPaths(rawPaths: Array<string | null | undefined>) {
  if (!ensurePluginReady()) return;
  const plugin = activePlugin.value!;
  const settings = activeSettings.value!;
  const paths = uniquePaths(rawPaths);
  if (!paths.length) return;

  const payloadTemplate = JSON.stringify(settings.values ?? {});

  resetState({ keepResults: true, keepFormat: true });
  uploading.value = true;
  const errors: string[] = [];

  const compressionEnabled = globalSettings.enableUploadCompression.value;
  const convertToWebp = globalSettings.convertToWebp.value;
  const targetSupportsWebp = supportsWebp(plugin);
  let useWebpMode = compressionEnabled && convertToWebp;

  if (useWebpMode && !targetSupportsWebp) {
    useWebpMode = false;
    await logWarn(
      `[upload] 插件 ${plugin.id} 未声明 WebP 支持，上传将回退为原格式压缩。`
    );
  }

  if (progressResetTimer) {
    clearTimeout(progressResetTimer);
    progressResetTimer = null;
  }

  const compressionSteps = compressionEnabled ? paths.length : 0;
  const uploadSteps = paths.length;
  const initialTotal = compressionEnabled
    ? compressionSteps + uploadSteps + paths.length
    : uploadSteps + paths.length;
  progress.active = true;
  progress.stage = compressionEnabled ? 'compress' : 'upload';
  progress.total = initialTotal;
  progress.completed = 0;
  progress.detail = compressionEnabled
    ? `准备压缩（共 ${paths.length} 张）…`
    : `准备上传（共 ${paths.length} 张）…`;

  try {
    const forceAnimated = useWebpMode && globalSettings.forceAnimatedWebp.value;
    const pngMode = globalSettings.pngCompressionMode.value;
    const pngOptimization = globalSettings.pngOptimization.value;
    const quality = globalSettings.quality.value;

    let processedPaths = paths;

    if (compressionEnabled) {
      try {
        progress.stage = 'compress';
        progress.detail = `正在压缩（${paths.length} 张）…`;
        const response = await invoke<string[]>('compress_images', {
          paths,
          quality,
          mode: useWebpMode ? 'webp' : 'original_format',
          forceAnimatedWebp: forceAnimated,
          pngMode,
          pngOptimization,
        });
        if (Array.isArray(response) && response.length === paths.length) {
          processedPaths = response;
        } else {
          await logWarn(
            `[upload] 压缩结果数量与输入不符（${response?.length ?? 0} != ${
              paths.length
            }），已回退原文件。`
          );
          processedPaths = paths;
          useWebpMode = false;
        }
      } catch (error) {
        const message =
          error instanceof Error ? error.message : String(error ?? '未知错误');
        await logError(`[upload] 压缩阶段失败，已回退原文件: ${message}`);
        errors.push(`压缩失败：${message}`);
        processedPaths = paths;
        useWebpMode = false;
      } finally {
        progress.completed = compressionSteps;
        progress.detail = '压缩阶段完成，准备上传…';
      }
    }

    const uploadEntries = paths.map((originalPath, index) => {
      const uploadPath = processedPaths[index] ?? originalPath;
      const baseName = extractName(originalPath) || `image-${index + 1}`;
      const shouldRenameToWebp = useWebpMode && uploadPath !== originalPath;
      const uploadFileName = shouldRenameToWebp
        ? ensureWebpExtension(baseName)
        : baseName;
      return {
        index,
        originalPath,
        uploadPath,
        uploadFileName,
      };
    });

    const results: Array<UploadSuccess | UploadFailure | undefined> = new Array(
      uploadEntries.length
    );
    const concurrency = clampConcurrency(
      globalSettings.maxConcurrentUploads.value
    );
    let uploadCompleted = 0;

    progress.stage = 'upload';
    progress.detail = `上传中 (0/${uploadEntries.length})`;

    let nextIndex = 0;
    const worker = async () => {
      while (true) {
        const current = nextIndex++;
        if (current >= uploadEntries.length) return;
        const entry = uploadEntries[current]!;
        const payload = JSON.parse(payloadTemplate) as Record<string, unknown>;
        try {
          await logInfo(
            `[upload] 使用插件 ${plugin.id} 上传文件 ${entry.uploadPath}`
          );
          const result = await plugin.upload(
            entry.uploadPath,
            entry.uploadFileName,
            payload,
            store.runtime
          );
          await logInfo(
            `[upload] 插件 ${plugin.id} 上传完成，访问链接 ${result.url}`
          );
          results[current] = {
            index: entry.index,
            originalPath: entry.originalPath,
            uploadFileName: entry.uploadFileName,
            result,
          } satisfies UploadSuccess;
        } catch (error) {
          const message =
            error instanceof Error
              ? error.message
              : String(error ?? '未知错误');
          await logError(
            `[upload] 插件 ${plugin.id} 上传 ${entry.uploadPath} 失败: ${message}`
          );
          results[current] = {
            index: entry.index,
            originalPath: entry.originalPath,
            uploadFileName: entry.uploadFileName,
            error: message,
          } satisfies UploadFailure;
        } finally {
          uploadCompleted += 1;
          progress.completed =
            compressionSteps + Math.min(uploadCompleted, uploadSteps);
          progress.detail = `上传中 (${uploadCompleted}/${uploadEntries.length})`;
        }
      }
    };

    const workerCount = Math.max(
      1,
      Math.min(concurrency, uploadEntries.length)
    );
    await Promise.all(Array.from({ length: workerCount }, () => worker()));

    const successes: UploadSuccess[] = [];
    for (const outcome of results) {
      if (!outcome) continue;
      if ('result' in outcome) {
        successes.push(outcome);
      } else {
        errors.push(`${outcome.uploadFileName}：${outcome.error}`);
      }
    }

    successes.sort((a, b) => a.index - b.index);
    for (const { originalPath, result } of successes) {
      uploadLines.value.push({
        id: nextId.value++,
        filePath: originalPath,
        url: result.url,
        deleteId: result.deleteId,
      });
    }

    const saveSteps = successes.length;
    progress.total = compressionSteps + uploadSteps + saveSteps;
    progress.completed = compressionSteps + uploadSteps;

    if (saveSteps > 0) {
      progress.stage = 'save';
      progress.detail = `保存到图库 (0/${saveSteps})`;
      let saved = 0;
      for (const success of successes) {
        try {
          await insertGalleryItem({
            file_name: success.uploadFileName,
            url: success.result.url,
            host: plugin.id,
            delete_marker: success.result.deleteId ?? null,
            filesize: resolveFilesize(success.result.metadata),
          });
        } catch (error) {
          const message =
            error instanceof Error
              ? error.message
              : String(error ?? '未知错误');
          await logError(
            `[upload] 保存至图库失败 (${success.uploadFileName}): ${message}`
          );
          errors.push(`${success.uploadFileName}：保存到图库失败：${message}`);
        } finally {
          saved += 1;
          progress.completed =
            compressionSteps + uploadSteps + Math.min(saved, saveSteps);
          progress.detail = `保存到图库 (${saved}/${saveSteps})`;
        }
      }
    }

    const summary = errors.length
      ? `已完成，成功 ${successes.length} / 失败 ${errors.length}`
      : '全部完成';
    progress.detail = summary;
    progress.completed = Math.max(progress.completed, progress.total);
    progress.total = Math.max(progress.total, progress.completed);
    progress.stage = saveSteps > 0 ? 'save' : progress.stage;

    progressResetTimer = setTimeout(() => {
      progress.active = false;
      progress.stage = 'idle';
      progress.total = 0;
      progress.completed = 0;
      progress.detail = '';
      progressResetTimer = null;
    }, 1600);
  } finally {
    uploading.value = false;
  }

  if (errors.length) {
    errorMessages.value = errors;
  } else {
    errorMessages.value = [];
  }
}

function onDragEnter(event: DragEvent) {
  if (!canInteract() || uploading.value) return;
  event.preventDefault();
  dragActive.value = true;
}

function onDragOver(event: DragEvent) {
  if (!canInteract() || uploading.value) {
    event.preventDefault();
    if (event.dataTransfer) event.dataTransfer.dropEffect = 'none';
    return;
  }
  event.preventDefault();
  if (event.dataTransfer) event.dataTransfer.dropEffect = 'copy';
}

function onDragLeave(event: DragEvent) {
  event.preventDefault();
  if (event.currentTarget === event.target) {
    dragActive.value = false;
  }
}

async function onDrop(event: DragEvent) {
  event.preventDefault();
  dragActive.value = false;
  if (!canInteract() || uploading.value) {
    if (!uploading.value) ensurePluginReady();
    return;
  }

  const files = Array.from(event.dataTransfer?.files ?? []);
  const paths = files.map(
    (file) => (file as File & { path?: string }).path ?? ''
  );
  await processPaths(paths);
}

async function copyLine(content: string) {
  try {
    await navigator.clipboard.writeText(content);
  } catch (error) {
    const message =
      error instanceof Error ? error.message : String(error ?? '未知错误');
    errorMessages.value = [`复制失败：${message}`];
  }
}

async function copyAll() {
  const joined = formattedLines.value.map((line) => line.text).join('\n');
  if (!joined) return;
  await copyLine(joined);
}

function clearResults() {
  uploadLines.value = [];
  errorMessages.value = [];
}
</script>

<template>
  <div class="upload-container">
    <section class="panel">
      <div v-if="!ready" class="status info">正在初始化插件，请稍候…</div>
      <div v-else-if="errorRef" class="status error">{{ errorRef }}</div>
      <div v-else-if="loading && !pluginList.length" class="status info">
        正在加载图床插件…
      </div>
      <div v-else-if="!pluginList.length" class="status muted">
        暂无可用的图床插件。
      </div>
      <template v-else>
        <div class="selector">
          <label for="plugin-select">图床插件</label>
          <select
            id="plugin-select"
            :value="localPluginId ?? ''"
            :disabled="uploading"
            @change="updateSelected(($event.target as HTMLSelectElement).value)"
          >
            <option value="" disabled>请选择一个插件</option>
            <option
              v-for="plugin in pluginList"
              :key="plugin.id"
              :value="plugin.id"
            >
              {{ plugin.name }}
            </option>
          </select>
        </div>

        <div
          class="dropzone"
          :class="{
            active: dragActive,
            disabled: uploading || !activePlugin,
          }"
          @dragenter.prevent="onDragEnter"
          @dragover.prevent="onDragOver"
          @dragleave.prevent="onDragLeave"
          @drop.prevent="onDrop"
          @click="selectFiles"
        >
          <div class="drop-content">
            <span class="drop-title">
              {{ uploading ? '正在上传…' : '拖拽图片到此，或点击选择文件' }}
            </span>
            <span class="drop-sub" v-if="activePlugin">
              当前：{{ activePlugin.name }}
            </span>
            <span class="drop-sub"> 支持批量上传，格式选项见下方 </span>
          </div>
        </div>

        <div class="actions">
          <button type="button" :disabled="uploading" @click.stop="selectFiles">
            {{ uploading ? '上传中…' : '从文件选择' }}
          </button>
          <button
            type="button"
            class="muted"
            :disabled="!uploadLines.length"
            @click="clearResults"
          >
            清空结果
          </button>
        </div>

        <div v-if="progressVisible" class="progress-card">
          <div class="progress-header">
            <span class="stage">{{ progressStageText }}</span>
            <span class="ratio"
              >{{ Math.min(progress.completed, progress.total) }} /
              {{ progress.total }}</span
            >
          </div>
          <div class="progress-bar">
            <div
              class="progress-bar__fill"
              :style="{ width: progressPercent + '%' }"
            ></div>
          </div>
          <div class="progress-detail">{{ progress.detail }}</div>
        </div>

        <div v-if="errorMessages.length" class="status error">
          <p v-for="(message, index) in errorMessages" :key="index">
            {{ message }}
          </p>
        </div>

        <div v-if="uploadLines.length" class="output">
          <div class="format-switcher">
            <div class="format-buttons">
              <button
                v-for="[key, label] in formatEntries"
                :key="key"
                type="button"
                :class="['format-button', { active: key === format }]"
                @click="selectFormat(key)"
              >
                {{ label }}
              </button>
            </div>
            <button type="button" class="copy-all" @click="copyAll">
              复制全部
            </button>
          </div>

          <div class="output-list">
            <div
              v-for="line in formattedLines"
              :key="line.id"
              class="output-line"
            >
              <code>{{ line.text }}</code>
              <button type="button" @click="copyLine(line.text)">复制</button>
            </div>
          </div>
        </div>
      </template>
    </section>
  </div>
</template>

<style scoped>
.upload-container {
  width: 100%;
  display: flex;
  flex-direction: column;
}

.panel {
  display: flex;
  flex-direction: column;
  gap: 18px;
  border-radius: 20px;
  padding: 28px;
  background: rgba(255, 255, 255, 0.9);
  border: 1px solid rgba(15, 27, 53, 0.08);
  box-shadow: 0 18px 36px rgba(12, 20, 48, 0.16);
  backdrop-filter: blur(16px);
  width: 100%; /* Ensure upload panel fills width */
}

.status {
  padding: 14px 16px;
  border-radius: 12px;
  font-size: 14px;
  line-height: 1.5;
}

.status.info {
  background: rgba(33, 100, 210, 0.1);
  color: #1f4ea0;
}

.status.error {
  background: rgba(178, 30, 53, 0.12);
  color: #9c1f33;
}

.status.muted {
  background: rgba(15, 27, 53, 0.08);
  color: rgba(15, 27, 53, 0.7);
}

.selector {
  display: flex;
  flex-direction: column;
  gap: 6px;
}

.selector label {
  font-size: 14px;
  font-weight: 600;
  color: #10203f;
}

.selector select {
  appearance: none;
  border-radius: 12px;
  border: 1px solid rgba(16, 31, 60, 0.18);
  padding: 10px 12px;
  font-size: 14px;
  background: rgba(255, 255, 255, 0.96);
  transition: border-color 0.15s ease, box-shadow 0.15s ease;
}

.selector select:focus {
  border-color: rgba(17, 33, 63, 0.45);
  box-shadow: 0 0 0 3px rgba(17, 33, 63, 0.12);
  outline: none;
}

.dropzone {
  position: relative;
  min-height: 180px;
  border: 2px dashed rgba(65, 82, 135, 0.32);
  border-radius: 18px;
  background: rgba(15, 27, 53, 0.05);
  display: flex;
  align-items: center;
  justify-content: center;
  transition: border-color 0.18s ease, background 0.18s ease, opacity 0.18s ease;
  cursor: pointer;
  text-align: center;
  padding: 24px;
}

.dropzone.disabled {
  opacity: 0.6;
  cursor: not-allowed;
}

.dropzone.active {
  border-color: rgba(99, 125, 255, 0.8);
  background: rgba(99, 125, 255, 0.12);
}

.drop-content {
  display: flex;
  flex-direction: column;
  gap: 8px;
  color: #0c1c38;
}

.drop-title {
  font-size: 18px;
  font-weight: 600;
}

.drop-sub {
  font-size: 13px;
  color: rgba(12, 28, 56, 0.66);
}

.actions {
  display: flex;
  align-items: center;
  gap: 12px;
}

.actions button {
  border: none;
  border-radius: 12px;
  padding: 10px 20px;
  font-size: 14px;
  font-weight: 600;
  cursor: pointer;
  transition: transform 0.15s ease, box-shadow 0.15s ease, opacity 0.15s ease;
}

.actions button:first-of-type {
  background: linear-gradient(135deg, #5a6bff, #9b46ff);
  color: #fff;
  box-shadow: 0 12px 26px rgba(90, 107, 255, 0.28);
}

.actions button:first-of-type:disabled {
  opacity: 0.6;
  cursor: not-allowed;
  box-shadow: none;
}

.actions button:first-of-type:not(:disabled):hover {
  transform: translateY(-1px);
  box-shadow: 0 16px 32px rgba(90, 107, 255, 0.34);
}

.actions button.muted {
  background: rgba(15, 27, 53, 0.08);
  color: rgba(15, 27, 53, 0.8);
}

.actions button.muted:disabled {
  opacity: 0.5;
  cursor: not-allowed;
}

.progress-card {
  display: flex;
  flex-direction: column;
  gap: 10px;
  margin-top: 8px;
  padding: 16px;
  border-radius: 14px;
  background: rgba(15, 27, 53, 0.06);
  border: 1px solid rgba(15, 27, 53, 0.1);
}

.progress-header {
  display: flex;
  justify-content: space-between;
  align-items: center;
  font-size: 13px;
  color: rgba(12, 28, 56, 0.7);
}

.progress-header .stage {
  font-weight: 600;
  color: #13244c;
}

.progress-header .ratio {
  font-family: 'Fira Code', 'Consolas', monospace;
}

.progress-bar {
  position: relative;
  height: 8px;
  border-radius: 999px;
  background: rgba(15, 27, 53, 0.12);
  overflow: hidden;
}

.progress-bar__fill {
  position: absolute;
  top: 0;
  left: 0;
  height: 100%;
  border-radius: inherit;
  background: linear-gradient(135deg, #5a6bff, #9b46ff);
  transition: width 0.2s ease;
}

.progress-detail {
  font-size: 12px;
  color: rgba(12, 28, 56, 0.7);
}

.output {
  display: flex;
  flex-direction: column;
  gap: 12px;
}

.format-switcher {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 12px;
  flex-wrap: wrap;
}

.format-buttons {
  display: flex;
  gap: 8px;
  flex-wrap: wrap;
}

.format-button {
  padding: 8px 16px;
  border-radius: 999px;
  border: 1px solid rgba(16, 31, 60, 0.2);
  background: rgba(255, 255, 255, 0.9);
  color: rgba(16, 31, 60, 0.7);
  font-size: 13px;
  font-weight: 600;
  cursor: pointer;
  transition: background 0.15s ease, border-color 0.15s ease, color 0.15s ease;
}

.format-button.active {
  border-color: rgba(90, 107, 255, 0.8);
  background: linear-gradient(135deg, #5a6bff, #9b46ff);
  color: #fff;
  box-shadow: 0 8px 18px rgba(90, 107, 255, 0.28);
}

.copy-all {
  padding: 8px 16px;
  border-radius: 10px;
  border: none;
  background: rgba(15, 27, 53, 0.12);
  color: rgba(15, 27, 53, 0.8);
  font-size: 13px;
  font-weight: 600;
  cursor: pointer;
}

.copy-all:hover {
  background: rgba(15, 27, 53, 0.18);
}

.output-list {
  display: flex;
  flex-direction: column;
  gap: 10px;
}

.output-line {
  display: grid;
  grid-template-columns: 1fr auto;
  align-items: center;
  gap: 12px;
  padding: 12px 14px;
  border-radius: 14px;
  background: rgba(15, 27, 53, 0.05);
  border: 1px solid rgba(15, 27, 53, 0.08);
}

.output-line code {
  white-space: pre-wrap;
  word-break: break-all;
  font-family: 'Fira Code', 'SFMono-Regular', Consolas, 'Liberation Mono', Menlo,
    monospace;
  font-size: 13px;
  color: #0c1c38;
}

.output-line button {
  border: none;
  border-radius: 10px;
  padding: 8px 16px;
  font-size: 12px;
  font-weight: 600;
  background: rgba(15, 27, 53, 0.12);
  color: rgba(15, 27, 53, 0.8);
  cursor: pointer;
  transition: background 0.15s ease, color 0.15s ease;
}

.output-line button:hover {
  background: rgba(15, 27, 53, 0.18);
  color: rgba(15, 27, 53, 0.9);
}

@media (max-width: 720px) {
  .panel {
    padding: 22px;
  }

  .dropzone {
    min-height: 160px;
  }

  .output-line {
    grid-template-columns: 1fr;
    gap: 8px;
  }

  .output-line button {
    justify-self: flex-start;
  }
}
</style>
