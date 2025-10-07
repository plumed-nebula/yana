<script setup lang="ts">
import { computed } from 'vue';
import { useSettingsStore } from '../stores/settings';
import { invoke } from '@tauri-apps/api/core';
import { info, error as logError } from '@tauri-apps/plugin-log';

const settings = useSettingsStore();

const convertToWebpEnabled = computed(
  () => settings.convertToWebp.value === true
);

const pngControlsDisabled = computed(() => settings.convertToWebp.value);

const pngModeDescription = computed(() => {
  return settings.pngCompressionMode.value === 'lossy'
    ? '进行颜色量化以尽量保持观感的前提下显著减小 PNG 体积。适合截图、插画等对绝对无损要求不高的场景。'
    : '保持像素原样，仅对 DEFLATE 压缩器进行调优，画质完全无损。适合对素材要求无损还原的场景。';
});

const pngOptimizationDescription = computed(() => {
  switch (settings.pngOptimization.value) {
    case 'best':
      return '优先压缩率，编码时间较长。适合离线批量压缩。';
    case 'fast':
      return '偏重速度，压缩率略有牺牲。适合快速试跑。';
    default:
      return '兼顾速度与压缩率，适用于大多数情况。';
  }
});

const animatedHint = computed(() =>
  convertToWebpEnabled.value
    ? '动图会尝试转为 WebP，可选强制转化。（注意：部分格式会退化为静态首帧）'
    : '动图默认保持原格式，GIF 会重新压缩。开启“转为 WebP”后可额外选择强制策略。'
);

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
    info('Triggered backend to open log directory');
  } catch (e) {
    logError(`Failed to open log directory: ${e}`);
  }
}

function restoreDefaults() {
  settings.quality.value = 80;
  settings.convertToWebp.value = false;
  settings.forceAnimatedWebp.value = false;
  settings.pngCompressionMode.value = 'lossless';
  settings.pngOptimization.value = 'default';
}
</script>

<template>
  <div class="wrapper">
    <div class="panel">
      <header>
        <h1>压缩参数</h1>
        <p>调整图片压缩的基础策略，所有更改会自动持久化。</p>
      </header>

      <section class="field">
        <div class="field-head">
          <label for="quality">压缩比率（0-100）</label>
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
            <span class="title">将静态图统一转为 WebP</span>
          </label>
          <p class="help">
            开启后 PNG/JPEG 等格式会输出为 WebP，可显著降低体积。
          </p>
        </div>
      </section>

      <section class="field">
        <div :class="['toggle', { disabled: !convertToWebpEnabled }]">
          <label>
            <input
              type="checkbox"
              :disabled="!convertToWebpEnabled"
              v-model="settings.forceAnimatedWebp.value"
            />
            <span class="title">动图尝试转为 WebP</span>
          </label>
          <p class="help">{{ animatedHint }}</p>
        </div>
      </section>

      <section class="field">
        <div class="field-head">
          <label>PNG 压缩策略</label>
          <span class="value" v-if="!pngControlsDisabled">
            {{
              settings.pngCompressionMode.value === 'lossy'
                ? '有损压缩'
                : '无损优化'
            }}
          </span>
        </div>
        <div :class="['option-group', { disabled: pngControlsDisabled }]">
          <label>
            <input
              type="radio"
              value="lossless"
              :disabled="pngControlsDisabled"
              v-model="settings.pngCompressionMode.value"
            />
            <span class="title">无损优化</span>
            <span class="desc">保持像素数据不变，仅优化压缩级别。</span>
          </label>
          <label>
            <input
              type="radio"
              value="lossy"
              :disabled="pngControlsDisabled"
              v-model="settings.pngCompressionMode.value"
            />
            <span class="title">有损压缩</span>
            <span class="desc">通过颜色量化进一步减小体积。</span>
          </label>
        </div>
        <p class="help">
          {{
            pngControlsDisabled
              ? '当前输出为 WebP，此设置暂不生效。'
              : pngModeDescription
          }}
        </p>
      </section>

      <section class="field">
        <div class="field-head">
          <label for="png-opt">PNG 优化级别</label>
          <span class="value" v-if="!pngControlsDisabled">
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
          <select
            id="png-opt"
            :disabled="pngControlsDisabled"
            v-model="settings.pngOptimization.value"
          >
            <option value="best">最佳压缩（最慢）</option>
            <option value="default">标准（推荐）</option>
            <option value="fast">快速（体积略大）</option>
          </select>
        </div>
        <p class="help">
          {{
            pngControlsDisabled
              ? '当前输出为 WebP，此设置暂不生效。'
              : pngOptimizationDescription
          }}
        </p>
      </section>

      <footer>
        <div class="status" :class="{ error: !!settings.error.value }">
          {{ persistenceMessage }}
        </div>
        <div class="buttons">
          <button type="button" @click="openLogDir">打开日志目录</button>
          <button type="button" @click="restoreDefaults">恢复默认</button>
        </div>
      </footer>
    </div>
  </div>
