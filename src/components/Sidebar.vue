<script setup lang="ts">
import { computed, ref } from 'vue'

const props = defineProps<{
  current: 'compress' | 'settings'
}>()

const emit = defineEmits<{
  (e: 'navigate', key: 'compress' | 'settings'): void
}>()

const collapsed = ref(false)

const items = [{
  key: 'compress' as const,
  label: '压缩',
  icon: '🗜️'
}, {
  key: 'settings' as const,
  label: '设置',
  icon: '⚙️'
}]

function toggleCollapsed() {
  collapsed.value = !collapsed.value
}

function onSelect(key: 'compress' | 'settings') {
  emit('navigate', key)
}

const sidebarWidth = computed(() => collapsed.value ? '64px' : '220px')
</script>

<template>
  <aside class="sidebar" :class="{ collapsed }" :style="{ width: sidebarWidth }">
    <div class="brand" @click="collapsed = false">
      <span class="brand-icon">✨</span>
      <transition name="fade">
        <span v-if="!collapsed" class="brand-title">Yana</span>
      </transition>
    </div>

    <nav class="nav">
      <button
        v-for="item in items"
        :key="item.key"
        type="button"
        :class="['nav-item', { active: props.current === item.key }]"
        @click="onSelect(item.key)"
        :title="collapsed ? item.label : undefined"
      >
        <span class="icon">{{ item.icon }}</span>
        <transition name="fade">
          <span v-if="!collapsed" class="label">{{ item.label }}</span>
        </transition>
      </button>
    </nav>

    <button class="collapse" type="button" @click="toggleCollapsed">
      <span class="icon">{{ collapsed ? '⮞' : '⮜' }}</span>
      <transition name="fade">
        <span v-if="!collapsed" class="label">{{ collapsed ? '展开' : '折叠' }}</span>
      </transition>
    </button>
  </aside>
</template>

<style scoped>
.sidebar {
  display: flex;
  flex-direction: column;
  border-right: 1px solid rgba(0, 0, 0, 0.06);
  background: linear-gradient(180deg, rgba(21, 30, 63, 0.92), rgba(6, 10, 19, 0.94));
  color: #fff;
  transition: width 0.2s ease, box-shadow 0.2s ease;
  box-shadow: inset -1px 0 0 rgba(255, 255, 255, 0.05);
  position: relative;
  backdrop-filter: blur(12px);
}

.sidebar.collapsed {
  box-shadow: inset -1px 0 0 rgba(255, 255, 255, 0.08);
}

.brand {
  display: flex;
  align-items: center;
  gap: 12px;
  padding: 24px 18px 16px;
  font-size: 18px;
  font-weight: 600;
  letter-spacing: 0.8px;
  cursor: pointer;
  user-select: none;
}

.brand-icon {
  font-size: 22px;
}

.brand-title {
  text-transform: uppercase;
}

.nav {
  display: flex;
  flex-direction: column;
  gap: 6px;
  padding: 8px 10px;
  flex: 1;
}

.nav-item {
  display: flex;
  align-items: center;
  border: none;
  padding: 10px 12px;
  gap: 12px;
  border-radius: 10px;
  color: rgba(255, 255, 255, 0.88);
  background: transparent;
  transition: background 0.15s ease, transform 0.15s ease, color 0.15s ease;
}

.nav-item .icon {
  font-size: 18px;
}

.nav-item:hover {
  background: rgba(255, 255, 255, 0.12);
  transform: translateX(2px);
}

.nav-item.active {
  background: rgba(255, 255, 255, 0.2);
  color: #fff;
  box-shadow: 0 6px 14px rgba(0, 0, 0, 0.18);
}

.collapse {
  margin: 12px;
  border: 1px solid rgba(255, 255, 255, 0.18);
  background: rgba(255, 255, 255, 0.08);
  color: #fff;
  border-radius: 10px;
  padding: 10px 12px;
  display: flex;
  align-items: center;
  gap: 10px;
  transition: background 0.15s ease, border-color 0.15s ease;
}

.collapse:hover {
  background: rgba(255, 255, 255, 0.16);
  border-color: rgba(255, 255, 255, 0.34);
}

.fade-enter-active,
.fade-leave-active {
  transition: opacity 0.15s ease;
}

.fade-enter-from,
.fade-leave-to {
  opacity: 0;
}

.label {
  font-size: 14px;
}
</style>
