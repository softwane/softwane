import { createApp } from "vue";
import App from "./App.vue";
import "./styles.css";
import { createAppI18n } from "./i18n";
import { setTranslationResolver } from "./i18n/runtime";

async function bootstrap() {
  const { i18n, initialPreference, setAppLocale } = await createAppI18n();
  setTranslationResolver((key, params) => i18n.global.t(key, params));

  createApp(App, {
    initialLocalePreference: initialPreference,
    setAppLocale,
  }).use(i18n).mount("#app");
}

bootstrap();
