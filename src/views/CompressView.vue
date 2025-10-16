<script setup lang="ts">
import { computed, ref } from 'vue';
import { open, save } from '@tauri-apps/plugin-dialog';
import { basename, dirname, extname, join } from '@tauri-apps/api/path';
import { invoke } from '@tauri-apps/api/core';
import { useSettingsStore } from '../stores/settings';

type MessageType = 'info' | 'success' | 'error';

type HistoryItem = {
  id: number;
  type: MessageType;
  text: string;
  timestamp: number;
};

const settings = useSettingsStore();
const busy = ref(false);
const history = ref<HistoryItem[]>([]);
const nextId = ref(1);

const currentModeLabel = computed(() =>
  settings.convertToWebp.value ? '输出 WebP' : '保留原格式'
);

const animatedStrategy = computed(() =>
  settings.forceAnimatedWebp.value
    ? '动图会尝试转为 WebP（可能退化为首帧静态）'
    : '动图保持原格式，GIF 会重新压缩'
);

const qualityLabel = computed(() => `${settings.quality.value} / 100`);

const pngStrategy = computed(() => {
  if (settings.convertToWebp.value) {
    return '静态图输出为 WebP，PNG 策略无效';
  }
  const mode =
    settings.pngCompressionMode.value === 'lossy' ? '有损压缩' : '无损优化';
  const opt =
    settings.pngOptimization.value === 'best'
      ? '最佳压缩'
      : settings.pngOptimization.value === 'fast'
      ? '快速'
      : '标准';
  return `${mode} / ${opt}`;
});

function pushMessage(type: MessageType, text: string) {
  history.value.unshift({
    id: nextId.value++,
    type,
    text,
    timestamp: Date.now(),
  });
  if (history.value.length > 12) history.value.length = 12;
}

function mutateLatest(type: MessageType, text: string) {
  const latest = history.value[0];
  if (latest) {
    latest.type = type;
    latest.text = text;
    latest.timestamp = Date.now();
    history.value = [...history.value];
  } else {
    pushMessage(type, text);
  }
}

function ensureArray(input: string | string[]): string[] {
  return Array.isArray(input) ? input : [input];
}

async function buildDefaultSavePath(source: string, targetWebp: boolean) {
  const dir = await dirname(source);
  const base = await basename(source);
  const ext = await extname(source);
  const stem = ext ? base.slice(0, -ext.length - 1) : base;

  const suffix = targetWebp ? 'webp' : ext;
  const tagged = `${stem}_compressed.${suffix}`;

  return await join(dir, tagged);
}

async function handleTest() {
  if (busy.value) return;
  if (!settings.ready.value) {
    pushMessage('info', '设置仍在加载，请稍候再试。');
    return;
  }

  busy.value = true;
  pushMessage('info', '选择要测试的图片…');

  try {
    const selected = await open({
      multiple: false,
      filters: [
        {
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
          ],
        },
      ],
    });

    if (!selected) {
      mutateLatest('info', '已取消选择。');
      return;
    }

    const paths = ensureArray(selected);
    const [target] = paths;
    mutateLatest('info', `已选择：${target}`);

    const forceAnimated =
      settings.forceAnimatedWebp.value && settings.convertToWebp.value;
    const mode = settings.convertToWebp.value ? 'webp' : 'original_format';

    mutateLatest('info', '正在压缩…');
    const outputs = await invoke<string[]>('compress_images', {
      paths,
      quality: settings.quality.value,
      mode,
      forceAnimatedWebp: forceAnimated,
      pngMode: settings.pngCompressionMode.value,
      pngOptimization: settings.pngOptimization.value,
    });

    if (!outputs.length) {
      throw new Error('后端未返回压缩结果。');
    }

    const defaultSave = await buildDefaultSavePath(
      target,
      settings.convertToWebp.value
    );

    mutateLatest('info', '选择保存位置…');
    const dest = await save({
      defaultPath: defaultSave,
      filters: settings.convertToWebp.value
        ? [{ name: 'WebP 图片', extensions: ['webp'] }]
        : undefined,
    });

    if (!dest) {
      mutateLatest('info', '已取消保存，临时文件仍保留在系统缓存目录。');
      return;
    }

    mutateLatest('info', '正在写入文件…');
    const copied = await invoke<number>('save_files', {
      sources: outputs,
      dests: [dest],
    });

    if (copied < 1) {
      throw new Error('保存失败，请检查权限或磁盘空间。');
    }

    mutateLatest('success', `已保存到 ${dest}`);
  } catch (err) {
    mutateLatest('error', err instanceof Error ? err.message : String(err));
  } finally {
    busy.value = false;
  }
}
</script>

<template>
  <div class="wrapper">
    <div class="card">
      <header>
        <h1>图片压缩</h1>
        <p>根据当前设置快速试跑一次压缩流程，体验效果并确认输出。</p>
      </header>

      <section class="overview">
        <div class="chip">
          <span class="label">目标模式</span>
          <span>{{ currentModeLabel }}</span>
        </div>
        <div class="chip">
          <span class="label">画质</span>
          <span>{{ qualityLabel }}</span>
        </div>
        <div class="chip">
          <span class="label">动画策略</span>
          <span>{{ animatedStrategy }}</span>
        </div>
        <div class="chip" v-if="!settings.convertToWebp.value">
          <span class="label">PNG 策略</span>
          <span>{{ pngStrategy }}</span>
        </div>
      </section>

      <section class="actions">
        <button
          type="button"
          class="primary"
          :disabled="busy || !settings.ready.value"
          @click="handleTest"
        >
          {{ busy ? '处理中…' : '选择图片并测试' }}
        </button>
        <p v-if="!settings.ready.value" class="hint">正在加载设置，请稍候。</p>
        <p v-else class="hint">
          将直接调用桌面文件对话框，过程中的临时文件位于系统缓存目录。
        </p>
      </section>
    </div>

    <div class="history" v-if="history.length">
      <h2>最近记录</h2>
      <ul>
        <li v-for="item in history" :key="item.id" :class="item.type">
          <span class="time">{{
            new Date(item.timestamp).toLocaleTimeString()
          }}</span>
          <span class="text">{{ item.text }}</span>
        </li>
      </ul>
    </div>
  </div>
