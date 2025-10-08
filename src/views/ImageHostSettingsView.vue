<script setup lang="ts">
import { computed, ref, watch } from 'vue';
import { useImageHostStore } from '../stores/imageHosts';

const store = useImageHostStore();
const selectedId = ref<string | null>(null);

const plugins = store.plugins;
const loading = store.loading;
const ready = store.ready;
const errorRef = store.error;

watch(
  () => plugins.value,
  (list) => {
    if (!list.length) {
      selectedId.value = null;
      return;
    }
    const exists = list.some((plugin) => plugin.id === selectedId.value);
    if (!exists) {
      selectedId.value = list[0]?.id ?? null;
    }
  },
  { immediate: true }
);

const activePlugin = computed(() => {
  if (!selectedId.value) return null;
  return plugins.value.find((item) => item.id === selectedId.value) ?? null;
});

const activeSettings = computed(() => {
  const plugin = activePlugin.value;
  if (!plugin) return null;
  return store.getSettingsState(plugin.id) ?? null;
});

function selectPlugin(id: string) {
  selectedId.value = id;
}

function formatTimestamp(value: number | null): string {
  if (!value) return '尚未保存';
  try {
    const formatter = new Intl.DateTimeFormat('zh-CN', {
      dateStyle: 'medium',
      timeStyle: 'medium',
    });
    return formatter.format(new Date(value));
  } catch (err) {
    console.warn('[imageHosts:view] failed to format timestamp', err);
    return new Date(value).toLocaleString();
  }
}

const persistenceMessage = computed(() => {
  if (!ready.value) return '正在加载插件配置…';
  if (loading.value) return '正在读取或保存配置…';
  if (errorRef.value) return `加载插件时出现问题：${errorRef.value}`;
  return '插件配置将自动同步到本地存储。';
});

const supportedTypesText = computed(() => {
  const plugin = activePlugin.value;
  if (!plugin || !plugin.supportedFileTypes?.length) {
    return '接受任何常见图片格式。';
  }
  return plugin.supportedFileTypes
    .map((item) => {
      const parts: string[] = [];
      if (item.description) parts.push(item.description);
      if (item.mimeTypes?.length)
        parts.push(`MIME: ${item.mimeTypes.join(', ')}`);
      if (item.extensions?.length)
        parts.push(`扩展名: ${item.extensions.join(', ')}`);
      return parts.join(' · ');
    })
    .join(' / ');
});

function handleManualSave() {
  const plugin = activePlugin.value;
  if (!plugin) return;
  store.saveNow(plugin.id);
}

const activeValues = computed<Record<string, any> | null>(() => {
  const settings = activeSettings.value;
  if (!settings) return null;
  return settings.values as Record<string, any>;
});
</script>

