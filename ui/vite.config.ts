import { defineConfig } from "vite";
import vue from "@vitejs/plugin-vue";
import pkg from "./package.json" with { type: "json" };

export default defineConfig({
  plugins: [vue()],
  define: {
    // ui/package.json is the single version source (installers read it too).
    __APP_VERSION__: JSON.stringify(pkg.version),
  },
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
    // Cargo output includes live WebView2 data with exclusively locked files.
    // Rust changes are watched by Tauri, not the frontend dev server.
    watch: { ignored: ["**/src-tauri/**"] },
  },
  envPrefix: ["VITE_", "TAURI_ENV_*"],
  build: { target: "chrome105" },
});
