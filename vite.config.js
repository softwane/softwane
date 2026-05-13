import { defineConfig } from "vite";
import vue from "@vitejs/plugin-vue";

export default defineConfig({
  plugins: [vue()],
  clearScreen: false,
  server: {
    host: "0.0.0.0",
    port: 1420,
    strictPort: true
  },
  define: {
    // 剔除 Vue 源码中的 Options API 支持
    __VUE_OPTIONS_API__: false, 
  },
  build: {
    // 直接面向现代引擎，不进行任何语法降级
    target: ['es2021', 'chrome100', 'safari15'],
    // 彻底禁用 polyfill
    modulePreload: {
      polyfill: false,
    }, 
  }
});