<template>
  <div class="hosts-layout">
    <aside class="hosts-sidebar">
      <header>
        <h2>可用图床</h2>
        <p>{{ persistenceMessage }}</p>
      </header>

      <div class="plugin-list" v-if="plugins.length">
        <button
          v-for="plugin in plugins"
          :key="plugin.id"
          type="button"
          :class="['plugin-item', { active: plugin.id === selectedId }]"
          @click="selectPlugin(plugin.id)"
        >
          <span class="name">{{ plugin.name }}</span>
          <span class="meta">{{ plugin.author ?? '官方提供' }}</span>
        </button>
      </div>

      <div v-else class="empty">
        <p v-if="loading">正在加载图床插件…</p>
        <p v-else-if="errorRef">{{ errorRef }}</p>
        <p v-else>暂无可用插件。</p>
      </div>
    </aside>

    <section class="hosts-content">
      <div v-if="!ready" class="status">正在初始化插件系统…</div>
      <div v-else-if="errorRef" class="status error">{{ errorRef }}</div>
      <div v-else-if="!activePlugin" class="status">
        请选择要配置的图床插件。
      </div>
      <div v-else class="plugin-panel">
        <header>
          <div class="title">
            <h1>{{ activePlugin.name }}</h1>
            <p v-if="activePlugin.description">
              {{ activePlugin.description }}
            </p>
          </div>
          <ul class="meta">
            <li v-if="activePlugin.version">版本 {{ activePlugin.version }}</li>
            <li>脚本来源：{{ activePlugin.sourceUrl }}</li>
            <li>支持格式：{{ supportedTypesText }}</li>
          </ul>
        </header>

        <form class="form" @submit.prevent>
          <fieldset v-if="activeValues" class="fields">
            <legend>插件参数</legend>
            <template v-if="activePlugin.parameters.length">
              <div
                v-for="descriptor in activePlugin.parameters"
                :key="descriptor.key"
                class="field"
              >
                <div class="field-head">
                  <span class="title">{{ descriptor.label }}</span>
                  <span v-if="descriptor.required" class="required">必填</span>
                </div>
                <div class="control" :class="`type-${descriptor.type}`">
                  <template v-if="descriptor.type === 'text'">
                    <input
                      :id="`param-${descriptor.key}`"
                      type="text"
                      v-model="activeValues[descriptor.key]"
                      :placeholder="descriptor.description"
                      :required="descriptor.required"
                    />
                  </template>
                  <template v-else-if="descriptor.type === 'password'">
                    <input
                      :id="`param-${descriptor.key}`"
                      type="password"
                      v-model="activeValues[descriptor.key]"
                      :placeholder="descriptor.description"
                      :required="descriptor.required"
                    />
                  </template>
                  <template v-else-if="descriptor.type === 'number'">
                    <input
                      :id="`param-${descriptor.key}`"
                      type="number"
                      v-model.number="activeValues[descriptor.key]"
                      :placeholder="descriptor.description"
                      :required="descriptor.required"
                    />
                  </template>
                  <template v-else-if="descriptor.type === 'boolean'">
                    <div class="boolean-toggle">
                      <input
                        :id="`param-${descriptor.key}`"
                        type="checkbox"
                        v-model="activeValues[descriptor.key]"
                      />
                      <label :for="`param-${descriptor.key}`">
                        {{ activeValues[descriptor.key] ? '已启用' : '未启用' }}
                      </label>
                    </div>
                  </template>
                  <template v-else-if="descriptor.type === 'select'">
                    <select
                      :id="`param-${descriptor.key}`"
                      v-model="activeValues[descriptor.key]"
                      :required="descriptor.required"
                    >
                      <option value="" v-if="!descriptor.required">
                        -- 请选择 --
                      </option>
                      <option
                        v-for="option in descriptor.options ?? []"
                        :key="`${descriptor.key}-${option.value}`"
                        :value="option.value"
                      >
                        {{ option.label }}
                      </option>
                    </select>
                  </template>
                  <template v-else-if="descriptor.type === 'textarea'">
                    <textarea
                      :id="`param-${descriptor.key}`"
                      v-model="activeValues[descriptor.key]"
                      :placeholder="descriptor.description"
                      :required="descriptor.required"
                      rows="3"
                    />
                  </template>
                  <template v-else>
                    <input
                      :id="`param-${descriptor.key}`"
                      type="text"
                      v-model="activeValues[descriptor.key]"
                      :placeholder="descriptor.description"
                      :required="descriptor.required"
                    />
                  </template>
                </div>
                <p v-if="descriptor.description" class="help">
                  {{ descriptor.description }}
                </p>
              </div>
            </template>
            <p v-else class="help">此插件不需要额外配置。</p>
          </fieldset>
          <fieldset v-else class="fields">
            <legend>插件参数</legend>
            <p class="help">正在准备配置，请稍候…</p>
          </fieldset>
        </form>

        <footer>
          <div
            class="status-text"
            :class="{ error: !!(activeSettings && activeSettings.error) }"
          >
            <template v-if="activeSettings && activeSettings.error">
              保存失败：{{ activeSettings.error }}
            </template>
            <template v-else-if="activeSettings && activeSettings.saving">
              正在保存配置…
            </template>
            <template v-else>
              上次保存：{{
                formatTimestamp(
                  activeSettings ? activeSettings.lastSavedAt : null
                )
              }}
            </template>
          </div>
          <button type="button" @click="handleManualSave">立即保存</button>
        </footer>
      </div>
    </section>
  </div>
</template>

<style scoped>
.hosts-layout {
  display: grid;
  grid-template-columns: minmax(240px, 320px) 1fr;
  gap: 28px;
  width: min(100%, 1080px);
}

.hosts-sidebar {
  background: rgba(255, 255, 255, 0.86);
  border-radius: 20px;
  padding: 24px;
  display: flex;
  flex-direction: column;
  gap: 18px;
  border: 1px solid rgba(12, 28, 56, 0.12);
  box-shadow: 0 12px 32px rgba(15, 27, 53, 0.15);
  backdrop-filter: blur(14px);
}

.hosts-sidebar header h2 {
  margin: 0 0 6px;
  font-size: 20px;
  color: #0e1d3c;
}

.hosts-sidebar header p {
  margin: 0;
  color: rgba(14, 29, 60, 0.6);
  font-size: 13px;
}

.plugin-list {
  display: flex;
  flex-direction: column;
  gap: 12px;
}

