import { ref, readonly, computed, watch } from 'vue';

export type ThemeMode = 'light' | 'dark';

const STORAGE_KEY = 'yana:theme-mode';

let singleton: ReturnType<typeof createThemeStore> | null = null;

function detectInitial(): ThemeMode {
  if (typeof window === 'undefined') {
    return 'light';
  }

  try {
    const stored = window.localStorage.getItem(STORAGE_KEY) as ThemeMode | null;
    if (stored === 'light' || stored === 'dark') {
      return stored;
    }
  } catch (error) {
    console.warn('[theme] failed to read stored mode', error);
  }

  if (typeof window !== 'undefined' && window.matchMedia) {
    try {
      if (window.matchMedia('(prefers-color-scheme: dark)').matches) {
        return 'dark';
      }
    } catch (error) {
      console.warn('[theme] matchMedia detection failed', error);
    }
  }

  return 'light';
}

function applyTheme(mode: ThemeMode) {
  if (typeof document === 'undefined') {
    return;
  }
  const root = document.documentElement;
  root.dataset.theme = mode;
  root.classList.toggle('theme-dark', mode === 'dark');
  root.classList.toggle('theme-light', mode === 'light');
  root.style.setProperty('color-scheme', mode);
}

function createThemeStore() {
  const mode = ref<ThemeMode>(detectInitial());
  const isDark = computed(() => mode.value === 'dark');

  if (typeof window !== 'undefined') {
    applyTheme(mode.value);

    watch(
      mode,
      (value) => {
        applyTheme(value);
        try {
          window.localStorage.setItem(STORAGE_KEY, value);
        } catch (error) {
          console.warn('[theme] failed to persist mode', error);
        }
      },
      { immediate: true }
    );

    if (window.matchMedia) {
      const media = window.matchMedia('(prefers-color-scheme: dark)');
      const listener = (event: MediaQueryListEvent) => {
        try {
          const stored = window.localStorage.getItem(STORAGE_KEY);
          if (stored === null) {
            mode.value = event.matches ? 'dark' : 'light';
          }
        } catch (error) {
          console.warn(
            '[theme] failed reading stored mode during media update',
            error
          );
        }
      };
      if (typeof media.addEventListener === 'function') {
        media.addEventListener('change', listener);
      } else {
        media.addListener(listener);
      }
    }
  }

  function toggleTheme() {
    mode.value = mode.value === 'dark' ? 'light' : 'dark';
  }

  function setTheme(value: ThemeMode) {
    mode.value = value;
  }

  return {
    mode: readonly(mode),
    isDark,
    toggleTheme,
    setTheme,
  };
}

export function useThemeStore() {
  if (!singleton) {
    singleton = createThemeStore();
  }
  return singleton;
}
