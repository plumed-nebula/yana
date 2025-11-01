<script setup lang="ts">
import { computed, ref, watch, reactive, onMounted, onUnmounted } from 'vue';
import { open } from '@tauri-apps/plugin-dialog';
import { invoke } from '@tauri-apps/api/core';
import { extname } from '@tauri-apps/api/path';
import {
  debug as logDebug,
  info as logInfo,
  error as logError,
  warn as logWarn,
} from '@tauri-apps/plugin-log';
import { listen } from '@tauri-apps/api/event';
import { useImageHostStore } from '../stores/imageHosts';
import { useSettingsStore } from '../stores/settings';
import { useDeviceStore } from '../stores/device';
import type { LoadedPlugin } from '../plugins/registry';
import { arePluginEntriesLoaded } from '../plugins/registry';
import type { PluginUploadResult } from '../types/imageHostPlugin';
import { insertGalleryItem } from '../types/gallery';
import { ClipboardCopy } from 'lucide-vue-next';
import GlobalSelect from '../components/GlobalSelect.vue';
import { retryAsync } from '../utils/retry';
import { writeText, readImage } from '../utils/clipboard';

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
const device = useDeviceStore();

const plugins = store.plugins;
const loading = store.loading;
const ready = store.ready;
const errorRef = store.error;

const LOCALSTORAGE_KEY_PLUGIN = 'yana.upload.lastPluginId';
const LOCALSTORAGE_KEY_FORMAT = 'yana.upload.lastFormat';

const localPluginId = ref<string | null>(props.pluginId ?? null);
const uploading = ref(false);
let initialFormat: FormatKey = 'link';
try {
  const raw = localStorage.getItem(LOCALSTORAGE_KEY_FORMAT);
  if (
    raw === 'link' ||
    raw === 'html' ||
    raw === 'bbcode' ||
    raw === 'markdown'
  ) {
    initialFormat = raw as FormatKey;
  }
} catch (e) {
  // ignore
}
const format = ref<FormatKey>(initialFormat);
const uploadLines = ref<UploadLine[]>([]);
const errorMessages = ref<string[]>([]);
const nextId = ref(1);
const dragActive = ref(false);

const progress = reactive({
  active: false,
  stage: 'idle' as ProgressStage,
  total: 0,
  completed: 0,
  detail: '',
});

let progressResetTimer: ReturnType<typeof setTimeout> | null = null;

const pluginList = computed(() => plugins.value as readonly LoadedPlugin[]);
const pluginOptions = computed(() =>
  pluginList.value.map((p) => ({ value: p.id, label: p.name }))
);

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
      // choose first plugin and persist
      const firstId = list[0]!.id;
      localPluginId.value = firstId;
      try {
        localStorage.setItem(LOCALSTORAGE_KEY_PLUGIN, firstId);
      } catch (e) {
        /* ignore */
      }
      updateSelected(firstId);
    }
  },
  { immediate: true }
);

let unlistenDrop: (() => void) | null = null;
let unlistenEnter: (() => void) | null = null;
let unlistenLeave: (() => void) | null = null;
let unlistenHostsReady: (() => void) | null = null;

