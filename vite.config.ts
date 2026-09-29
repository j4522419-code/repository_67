import { defineConfig } from "vite";
import { svelte } from "@sveltejs/vite-plugin-svelte";

// The Tauri app loads the built UI from dist/ and, during `tauri dev`,
// from this dev server.
export default defineConfig({
  root: "ui",
  plugins: [svelte()],
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
  },
  build: {
    outDir: "../dist",
    emptyOutDir: true,
    // WebView2 on Windows 11 is evergreen Chromium.
    target: "chrome120",
  },
});