.plugin-item {
  display: flex;
  flex-direction: column;
  align-items: flex-start;
  gap: 6px;
  padding: 12px 14px;
  border-radius: 14px;
  border: 1px solid transparent;
  background: rgba(17, 35, 68, 0.06);
  color: #112345;
  font-weight: 600;
  transition: background 0.18s ease, transform 0.18s ease,
    border-color 0.18s ease;
}

.plugin-item .name {
  font-size: 16px;
}

.plugin-item .meta {
  font-size: 13px;
  color: rgba(17, 35, 68, 0.7);
}

.plugin-item:hover {
  background: rgba(17, 35, 68, 0.12);
  transform: translateX(2px);
}

.plugin-item.active {
  border-color: rgba(17, 35, 68, 0.35);
  background: rgba(17, 35, 68, 0.18);
  box-shadow: 0 10px 24px rgba(10, 20, 40, 0.22);
}

.empty {
  padding: 20px;
  font-size: 14px;
  color: rgba(14, 29, 60, 0.6);
  background: rgba(17, 35, 68, 0.08);
  border-radius: 16px;
}

.hosts-content {
  background: rgba(255, 255, 255, 0.9);
  border-radius: 24px;
  border: 1px solid rgba(12, 28, 56, 0.12);
  box-shadow: 0 18px 42px rgba(15, 27, 53, 0.18);
  padding: 32px;
  display: flex;
  flex-direction: column;
  gap: 24px;
  backdrop-filter: blur(16px);
}

.status {
  font-size: 15px;
  color: rgba(14, 29, 60, 0.7);
}

.status.error {
  color: #b21e35;
}

.plugin-panel header {
  display: flex;
  flex-direction: column;
  gap: 12px;
}

.plugin-panel header .title h1 {
  margin: 0;
  font-size: 26px;
  color: #0c1c38;
}

.plugin-panel header .title p {
  margin: 0;
  color: rgba(14, 29, 60, 0.65);
}

.plugin-panel header .meta {
  margin: 0;
  padding: 0;
  list-style: none;
  display: flex;
  flex-direction: column;
  gap: 4px;
  font-size: 13px;
  color: rgba(14, 29, 60, 0.6);
}

.fields {
  border: none;
  padding: 0;
  margin: 0;
  display: flex;
  flex-direction: column;
  gap: 18px;
}

.fields legend {
  font-weight: 600;
  margin-bottom: 12px;
  color: rgba(14, 29, 60, 0.76);
}

.field {
  display: flex;
  flex-direction: column;
  gap: 10px;
}

.field-head {
  display: flex;
  align-items: center;
  gap: 10px;
}

.field-head .title {
  font-weight: 600;
  color: #10203f;
}

.field-head .required {
  font-size: 12px;
  color: #b21e35;
  background: rgba(178, 30, 53, 0.12);
  padding: 2px 8px;
  border-radius: 999px;
}

.control input,
.control select,
.control textarea {
  width: 100%;
  padding: 10px 12px;
  border-radius: 12px;
  border: 1px solid rgba(16, 31, 60, 0.18);
  background: rgba(255, 255, 255, 0.92);
  color: #10203f;
  font-size: 14px;
  transition: border-color 0.15s ease, box-shadow 0.15s ease;
}

.control input:focus,
.control select:focus,
.control textarea:focus {
  border-color: rgba(17, 33, 63, 0.45);
  box-shadow: 0 0 0 3px rgba(17, 33, 63, 0.12);
  outline: none;
}

.boolean-toggle {
  display: inline-flex;
  align-items: center;
  gap: 10px;
  cursor: pointer;
  user-select: none;
  padding: 10px 12px;
  border-radius: 12px;
  border: 1px solid rgba(16, 31, 60, 0.18);
  background: rgba(255, 255, 255, 0.92);
}

.boolean-toggle input {
  width: 18px;
  height: 18px;
}

.boolean-toggle label {
  font-weight: 600;
  color: #10203f;
}

.help {
  margin: 0;
  color: rgba(16, 31, 60, 0.58);
  font-size: 13px;
}

footer {
  display: flex;
  justify-content: space-between;
  align-items: center;
  gap: 16px;
  margin-top: 12px;
}

.status-text {
  font-size: 14px;
  color: rgba(14, 29, 60, 0.7);
}

.status-text.error {
  color: #b21e35;
}

footer button {
  border: none;
  padding: 10px 18px;
  border-radius: 12px;
  background: rgba(12, 28, 56, 0.16);
  color: #11213f;
  font-weight: 600;
  transition: background 0.2s ease;
}

footer button:hover {
  background: rgba(17, 33, 63, 0.26);
}

@media (max-width: 960px) {
  .hosts-layout {
    grid-template-columns: 1fr;
  }

  .hosts-sidebar {
    order: 2;
  }

  .hosts-content {
    order: 1;
  }
}
</style>
