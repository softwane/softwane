import { createI18n } from "vue-i18n";
import en from "./locales/en.json";
import zhCN from "./locales/zh-CN.json";
import {
  getAppLocale,
  getResolvedAppLocale,
  setAppLocale as persistAppLocale,
} from "../api/commands";

export const DEFAULT_LOCALE = "en";
export const SYSTEM_LOCALE_SENTINEL = "system";
export const SUPPORTED_LOCALES = ["en", "zh-CN"];

const messages = {
  en,
  "zh-CN": zhCN,
};

function resolveSupportedLocale(rawLocale) {
  if (!rawLocale) return DEFAULT_LOCALE;
  const normalized = String(rawLocale).trim().toLowerCase();
  if (normalized.startsWith("zh")) return "zh-CN";
  return "en";
}

async function detectInitialLocale() {
  const storedPreference = await getAppLocale().catch(() => null);
  if (storedPreference && storedPreference !== SYSTEM_LOCALE_SENTINEL) {
    return {
      activeLocale: resolveSupportedLocale(storedPreference),
      preference: resolveSupportedLocale(storedPreference),
    };
  }

  return {
    activeLocale: resolveSupportedLocale(
      await getResolvedAppLocale().catch(() => null),
    ),
    preference: SYSTEM_LOCALE_SENTINEL,
  };
}

export async function createAppI18n() {
  const { activeLocale, preference } = await detectInitialLocale();

  const i18n = createI18n({
    legacy: false,
    locale: activeLocale,
    fallbackLocale: DEFAULT_LOCALE,
    messages,
  });

  async function setAppLocale(nextPreference) {
    const normalizedPreference =
      nextPreference === SYSTEM_LOCALE_SENTINEL
        ? SYSTEM_LOCALE_SENTINEL
        : resolveSupportedLocale(nextPreference);

    let resolvedLocale = normalizedPreference;
    if (normalizedPreference === SYSTEM_LOCALE_SENTINEL) {
      resolvedLocale = resolveSupportedLocale(
        await getResolvedAppLocale().catch(() => null),
      );
    }

    i18n.global.locale.value = resolvedLocale;
    await persistAppLocale(normalizedPreference).catch(() => {});
  }

  return {
    i18n,
    initialPreference: preference,
    setAppLocale,
    resolveSupportedLocale,
  };
}
