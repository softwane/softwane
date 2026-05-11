import { getCurrentWindow } from "@tauri-apps/api/window";
import { computed, onMounted, onUnmounted, ref, watch } from "vue";

const STORAGE_KEY = "softwane.appearance";
const DEFAULT_THEME_MODE = "auto";
const MEDIA_QUERY = "(prefers-color-scheme: dark)";

function normalizeThemeMode(value) {
  switch (value) {
    case "dark":
    case "light":
    case "auto":
      return value;
    default:
      return DEFAULT_THEME_MODE;
  }
}

function normalizeResolvedTheme(value) {
  return value === "dark" ? "dark" : "light";
}

function hasTauriRuntime() {
  return typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
}

function readStoredThemeMode() {
  if (typeof window === "undefined") {
    return DEFAULT_THEME_MODE;
  }

  try {
    const raw = window.localStorage.getItem(STORAGE_KEY);

    if (!raw) {
      return DEFAULT_THEME_MODE;
    }

    const saved = JSON.parse(raw);
    return normalizeThemeMode(saved.themeMode);
  } catch {
    window.localStorage.removeItem(STORAGE_KEY);
    return DEFAULT_THEME_MODE;
  }
}

function readPreferredSystemTheme() {
  if (typeof window === "undefined" || typeof window.matchMedia !== "function") {
    return "light";
  }

  return window.matchMedia(MEDIA_QUERY).matches ? "dark" : "light";
}

function applyDocumentTheme(theme, mode) {
  if (typeof document === "undefined") {
    return;
  }

  const root = document.documentElement;
  root.dataset.theme = theme;
  root.dataset.themeMode = mode;
  root.style.colorScheme = theme;
}

export function useAppearance() {
  const themeMode = ref(readStoredThemeMode());
  const systemTheme = ref(readPreferredSystemTheme());
  const resolvedTheme = computed(() => (
    themeMode.value === "auto" ? systemTheme.value : themeMode.value
  ));
  let mediaQueryList = null;
  let unlistenThemeChange = null;

  function setThemeMode(value) {
    themeMode.value = normalizeThemeMode(value);
  }

  function persistThemeMode() {
    if (typeof window === "undefined") {
      return;
    }

    window.localStorage.setItem(
      STORAGE_KEY,
      JSON.stringify({
        themeMode: themeMode.value
      })
    );
  }

  async function syncNativeSystemTheme() {
    if (!hasTauriRuntime()) {
      return;
    }

    try {
      const theme = await getCurrentWindow().theme();

      if (theme) {
        systemTheme.value = normalizeResolvedTheme(theme);
      }
    } catch {
      // Ignore theme lookup failures and keep the current resolved theme.
    }
  }

  async function syncNativeThemePreference() {
    if (!hasTauriRuntime()) {
      return;
    }

    try {
      await getCurrentWindow().setTheme(
        themeMode.value === "auto" ? null : themeMode.value
      );
    } catch {
      // Ignore runtime permission/platform failures and keep CSS theme switching working.
    }
  }

  onMounted(async () => {
    if (hasTauriRuntime()) {
      await syncNativeSystemTheme();

      try {
        unlistenThemeChange = await getCurrentWindow().onThemeChanged(({ payload }) => {
          systemTheme.value = normalizeResolvedTheme(payload);
        });
      } catch {
        unlistenThemeChange = null;
      }
    } else if (typeof window !== "undefined" && typeof window.matchMedia === "function") {
      mediaQueryList = window.matchMedia(MEDIA_QUERY);
      const handleMediaThemeChange = (event) => {
        systemTheme.value = event.matches ? "dark" : "light";
      };

      if (typeof mediaQueryList.addEventListener === "function") {
        mediaQueryList.addEventListener("change", handleMediaThemeChange);
        unlistenThemeChange = () => mediaQueryList?.removeEventListener("change", handleMediaThemeChange);
      } else if (typeof mediaQueryList.addListener === "function") {
        mediaQueryList.addListener(handleMediaThemeChange);
        unlistenThemeChange = () => mediaQueryList?.removeListener(handleMediaThemeChange);
      }
    }

    await syncNativeThemePreference();
  });

  onUnmounted(() => {
    if (typeof unlistenThemeChange === "function") {
      unlistenThemeChange();
      unlistenThemeChange = null;
    }

    mediaQueryList = null;
  });

  watch(themeMode, () => {
    persistThemeMode();
    void syncNativeThemePreference();
  }, { immediate: true });

  watch(
    resolvedTheme,
    (theme) => {
      applyDocumentTheme(normalizeResolvedTheme(theme), themeMode.value);
    },
    { immediate: true }
  );

  watch(themeMode, (mode) => {
    applyDocumentTheme(normalizeResolvedTheme(resolvedTheme.value), mode);
  });

  return {
    resolvedTheme,
    setThemeMode,
    systemTheme,
    themeMode
  };
}