onMounted(async () => {
  // Load persisted plugin selection if available
  try {
    const saved = localStorage.getItem(LOCALSTORAGE_KEY_PLUGIN);
    if (saved) {
      // Defer applying until plugins list is available; watcher on pluginList will handle fallback
      localPluginId.value = saved;
      try {
        await logDebug?.('[upload] 恢复上次选中的图床: ' + saved);
      } catch (e) {
        /* ignore logging failure */
      }
    } else {
      try {
        await logDebug?.('[upload] 未找到已保存的图床选择');
      } catch (e) {}
    }
  } catch (e) {
    try {
      await logDebug?.('[upload] 读取本地存储图床选择失败: ' + String(e));
    } catch (ee) {}
  }

  unlistenDrop = await listen<{
    paths: string[];
    position: { x: number; y: number };
  }>('tauri://drag-drop', async (event) => {
    await logInfo('[upload] 收到文件拖放事件');
    dragActive.value = false;
    if (uploading.value) {
      await logWarn('[upload] 正在上传中，已忽略此次文件拖放。');
      return;
    }
    if (!ensurePluginReady()) {
      return;
    }
    await processPaths(event.payload.paths);
  });

  unlistenEnter = await listen('tauri://drag-enter', async () => {
    await logInfo('[upload] 文件进入拖放区域');
    dragActive.value = true;
  });

  unlistenLeave = await listen('tauri://drag-leave', async () => {
    await logInfo('[upload] 文件离开拖放区域');
    dragActive.value = false;
  });

  // 如果插件列表在页面加载后才完成，监听 registry 发出的 ready 事件并在收到时重新应用选择
  try {
    const handler = () => {
      try {
        logDebug?.(
          '[upload] 收到 imageHosts:ready 事件，尝试从 localStorage 应用已保存选择'
        );

        // 首选从 localStorage 读取保存的选择，不再通过 updateSelected 写回（避免重复写入/覆盖）
        let saved: string | null = null;
        try {
          saved = localStorage.getItem(LOCALSTORAGE_KEY_PLUGIN);
        } catch (e) {
          /* ignore */
        }

        if (saved) {
          const found = pluginList.value.some((p) => p.id === saved);
          if (found) {
            try {
              logDebug?.('[upload] 从 localStorage 应用已保存图床: ' + saved);
            } catch (e) {}
            // 应用但不再次持久化（已来自 localStorage）
            localPluginId.value = saved;
            props.onSelectPlugin?.({ id: saved, navigate: false });
            return;
          } else {
            try {
              logDebug?.('[upload] 本地保存的图床未在可用插件中找到: ' + saved);
            } catch (e) {}
          }
        }

        // 若无法从 localStorage 恢复（无保存或不可用），再尝试使用当前 localPluginId
        if (localPluginId.value) {
          const exists = pluginList.value.some(
            (p) => p.id === localPluginId.value
          );
          if (exists) {
            try {
              logDebug?.(
                '[upload] 使用当前内存中的选中值: ' + localPluginId.value
              );
            } catch (e) {}
            props.onSelectPlugin?.({
              id: localPluginId.value,
              navigate: false,
            });
            return;
          }
        }

        // 最后回退：选择第一个并持久化（保留原有行为）
        if (pluginList.value.length) {
          const firstId = pluginList.value[0]!.id;
          try {
            logDebug?.('[upload] 回退到首个可用图床并持久化: ' + firstId);
          } catch (e) {}
          updateSelected(firstId);
        }
      } catch (e) {
        /* ignore */
      }
    };
    window.addEventListener('imageHosts:ready', handler);
    unlistenHostsReady = () =>
      window.removeEventListener('imageHosts:ready', handler);
    try {
      await logDebug?.('[upload] 添加 imageHosts:ready 事件监听器');
    } catch (e) {
      /* ignore */
    }
    // If entries were already loaded before we registered listener, run handler immediately
    try {
      if (arePluginEntriesLoaded()) {
        try {
          logDebug?.(
            '[upload] 插件条目已加载（事件可能已派发），立即执行一次重应用路径'
          );
        } catch (e) {
          /* ignore */
        }
        handler();
      }
    } catch (e) {
      /* ignore */
    }
  } catch (e) {
    /* ignore */
  }
});

onUnmounted(() => {
  if (unlistenDrop) {
    unlistenDrop();
    unlistenDrop = null;
  }
  if (unlistenEnter) {
    unlistenEnter();
    unlistenEnter = null;
  }
  if (unlistenLeave) {
    unlistenLeave();
    unlistenLeave = null;
  }
  if (unlistenHostsReady) {
    unlistenHostsReady();
    unlistenHostsReady = null;
    try {
      logDebug?.('[upload] 移除 imageHosts:ready 事件监听器');
    } catch (e) {
      /* ignore */
    }
  }
});

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
  const previous = localPluginId.value;
  localPluginId.value = id;
  try {
    localStorage.setItem(LOCALSTORAGE_KEY_PLUGIN, id);
    try {
      logDebug?.(
        '[upload] 保存选中图床: ' + id + ' (之前: ' + String(previous) + ')'
      );
    } catch (ee) {
      /* ignore logging errors */
    }
  } catch (e) {
    try {
      logDebug?.('[upload] 保存选中图床失败: ' + String(e));
    } catch (ee) {}
  }
  // If actually changed, reset and notify parent
  if (previous !== id) {
    resetState();
    props.onSelectPlugin?.({ id, navigate: false });
  }
}

