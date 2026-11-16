import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";

export default defineConfig({
  plugins: [react()],
  base: "./",
  build: { outDir: "dist", emptyOutDir: true, chunkSizeWarningLimit: 900 },
  server: { port: 5183, proxy: { "/api": "http://127.0.0.1:7790", "/file": "http://127.0.0.1:7790" } },
});
