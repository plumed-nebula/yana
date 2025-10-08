<script setup lang="ts">
import { computed } from 'vue';
import { useImageHostStore } from '../stores/imageHosts';

const props = defineProps<{
  pluginId: string | null;
}>();

const store = useImageHostStore();

void store.ensureLoaded();

const ready = store.ready;
const loading = store.loading;
const errorRef = store.error;

const activePlugin = computed(() => {
  if (!props.pluginId) return null;
  return store.getPluginById(props.pluginId) ?? null;
});

const activeSettings = computed(() => {
  const plugin = activePlugin.value;
  if (!plugin) return null;
  return store.getSettingsState(plugin.id) ?? null;
});

const activeValues = computed<Record<string, any> | null>(() => {
  const settings = activeSettings.value;
  if (!settings) return null;
  return settings.values as Record<string, any>;
});

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

const supportedTypesText = computed(() => {
  const plugin = activePlugin.value;
  if (!plugin || !plugin.supportedFileTypes?.length) {
    return '接受常见图片格式。';
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
</script>

<template>
  <div class="host-container">
    <section class="panel">
      <header class="panel-head">
        <h1>图床插件配置</h1>
        <p>在侧栏选择插件后，可在此调整其参数并自动保存。</p>
      </header>

      <div v-if="!ready" class="status-block info">正在加载插件列表…</div>
      <div v-else-if="errorRef" class="status-block error">{{ errorRef }}</div>
      <div v-else-if="loading && !activePlugin" class="status-block info">
        正在准备插件信息…
      </div>
      <div v-else-if="!activePlugin" class="status-block muted">
        请在侧栏的图床列表中选择需要配置的插件。
      </div>
      <template v-else>
        <section class="plugin-summary">
          <div class="summary-main">
            <h2>{{ activePlugin.name }}</h2>
            <p v-if="activePlugin.description">
              {{ activePlugin.description }}
            </p>
          </div>
          <ul class="summary-meta">
            <li v-if="activePlugin.author">作者：{{ activePlugin.author }}</li>
            <li v-if="activePlugin.version">
              版本：{{ activePlugin.version }}
            </li>
            <li>脚本来源：{{ activePlugin.sourceUrl }}</li>
            <li>支持格式：{{ supportedTypesText }}</li>
          </ul>
        </section>

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
                    <label
                      class="boolean-toggle"
                      :for="`param-${descriptor.key}`"
                    >
                      <input
                        :id="`param-${descriptor.key}`"
                        type="checkbox"
                        v-model="activeValues[descriptor.key]"
                      />
                      <span>{{
                        activeValues[descriptor.key] ? '已启用' : '未启用'
                      }}</span>
                    </label>
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

        <footer class="panel-footer">
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
      </template>
    </section>
  </div>
</template>

<style scoped>
.host-container {
  width: min(920px, 100%);
}

.panel {
  background: rgba(255, 255, 255, 0.9);
  border-radius: 24px;
  border: 1px solid rgba(12, 28, 56, 0.12);
  box-shadow: 0 20px 46px rgba(15, 27, 53, 0.16);
  padding: 32px;
  display: flex;
  flex-direction: column;
  gap: 24px;
  backdrop-filter: blur(18px);
}

.panel-head h1 {
  margin: 0 0 6px;
  font-size: 26px;
  color: #0c1c38;
}

.panel-head p {
  margin: 0;
  color: rgba(14, 29, 60, 0.64);
}

.status-block {
  padding: 18px 20px;
  border-radius: 14px;
  font-size: 15px;
}

.status-block.info {
  background: rgba(20, 60, 160, 0.08);
  color: #143ca0;
}

.status-block.error {
  background: rgba(178, 30, 53, 0.1);
  color: #9c1f33;
}

.status-block.muted {
  background: rgba(16, 31, 60, 0.06);
  color: rgba(16, 31, 60, 0.72);
}

.plugin-summary {
  display: flex;
  flex-direction: column;
  gap: 14px;
  padding: 20px;
  border-radius: 18px;
  background: rgba(16, 31, 60, 0.05);
}

.plugin-summary h2 {
  margin: 0 0 4px;
  font-size: 22px;
  color: #0c1c38;
}

.plugin-summary p {
  margin: 0;
  color: rgba(14, 29, 60, 0.66);
}

.summary-meta {
  margin: 0;
  padding: 0;
  list-style: none;
  display: flex;
  flex-direction: column;
  gap: 6px;
  font-size: 13px;
  color: rgba(14, 29, 60, 0.64);
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

.boolean-toggle span {
  font-weight: 600;
  color: #10203f;
}

.help {
  margin: 0;
  color: rgba(16, 31, 60, 0.58);
  font-size: 13px;
}

.panel-footer {
  display: flex;
  justify-content: space-between;
  align-items: center;
  gap: 16px;
  margin-top: 8px;
}

.status-text {
  font-size: 14px;
  color: rgba(14, 29, 60, 0.7);
}

.status-text.error {
  color: #b21e35;
}

.panel-footer button {
  border: none;
  padding: 10px 18px;
  border-radius: 12px;
  background: rgba(12, 28, 56, 0.16);
  color: #11213f;
  font-weight: 600;
  transition: background 0.2s ease;
}

.panel-footer button:hover {
  background: rgba(17, 33, 63, 0.26);
}

@media (max-width: 640px) {
  .panel {
    padding: 26px;
  }
}
</style>
