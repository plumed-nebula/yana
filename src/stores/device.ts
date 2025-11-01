import { defineStore } from 'pinia';
import { ref } from 'vue';
import { platform } from '@tauri-apps/plugin-os';
import { warn as logWarn } from '@tauri-apps/plugin-log';

const MOBILE_PLATFORMS = new Set(['android', 'ios']);

export const useDeviceStore = defineStore('device', () => {
  const isMobile = ref(false);
  const currentPlatform = ref<string | null>(null);

  async function detectPlatform(): Promise<void> {
    try {
      // Tauri's platform() may return a promise; await to be safe
      const current = await platform();
      if (current) {
        currentPlatform.value = current.toLowerCase();
        isMobile.value = MOBILE_PLATFORMS.has(currentPlatform.value);
        return;
      }
    } catch (e) {
      // Log a warning via tauri plugin-log and fall back to false
      try {
        await logWarn(
          `[device] Failed to detect platform via Tauri OS plugin: ${String(e)}`
        );
      } catch {
        // ignore logging failures
      }
    }

    // Fallback: try simple userAgent sniffing in case we're running in a browser
    try {
      if (typeof navigator !== 'undefined' && navigator.userAgent) {
        const ua = navigator.userAgent.toLowerCase();
        const isMobileUA =
          ua.includes('android') ||
          ua.includes('iphone') ||
          ua.includes('ipad');
        if (ua.includes('android')) {
          currentPlatform.value = 'android';
        } else if (ua.includes('iphone') || ua.includes('ipad')) {
          currentPlatform.value = 'ios';
        }
        isMobile.value = isMobileUA;
        return;
      }
    } catch {
      // ignore
    }

    isMobile.value = false;
    currentPlatform.value = null;
  }

  return {
    isMobile,
    currentPlatform,
    detectPlatform,
  };
});
