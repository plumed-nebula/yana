<script setup lang="ts">
import { computed, ref } from 'vue'
import Sidebar from './components/Sidebar.vue'
import SettingsView from './views/SettingsView.vue'
import CompressView from './views/CompressView.vue'

const current = ref<'compress' | 'settings'>('compress')
function onNavigate(key: 'compress' | 'settings') { current.value = key }
const activeComponent = computed(() => current.value === 'compress' ? CompressView : SettingsView)
</script>

<template>
  <div class="layout">
    <Sidebar :current="current" @navigate="onNavigate" />
    <section class="content">
      <component :is="activeComponent" />
    </section>
  </div>
  
</template>

<style scoped>
.layout {
  display: flex;
  height: 100vh;
  background: linear-gradient(135deg, #f4f6fb 0%, #dde2f3 40%, #f4f6fb 100%);
}

.content {
  flex: 1;
  overflow: auto;
  padding: 32px 40px;
  display: flex;
  justify-content: center;
  align-items: flex-start;
}
</style>

<style>
:root { color: #0f0f0f; background-color: #f6f6f6; }
@media (prefers-color-scheme: dark) {
  :root { color: #f6f6f6; background-color: #2f2f2f; }
}
button { cursor: pointer; }
</style>