</template>

<style scoped>
.wrapper {
  width: min(720px, 100%);
}

.panel {
  background: rgba(255, 255, 255, 0.88);
  border-radius: 22px;
  box-shadow: 0 18px 42px rgba(15, 27, 53, 0.18);
  padding: 32px;
  backdrop-filter: blur(16px);
  border: 1px solid rgba(255, 255, 255, 0.55);
}

header h1 {
  margin: 0 0 6px;
  font-size: 26px;
  color: #0c1c38;
}

header p {
  margin: 0;
  color: rgba(12, 28, 56, 0.65);
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
  color: #101f3c;
}

.field-head .value {
  font-family: 'Fira Code', 'Consolas', monospace;
  font-size: 15px;
  color: rgba(16, 31, 60, 0.75);
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
  width: 84px;
  padding: 8px 10px;
  border-radius: 10px;
  border: 1px solid rgba(16, 31, 60, 0.18);
  background: rgba(255, 255, 255, 0.9);
}

.field-body select {
  flex: 1;
  min-width: 220px;
  padding: 10px 12px;
  border-radius: 12px;
  border: 1px solid rgba(16, 31, 60, 0.18);
  background: rgba(255, 255, 255, 0.92);
  color: #10203f;
}

.toggle {
  background: rgba(16, 31, 60, 0.05);
  border-radius: 16px;
  padding: 18px 20px;
  display: flex;
  flex-direction: column;
  gap: 8px;
  transition: opacity 0.2s ease;
}

.toggle label {
  display: flex;
  align-items: center;
  gap: 12px;
  font-weight: 600;
  color: #102040;
}

.toggle .title {
  font-size: 16px;
}

.toggle.disabled {
  opacity: 0.5;
}

.help {
  margin: 0;
  color: rgba(16, 31, 60, 0.6);
  font-size: 14px;
}

.option-group {
  display: grid;
  gap: 12px;
  background: rgba(16, 31, 60, 0.05);
  border-radius: 16px;
  padding: 16px 18px;
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
  color: #102040;
}

.option-group .title {
  font-size: 16px;
}

.option-group .desc {
  grid-column: 2 / 3;
  font-size: 13px;
  color: rgba(16, 31, 60, 0.6);
}

footer {
  margin-top: 36px;
  display: flex;
  justify-content: space-between;
  align-items: center;
  gap: 16px;
}

.status {
  color: rgba(14, 29, 58, 0.7);
  font-size: 14px;
}

.status.error {
  color: #b21e35;
}

.buttons {
  display: flex;
  gap: 12px;
}

footer button {
  border: none;
  padding: 10px 18px;
  border-radius: 12px;
  background: rgba(12, 28, 56, 0.12);
  color: #11213f;
  font-weight: 600;
  transition: background 0.2s ease;
}

footer button:hover {
  background: rgba(17, 33, 63, 0.22);
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
