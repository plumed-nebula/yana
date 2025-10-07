import { createApp } from 'vue';
import App from './App.vue';
import { attachConsole } from '@tauri-apps/plugin-log';

// 将插件日志输出到 DevTools 控制台，便于开发期查看
attachConsole();

createApp(App).mount('#app');
