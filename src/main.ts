import { createApp } from 'vue';
import App from './App.vue';
import { attachConsole } from '@tauri-apps/plugin-log';

// 立即执行的异步函数，确保日志尽早挂接
(async () => {
  try {
    // 将插件日志输出到 DevTools 控制台，便于开发期查看
    await attachConsole();
    console.log('Frontend console attached to Tauri logging system.');
  } catch (err) {
    console.error('Failed to attach frontend console to Tauri logger:', err);
  }
})();

createApp(App).mount('#app');
