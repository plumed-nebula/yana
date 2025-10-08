<script setup lang="ts">
import { computed, ref, watch } from 'vue';
import { open } from '@tauri-apps/plugin-dialog';
import { info as logInfo, error as logError } from '@tauri-apps/plugin-log';
import { useImageHostStore } from '../stores/imageHosts';
import type { LoadedPlugin } from '../plugins/registry';

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

function canInteract(): boolean {
  return !!activePlugin.value && !!activeSettings.value && ready.value;
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
  return true;
}

async function processPaths(rawPaths: Array<string | null | undefined>) {
  if (!ensurePluginReady()) return;
  const plugin = activePlugin.value!;
  const settings = activeSettings.value!;
  const paths = uniquePaths(rawPaths);
  if (!paths.length) return;

  const payload = JSON.parse(JSON.stringify(settings.values ?? {})) as Record<
    string,
    unknown
  >;

  resetState({ keepResults: true, keepFormat: true });
  uploading.value = true;
  const errors: string[] = [];

  try {
    for (const path of paths) {
      try {
        await logInfo(`[upload] 使用插件 ${plugin.id} 上传文件 ${path}`);
        const result = await plugin.upload(path, payload, store.runtime);
        uploadLines.value.push({
          id: nextId.value++,
          filePath: path,
          url: result.url,
          deleteId: result.deleteId,
        });
        await logInfo(
          `[upload] 插件 ${plugin.id} 上传完成，访问链接 ${result.url}`
        );
      } catch (error) {
        const message =
          error instanceof Error ? error.message : String(error ?? '未知错误');
        const displayName = extractName(path);
        errors.push(`${displayName}：${message}`);
        await logError(
          `[upload] 插件 ${plugin.id} 上传 ${path} 失败: ${message}`
        );
      }
    }
  } finally {
    uploading.value = false;
  }

  if (errors.length) {
    errorMessages.value = errors;
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
  width: min(780px, 100%);
  margin: 0 auto;
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
