<template>
  <div
    ref="container"
    class="global-select"
    :class="{ open, disabled: props.disabled, dark: isDark, light: !isDark }"
    @click="toggle"
  >
    <div class="select-trigger">
      <span>{{ selectedLabel }}</span>
      <ChevronDown class="icon" :size="20" />
    </div>
    <div v-if="open" class="options">
      <div
        v-for="opt in options"
        :key="opt.value"
        class="option"
        :class="{ selected: opt.value === internalValue }"
        @click.stop="select(opt.value)"
      >
        {{ opt.label }}
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref, computed, watch, onMounted, onBeforeUnmount } from 'vue';
import { ChevronDown } from 'lucide-vue-next';
import { useThemeStore } from '../stores/theme';

interface Option {
  label: string;
  value: string;
}

const props = defineProps<{
  modelValue: string | null;
  options: Option[];
  placeholder?: string;
  disabled?: boolean;
}>();
const emit = defineEmits<{ (e: 'update:modelValue', val: string): void }>();

const internalValue = ref(props.modelValue);
watch(
  () => props.modelValue,
  (val) => {
    internalValue.value = val;
  }
);
const selectedLabel = computed(() => {
  const found = props.options.find((o) => o.value === internalValue.value);
  if (found) return found.label;
  if (props.placeholder) return props.placeholder;
  return '';
});

const open = ref(false);
function toggle() {
  if (!props.disabled) {
    open.value = !open.value;
  }
}
function select(val: string) {
  internalValue.value = val;
  emit('update:modelValue', val);
  open.value = false;
}

// click outside to close
function onClickOutside(event: MouseEvent) {
  const target = event.target as HTMLElement;
  if (!container.value?.contains(target)) {
    open.value = false;
  }
}
const container = ref<HTMLElement | null>(null);
onMounted(() => document.addEventListener('click', onClickOutside));
onBeforeUnmount(() => document.removeEventListener('click', onClickOutside));

const themeStore = useThemeStore();
const isDark = computed(() => themeStore.isDark.value);
</script>

<style scoped>
.global-select {
  position: relative;
  width: 100%;
  cursor: pointer;
}
.select-trigger {
  display: flex;
  justify-content: space-between;
  align-items: center;
  padding: 8px 12px;
  border: 1px solid var(--surface-border);
  border-radius: 4px;
  background: var(--surface);
  color: var(--text-primary);
}
.global-select.dark .select-trigger {
  background: var(--surface-dark);
}
.global-select.light .select-trigger {
  background: var(--surface-light);
}
.icon {
  pointer-events: none;
  color: var(--text-primary);
}
.options {
  position: absolute;
  width: 100%;
  max-height: 200px;
  overflow-y: auto;
  margin-top: 4px;
  border: 1px solid var(--surface-border);
  border-radius: 4px;
  /* 使用面板背景并添加亚克力效果 */
  background: var(--surface-panel);
  backdrop-filter: blur(18px) saturate(1.08);
  opacity: 0.95;
  box-shadow: var(--shadow-strong);
  z-index: 100;
}
.option {
  padding: 8px 12px;
  cursor: pointer;
}
.option:hover {
  background: var(--surface-hover);
}
.global-select.dark .option:hover {
  background: var(--surface-hover-dark);
}
.option.selected {
  font-weight: bold;
}
.disabled .select-trigger {
  opacity: 0.6;
  cursor: not-allowed;
}
</style>
