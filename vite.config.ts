import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";

// Tauri 要求固定端口且不清屏，便于 Rust 侧等待前端就绪。
export default defineConfig({
  plugins: [react()],
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
    watch: { ignored: ["**/src-tauri/**", "**/tests/**"] },
  },
  envPrefix: ["VITE_", "TAURI_"],
  build: {
    target: "es2021",
    minify: "esbuild",
    sourcemap: false,
  },
});
