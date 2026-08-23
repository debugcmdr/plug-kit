import { defineConfig } from "vite";
import vue from "@vitejs/plugin-vue";
import { readFileSync } from "node:fs";

// @ts-expect-error process is a nodejs global
const host = process.env.TAURI_DEV_HOST;

// 应用版本注入(设置面板「关于」展示):以 package.json 为准,
// 发布时与 tauri.conf.json 同步升版即可,无需再维护前端版本常量。
const appVersion = JSON.parse(readFileSync(new URL("./package.json", import.meta.url), "utf-8")).version;

export default defineConfig(async () => ({
  plugins: [vue()],
  clearScreen: false,
  define: {
    "import.meta.env.VITE_APP_VERSION": JSON.stringify(appVersion),
  },
  server: {
    port: 1420,
    strictPort: true,
    host: host || false,
    hmr: host
      ? { protocol: "ws", host, port: 1421 }
      : undefined,
    watch: {
      ignored: ["**/src-tauri/**"],
    },
  },
}));
