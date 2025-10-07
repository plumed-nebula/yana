import { createApp } from 'vue';
import App from './App.vue';
import { attachConsole, info, error as logError } from '@tauri-apps/plugin-log';

function describeError(err: unknown): string {
  if (err instanceof Error) {
    const stack = err.stack ? `\n${err.stack}` : '';
    return `${err.name}: ${err.message}${stack}`;
  }
  return String(err);
}

// 立即执行的异步函数，确保日志尽早挂接
(async () => {
  try {
    // 将插件日志输出到 DevTools 控制台，便于开发期查看
    await attachConsole();
    await info('Frontend console attached to Tauri logging system.');
  } catch (err) {
    await logError(
      `Failed to attach frontend console to Tauri logger: ${describeError(err)}`
    );
  }
})();

createApp(App).mount('#app');
