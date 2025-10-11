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
                    <GlobalSelect
                      :id="`param-${descriptor.key}`"
                      v-model="activeValues[descriptor.key]"
                      :options="descriptor.options"
                      :placeholder="
                        !descriptor.required ? '-- 请选择 --' : undefined
                      "
                    />
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
  width: 100%;
  display: flex;
  flex-direction: column;
  color: var(--text-primary);
}

.panel {
  background: var(--surface-panel);
  border-radius: 24px;
  border: 1px solid var(--surface-border);
  box-shadow: var(--shadow-strong);
  padding: 32px;
  display: flex;
  flex-direction: column;
  gap: 24px;
  backdrop-filter: blur(20px) saturate(1.06);
  width: 100%;
  transition: border-color 0.2s ease, box-shadow 0.2s ease;
}

.panel:hover {
  border-color: var(--surface-border);
  border-color: color-mix(in srgb, var(--surface-border) 60%, var(--accent));
  box-shadow: 0 28px 60px rgba(6, 12, 24, 0.34);
}

.panel-head h1 {
  margin: 0 0 6px;
  font-size: 26px;
  color: var(--text-primary);
}

.panel-head p {
  margin: 0;
  color: var(--text-secondary);
}

.status-block {
  padding: 18px 20px;
  border-radius: 14px;
  font-size: 15px;
  border: 1px solid var(--surface-border);
  background: var(--surface-acrylic);
  color: var(--text-secondary);
}

.status-block.info {
  background: var(--accent-soft);
  background: color-mix(in srgb, var(--accent-soft) 80%, transparent);
  color: var(--accent);
  border-color: var(--accent);
  border-color: color-mix(in srgb, var(--accent) 28%, transparent);
}

.status-block.error {
  background: var(--danger-soft);
  color: var(--danger);
  border-color: var(--danger);
  border-color: color-mix(in srgb, var(--danger) 28%, transparent);
}

.status-block.muted {
  color: var(--text-secondary);
}

.plugin-summary {
  display: flex;
  flex-direction: column;
  gap: 14px;
  padding: 20px;
  border-radius: 18px;
  background: var(--surface-acrylic);
  border: 1px solid var(--surface-border);
  box-shadow: inset 0 1px 0 rgba(255, 255, 255, 0.06);
}

.plugin-summary h2 {
  margin: 0 0 4px;
  font-size: 22px;
  color: var(--text-primary);
}

.plugin-summary p {
  margin: 0;
  color: var(--text-secondary);
}

.summary-meta {
  margin: 0;
  padding: 0;
  list-style: none;
  display: flex;
  flex-direction: column;
  gap: 6px;
  font-size: 13px;
  color: var(--text-secondary);
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
  color: var(--text-primary);
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
  color: var(--text-primary);
}

.field-head .required {
  font-size: 12px;
  color: var(--danger);
  background: var(--danger-soft);
  padding: 2px 8px;
  border-radius: 999px;
}

.control input,
.control select,
.control textarea {
  width: 100%;
  padding: 10px 12px;
  border-radius: 12px;
  border: 1px solid var(--surface-border);
  background: var(--surface-acrylic);
  color: var(--text-primary);
  font-size: 14px;
  transition: border-color 0.18s ease, box-shadow 0.18s ease;
  box-shadow: inset 0 1px 0 rgba(255, 255, 255, 0.06);
}

.control input:focus,
.control select:focus,
.control textarea:focus {
  border-color: var(--accent);
  border-color: color-mix(in srgb, var(--accent) 70%, transparent);
  box-shadow: 0 0 0 3px rgba(122, 163, 255, 0.18);
  box-shadow: 0 0 0 3px color-mix(in srgb, var(--accent) 18%, transparent);
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
  border: 1px solid var(--surface-border);
  background: var(--surface-acrylic);
  color: var(--text-primary);
}

.boolean-toggle input {
  width: 18px;
  height: 18px;
}

.boolean-toggle span {
  font-weight: 600;
  color: var(--text-primary);
}

.help {
  margin: 0;
  color: var(--text-secondary);
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
  color: var(--text-secondary);
}

.status-text.error {
  color: var(--danger);
}

.panel-footer button {
  border: none;
  padding: 10px 18px;
  border-radius: 12px;
  background: linear-gradient(135deg, var(--accent), rgba(183, 148, 255, 0.92));
  background: linear-gradient(
    135deg,
    var(--accent),
    color-mix(in srgb, var(--accent) 65%, #b794ff 35%)
  );
  color: #fff;
  font-weight: 600;
  transition: transform 0.2s ease, box-shadow 0.2s ease, opacity 0.18s ease;
  box-shadow: 0 12px 30px rgba(122, 163, 255, 0.28);
  box-shadow: 0 12px 30px
    color-mix(in srgb, var(--accent) 32%, rgba(0, 0, 0, 0.38));
}

.panel-footer button:hover {
  transform: translateY(-2px);
  box-shadow: 0 18px 44px rgba(122, 163, 255, 0.32);
  box-shadow: 0 18px 44px
    color-mix(in srgb, var(--accent) 40%, rgba(0, 0, 0, 0.4));
}

@media (max-width: 640px) {
  .panel {
    padding: 26px;
  }
}
</style>
