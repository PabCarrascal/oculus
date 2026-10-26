import tailwindcss from "@tailwindcss/vite";
import react from "@vitejs/plugin-react";
import { defineConfig } from "vitest/config";

const host = process.env.TAURI_DEV_HOST;

export default defineConfig({
  plugins: [react(), tailwindcss()],
  // Tauri muestra sus propios errores de Rust; que Vite no borre la consola.
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
    host: host || false,
    watch: { ignored: ["**/src-tauri/**"] },
  },
  envPrefix: ["VITE_", "TAURI_ENV_"],
  build: {
    // Mínimos de los webviews soportados: WebView2 (Chromium) y WKWebView/WebKitGTK.
    target: process.env.TAURI_ENV_PLATFORM === "windows" ? "chrome111" : "safari16",
    sourcemap: !!process.env.TAURI_ENV_DEBUG,
  },
  test: {
    environment: "jsdom",
    setupFiles: ["./src/test/setup.ts"],
  },
});