</template>

<style scoped>
.wrapper {
  width: 100%;
  display: flex;
  flex-direction: column;
  gap: 24px;
  color: var(--text-primary);
}

.card {
  background: var(--surface-panel);
  border-radius: 22px;
  box-shadow: var(--shadow-strong);
  padding: 32px;
  backdrop-filter: blur(18px) saturate(1.08);
  border: 1px solid var(--surface-border);
  width: 100%;
  transition: border-color 0.2s ease, box-shadow 0.2s ease;
}

.card:hover {
  border-color: var(--surface-border);
  border-color: color-mix(in srgb, var(--surface-border) 60%, var(--accent));
  box-shadow: 0 26px 52px rgba(6, 12, 24, 0.32);
}

header h1 {
  font-size: 28px;
  margin-bottom: 8px;
  color: var(--text-primary);
}

header p {
  color: var(--text-secondary);
  margin: 0;
}

.overview {
  display: flex;
  flex-wrap: wrap;
  gap: 12px;
  margin: 24px 0 8px;
}

.chip {
  display: inline-flex;
  flex-direction: column;
  gap: 4px;
  padding: 12px 16px;
  border-radius: 14px;
  background: var(--surface-acrylic);
  border: 1px solid var(--surface-border);
  color: var(--text-primary);
  min-width: 160px;
  box-shadow: inset 0 1px 0 rgba(255, 255, 255, 0.08);
}

.chip .label {
  font-size: 12px;
  text-transform: uppercase;
  letter-spacing: 1.2px;
  color: var(--text-secondary);
}

.actions {
  display: flex;
  flex-direction: column;
  gap: 10px;
  margin-top: 24px;
}

.primary {
  align-self: flex-start;
  background: linear-gradient(135deg, var(--accent), rgba(183, 148, 255, 0.92));
  background: linear-gradient(
    135deg,
    var(--accent),
    color-mix(in srgb, var(--accent) 65%, #b794ff 35%)
  );
  color: #fff;
  border: none;
  border-radius: 14px;
  padding: 12px 24px;
  font-size: 16px;
  font-weight: 600;
  box-shadow: 0 12px 28px rgba(122, 163, 255, 0.28);
  box-shadow: 0 12px 28px
    color-mix(in srgb, var(--accent) 35%, rgba(0, 0, 0, 0.38));
  transition: transform 0.18s ease, box-shadow 0.2s ease, opacity 0.18s ease;
}

.primary:disabled {
  opacity: 0.65;
  cursor: not-allowed;
  box-shadow: none;
}

.primary:not(:disabled):hover {
  transform: translateY(-2px);
  box-shadow: 0 18px 42px rgba(122, 163, 255, 0.32);
  box-shadow: 0 18px 42px
    color-mix(in srgb, var(--accent) 40%, rgba(0, 0, 0, 0.42));
}

.hint {
  margin: 0;
  color: var(--text-secondary);
  font-size: 14px;
}

.history {
  background: var(--surface-acrylic);
  border-radius: 18px;
  padding: 20px 26px;
  border: 1px solid var(--surface-border);
  box-shadow: inset 0 1px 0 rgba(255, 255, 255, 0.08), var(--shadow-soft);
}

.history h2 {
  margin-top: 0;
  margin-bottom: 12px;
  font-size: 18px;
  color: var(--text-primary);
}

.history ul {
  margin: 0;
  padding: 0;
  list-style: none;
  display: flex;
  flex-direction: column;
  gap: 8px;
}

.history li {
  display: flex;
  gap: 12px;
  align-items: baseline;
  font-size: 14px;
  padding: 10px 14px;
  border-radius: 12px;
  background: var(--surface-panel);
  border: 1px solid var(--surface-border);
  color: var(--text-secondary);
  box-shadow: inset 0 1px 0 rgba(255, 255, 255, 0.05);
}

.history li.info {
  border-color: var(--accent-soft);
  border-color: color-mix(in srgb, var(--accent) 32%, transparent);
  background: var(--accent-soft);
  background: color-mix(in srgb, var(--accent-soft) 75%, transparent);
  color: var(--accent);
}

.history li.success {
  border-color: rgba(44, 187, 126, 0.38);
  background: rgba(44, 187, 126, 0.16);
  color: #2cbb7e;
}

.history li.error {
  border-color: var(--danger-soft);
  background: var(--danger-soft);
  background: color-mix(in srgb, var(--danger-soft) 80%, transparent);
  color: var(--danger);
}

.time {
  font-feature-settings: 'tnum';
  color: var(--text-secondary);
  min-width: 86px;
}

.text {
  flex: 1;
  word-break: break-all;
  color: var(--text-primary);
}

@media (max-width: 720px) {
  .card {
    padding: 24px;
  }
  .overview {
    flex-direction: column;
  }
  .chip {
    width: 100%;
  }
  .primary {
    width: 100%;
    text-align: center;
  }
}
</style>