function selectFormat(key: FormatKey) {
  const prev = format.value;
  format.value = key;
  try {
    localStorage.setItem(LOCALSTORAGE_KEY_FORMAT, key);
    try {
      logDebug?.('[upload] 保存链接格式: ' + key + ' (之前: ' + prev + ')');
    } catch (ee) {}
  } catch (e) {
    try {
      logDebug?.('[upload] 保存链接格式失败: ' + String(e));
    } catch (ee) {}
  }
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
    let paths: string[];

    // 在 Android 上使用新的文件选择器
    if (device.currentPlatform === 'android') {
      const result = await invoke<string[]>('select_multiple_images');
      if (!result || result.length === 0) return;
      paths = result;
    } else {
      // 其他平台使用原有逻辑
      const selection = await open({
        multiple: true,
        filters: availableFilters.value,
      });
      if (!selection) return;
      paths = Array.isArray(selection) ? selection : [selection];
    }

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

  const resolvedPaths = paths;

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

  const compressionSteps = compressionEnabled ? resolvedPaths.length : 0;
  const uploadSteps = resolvedPaths.length;
  const initialTotal = compressionEnabled
    ? compressionSteps + uploadSteps + resolvedPaths.length
    : uploadSteps + resolvedPaths.length;
  progress.active = true;
  progress.stage = compressionEnabled ? 'compress' : 'upload';
  progress.total = initialTotal;
  progress.completed = 0;
  progress.detail = compressionEnabled
    ? `准备压缩（共 ${resolvedPaths.length} 张）…`
    : `准备上传（共 ${resolvedPaths.length} 张）…`;

  try {
    const pngMode = globalSettings.pngCompressionMode.value;
    const pngOptimization = globalSettings.pngOptimization.value;
    const quality = globalSettings.quality.value;

    let processedPaths = resolvedPaths;
    let compressedFileSizes: number[] = [];

    if (compressionEnabled) {
      try {
        progress.stage = 'compress';
        progress.detail = `正在压缩（${resolvedPaths.length} 张）…`;
        const response = await invoke<string[]>('compress_images', {
          paths: resolvedPaths,
          quality,
          mode: useWebpMode ? 'webp' : 'original_format',
          pngMode,
          pngOptimization,
        });
        if (
          Array.isArray(response) &&
          response.length === resolvedPaths.length
        ) {
          processedPaths = response;
        } else {
          await logWarn(
            `[upload] 压缩结果数量与输入不符（${response?.length ?? 0} != ${
              resolvedPaths.length
            }），已回退原文件。`
          );
          processedPaths = resolvedPaths;
          useWebpMode = false;
        }
      } catch (error) {
        const message =
          error instanceof Error ? error.message : String(error ?? '未知错误');
        await logError(`[upload] 压缩阶段失败，已回退原文件: ${message}`);
        errors.push(`压缩失败：${message}`);
        processedPaths = resolvedPaths;
        useWebpMode = false;
      } finally {
        progress.completed = compressionSteps;
        progress.detail = '压缩阶段完成，准备上传…';
      }

      // 获取压缩后文件的大小
      try {
        const fileSizes = await invoke<number[]>('get_file_sizes', {
          paths: processedPaths,
        });
        compressedFileSizes = fileSizes;
        await logDebug(`[upload] 获取文件大小: ${JSON.stringify(fileSizes)}`);
      } catch (error) {
        const message =
          error instanceof Error ? error.message : String(error ?? '未知错误');
        await logWarn(`[upload] 获取文件大小失败: ${message}`);
      }
    }

    const uploadEntries = await Promise.all(
      paths.map(async (originalPath, index) => {
        const uploadPath = processedPaths[index] ?? originalPath;
        const baseName = extractName(originalPath) || `image-${index + 1}`;

        // 从后端返回的压缩文件中提取扩展名
        let uploadFileName = baseName;
        if (uploadPath !== originalPath) {
          // 文件被压缩过，从压缩后的文件路径获取扩展名
          const compressedExt = await extname(uploadPath);
          if (compressedExt) {
            // 规范化扩展名（确保带点号）
            const normalizedExt = compressedExt.startsWith('.')
              ? compressedExt
              : `.${compressedExt}`;
            // 移除原文件名的扩展名，添加压缩后的扩展名
            const dotIndex = baseName.lastIndexOf('.');
            const stem = dotIndex > 0 ? baseName.slice(0, dotIndex) : baseName;
            uploadFileName = `${stem}${normalizedExt}`;
          }
        }

        return {
          index,
          originalPath,
          uploadPath,
          uploadFileName,
        };
      })
    );

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
          const result = await retryAsync(
            async () => {
              return await plugin.upload(
                entry.uploadPath,
                entry.uploadFileName,
                payload,
                store.runtime
              );
            },
            { maxRetries: 1 }
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
          // 使用压缩后的文件大小，如果没有则使用上传结果中的大小
          const filesizeIndex = successes.indexOf(success);
          const filesize =
            compressedFileSizes[filesizeIndex] ??
            resolveFilesize(success.result.metadata);

          await insertGalleryItem({
            file_name: success.uploadFileName,
            url: success.result.url,
            host: plugin.id,
            delete_marker: success.result.deleteId ?? null,
            filesize,
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

      // 上传完成后，在后台批量生成缩略图（后台任务，切出页面后仍会继续）
      if (globalSettings.enableThumbnailCache.value && successes.length > 0) {
        // 构建 (url, local_path) 元组，使用本地文件直接生成缩略图，避免再次下载
        const thumbnailItems = successes
          .map((s) => {
            // 从 uploadEntries 中找到对应的本地文件路径
            const uploadEntry = uploadEntries.find((e) => e.index === s.index);
            if (uploadEntry) {
              return [s.result.url, uploadEntry.uploadPath] as [string, string];
            }
            return null;
          })
          .filter(Boolean) as Array<[string, string]>;

        if (thumbnailItems.length > 0) {
          // 在后台生成缩略图，不等待，用户切走也会继续执行
          void generateThumbnailsInBackground(thumbnailItems);
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

async function copyLine(content: string) {
  try {
    await writeText(content);
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

// eslint-disable-next-line @typescript-eslint/no-unused-vars
// 从剪贴板读取图片并上传（优先使用 Tauri 插件/API，回退到浏览器）
async function uploadClipboard() {
  if (!canInteract() || uploading.value) {
    if (!uploading.value) ensurePluginReady();
    return;
  }
  // 读取剪贴板（使用我们的 readImage 工具）
  let blob: Blob | null = null;
  try {
    blob = await readImage();
  } catch (e) {
    errorMessages.value = [
      '读取剪贴板失败：' + (e instanceof Error ? e.message : String(e)),
    ];
    return;
  }
  if (!blob) {
    errorMessages.value = ['剪贴板中没有图片'];
    return;
  }
  // 转为 Uint8Array
  const buffer = await blob.arrayBuffer();
  const data = new Uint8Array(buffer);
  // 调用后端接口，仅保存原始数据，后续由 processPaths 处理压缩
  let tempPath: string;
  try {
    tempPath = await invoke<string>('save_image_data', {
      data: Array.from(data),
    });
  } catch (e) {
    errorMessages.value = [
      '处理图片失败：' + (e instanceof Error ? e.message : String(e)),
    ];
    return;
  }
  // 继续执行上传流程
  await processPaths([tempPath]);
}

// 后台生成缩略图（不等待，切出页面后仍会继续执行）
// 接受 (url, localPath) 元组数组，直接使用本地文件生成，无需再次下载
async function generateThumbnailsInBackground(
  items: Array<[string, string]>
): Promise<void> {
  try {
    await invoke<string[]>('generate_thumbnails_from_local', { items });
  } catch (err: any) {
    void logWarn(`[upload] 缩略图生成失败: ${String(err)}`);
  }
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
          <GlobalSelect
            id="plugin-select"
            v-model="localPluginId"
            :options="pluginOptions"
            :disabled="uploading"
            @update:modelValue="updateSelected"
          />
        </div>

        <div
          class="dropzone"
          :class="{
            active: dragActive,
            disabled: uploading || !activePlugin,
          }"
          @click="selectFiles"
        >
          <div class="drop-content">
            <span class="drop-title">
              {{ uploading ? '正在上传…' : '选择图片文件' }}
            </span>
            <span class="drop-sub" v-if="activePlugin">
              当前：{{ activePlugin.name }}
            </span>
            <span v-if="device.currentPlatform !== 'android'" class="drop-sub">
              拖拽文件到窗口或点击以选择
            </span>
            <span v-else class="drop-sub"> 点击以选择文件，支持批量上传 </span>
          </div>
        </div>

        <div class="actions">
          <!-- 剪贴板上传按钮：仅在非 Android 平台显示 -->
          <button
            v-if="device.currentPlatform !== 'android'"
            type="button"
            class="primary"
            :disabled="uploading"
            @click.stop="uploadClipboard"
            title="从剪贴板上传图片"
          >
            <ClipboardCopy class="button-icon" :size="18" :stroke-width="1.6" />
            <span style="vertical-align: middle">{{
              uploading ? '上传中…' : '从剪贴板上传'
            }}</span>
          </button>
          <!-- Android 平台提示 -->
          <!-- <span v-else class="android-hint">
            💡 Android 上可从剪贴板选择的应用中长按图片并"粘贴"
          </span> -->
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
  gap: 24px;
  color: var(--text-primary);
}

.panel {
  display: flex;
  flex-direction: column;
  gap: 18px;
  border-radius: 22px;
  padding: 28px;
  background: var(--surface-panel);
  border: 1px solid var(--surface-border);
  box-shadow: var(--shadow-strong);
  backdrop-filter: blur(18px) saturate(1.08);
  width: 100%;
  transition: border-color 0.2s ease, box-shadow 0.2s ease;
}

.panel:hover {
  border-color: var(--surface-border);
  border-color: color-mix(in srgb, var(--surface-border) 60%, var(--accent));
  box-shadow: 0 26px 50px rgba(8, 14, 28, 0.28);
}

.status {
  padding: 14px 16px;
  border-radius: 12px;
  font-size: 14px;
  line-height: 1.5;
  background: var(--surface-acrylic);
  border: 1px solid var(--surface-border);
  color: var(--text-secondary);
}

.status.info {
  background: var(--accent-soft);
  background: color-mix(in srgb, var(--accent-soft) 80%, transparent);
  color: var(--accent);
  border-color: color-mix(in srgb, var(--accent) 32%, transparent);
}

.status.error {
  background: var(--danger-soft);
  background: color-mix(in srgb, var(--danger-soft) 80%, transparent);
  color: var(--danger);
  border-color: color-mix(in srgb, var(--danger) 28%, transparent);
}

.status.muted {
  color: var(--text-secondary);
}

.selector {
  display: flex;
  flex-direction: column;
  gap: 6px;
}

.selector label {
  font-size: 14px;
  font-weight: 600;
  color: var(--text-secondary);
}

.selector select {
  appearance: none;
  border-radius: 12px;
  border: 1px solid var(--surface-border);
  padding: 10px 12px;
  font-size: 14px;
  background: var(--surface-acrylic);
  color: var(--text-primary);
  transition: border-color 0.2s ease, box-shadow 0.2s ease;
  box-shadow: inset 0 1px 0 rgba(255, 255, 255, 0.06);
}

.selector select:focus {
  border-color: var(--accent);
  border-color: color-mix(in srgb, var(--accent) 70%, transparent);
  box-shadow: 0 0 0 3px rgba(122, 163, 255, 0.18);
  box-shadow: 0 0 0 3px color-mix(in srgb, var(--accent) 18%, transparent);
  outline: none;
}

.dropzone {
  position: relative;
  min-height: 190px;
  border: 2px dashed var(--surface-border);
  border: 2px dashed
    color-mix(in srgb, var(--surface-border) 60%, var(--accent) 20%);
  border-radius: 20px;
  background: var(--surface-acrylic);
  background: color-mix(in srgb, var(--surface-acrylic) 90%, transparent);
  display: flex;
  align-items: center;
  justify-content: center;
  transition: border-color 0.18s ease, background 0.18s ease, opacity 0.18s ease;
  cursor: pointer;
  text-align: center;
  padding: 28px;
  overflow: hidden;
}

.dropzone::after {
  content: '';
  position: absolute;
  inset: 0;
  border-radius: inherit;
  border: 1px solid transparent;
  background: linear-gradient(
      180deg,
      rgba(255, 255, 255, 0.18),
      rgba(255, 255, 255, 0)
    )
    border-box;
  opacity: 0.35;
  pointer-events: none;
}

.dropzone.disabled {
  opacity: 0.5;
  cursor: not-allowed;
}

.dropzone.active {
  border-color: var(--accent);
  border-color: color-mix(in srgb, var(--accent) 70%, white 30%);
  background: var(--accent-soft);
  background: color-mix(in srgb, var(--accent-soft) 60%, transparent);
}

.drop-content {
  position: relative;
  display: flex;
  flex-direction: column;
  gap: 8px;
  color: var(--text-primary);
  max-width: 460px;
  z-index: 1;
}

.drop-title {
  font-size: 19px;
  font-weight: 600;
}

.drop-sub {
  font-size: 13px;
  color: var(--text-secondary);
}

.actions {
  display: flex;
  align-items: center;
  gap: 12px;
  flex-wrap: wrap;
}

.actions button {
  border: none;
  border-radius: 14px;
  padding: 10px 22px;
  font-size: 14px;
  font-weight: 600;
  cursor: pointer;
  transition: transform 0.18s ease, box-shadow 0.2s ease, opacity 0.18s ease,
    background 0.2s ease;
}

.actions button.primary {
  background: linear-gradient(135deg, var(--accent), rgba(183, 148, 255, 0.92));
  background: linear-gradient(
    135deg,
    var(--accent),
    color-mix(in srgb, var(--accent) 65%, #b794ff 35%)
  );
  color: #fff;
  box-shadow: 0 14px 32px rgba(122, 163, 255, 0.26);
  box-shadow: 0 14px 32px
    color-mix(in srgb, var(--accent) 32%, rgba(0, 0, 0, 0.35));
}

.actions button.primary:disabled {
  opacity: 0.6;
  cursor: not-allowed;
  box-shadow: none;
}

.actions button.primary:not(:disabled):hover {
  transform: translateY(-2px);
  box-shadow: 0 18px 44px rgba(122, 163, 255, 0.32);
  box-shadow: 0 18px 44px
    color-mix(in srgb, var(--accent) 40%, rgba(0, 0, 0, 0.38));
}

.actions button.muted {
  background: var(--surface-acrylic);
  color: var(--text-secondary);
  border: 1px solid var(--surface-border);
}

.actions button.muted:disabled {
  opacity: 0.5;
  cursor: not-allowed;
}

.button-icon {
  margin-right: 8px;
  vertical-align: middle;
}

.android-hint {
  font-size: 13px;
  color: var(--text-secondary);
  padding: 8px 0;
  flex: 1;
  min-width: 200px;
}

.progress-card {
  display: flex;
  flex-direction: column;
  gap: 10px;
  margin-top: 8px;
  padding: 16px;
  border-radius: 16px;
  background: var(--surface-acrylic-strong);
  background: color-mix(
    in srgb,
    var(--surface-acrylic-strong) 72%,
    transparent
  );
  border: 1px solid var(--surface-border);
  box-shadow: inset 0 1px 0 rgba(255, 255, 255, 0.06);
}

.progress-header {
  display: flex;
  justify-content: space-between;
  align-items: center;
  font-size: 13px;
  color: var(--text-secondary);
}

.progress-header .stage {
  font-weight: 600;
  color: var(--text-primary);
}

.progress-header .ratio {
  font-family: 'Fira Code', 'Consolas', monospace;
}

.progress-bar {
  position: relative;
  height: 8px;
  border-radius: 999px;
  background: var(--surface-acrylic);
  background: color-mix(in srgb, var(--surface-acrylic) 70%, transparent);
  overflow: hidden;
}

.progress-bar__fill {
  position: absolute;
  top: 0;
  left: 0;
  height: 100%;
  border-radius: inherit;
  background: linear-gradient(135deg, var(--accent), rgba(183, 148, 255, 0.92));
  background: linear-gradient(
    135deg,
    var(--accent),
    color-mix(in srgb, var(--accent) 65%, #b794ff 35%)
  );
  transition: width 0.2s ease;
}

.progress-detail {
  font-size: 12px;
  color: var(--text-secondary);
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
  border: 1px solid var(--surface-border);
  background: var(--surface-acrylic);
  color: var(--text-secondary);
  font-size: 13px;
  font-weight: 600;
  cursor: pointer;
  transition: background 0.2s ease, border-color 0.2s ease, color 0.2s ease,
    box-shadow 0.2s ease;
}

.format-button:hover {
  border-color: var(--accent);
  border-color: color-mix(in srgb, var(--accent) 55%, transparent);
  color: var(--text-primary);
}

.format-button.active {
  border-color: var(--accent);
  border-color: color-mix(in srgb, var(--accent) 70%, transparent);
  background: linear-gradient(135deg, var(--accent), rgba(183, 148, 255, 0.92));
  background: linear-gradient(
    135deg,
    var(--accent),
    color-mix(in srgb, var(--accent) 60%, #b794ff 40%)
  );
  color: #fff;
  box-shadow: 0 12px 30px rgba(122, 163, 255, 0.3);
  box-shadow: 0 12px 30px
    color-mix(in srgb, var(--accent) 30%, rgba(0, 0, 0, 0.35));
}

.copy-all {
  padding: 8px 16px;
  border-radius: 12px;
  border: 1px solid var(--surface-border);
  background: var(--surface-acrylic);
  color: var(--text-secondary);
  font-size: 13px;
  font-weight: 600;
  cursor: pointer;
  transition: background 0.18s ease, color 0.18s ease, border-color 0.18s ease;
}

.copy-all:hover {
  border-color: var(--accent);
  border-color: color-mix(in srgb, var(--accent) 50%, transparent);
  color: var(--text-primary);
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
  border-radius: 16px;
  background: var(--surface-acrylic);
  background: color-mix(in srgb, var(--surface-acrylic) 85%, transparent);
  border: 1px solid var(--surface-border);
  box-shadow: inset 0 1px 0 rgba(255, 255, 255, 0.06);
}

.output-line code {
  white-space: pre-wrap;
  word-break: break-all;
  font-family: 'Fira Code', 'SFMono-Regular', Consolas, 'Liberation Mono', Menlo,
    monospace;
  font-size: 13px;
  color: var(--text-primary);
}

.output-line button {
  border: 1px solid var(--surface-border);
  border-radius: 12px;
  padding: 8px 16px;
  font-size: 12px;
  font-weight: 600;
  background: var(--surface-acrylic);
  color: var(--text-secondary);
  cursor: pointer;
  transition: background 0.18s ease, color 0.18s ease, border-color 0.18s ease;
}

.output-line button:hover {
  border-color: var(--accent);
  border-color: color-mix(in srgb, var(--accent) 45%, transparent);
  color: var(--text-primary);
}

@media (max-width: 720px) {
  .panel {
    padding: 24px;
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
