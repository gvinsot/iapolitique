import { fileURLToPath, URL } from "node:url";

import react from "@vitejs/plugin-react";
import { defineConfig } from "vitest/config";

// Backend ports are defined in AGENTS.md §13.1: public API on 8080, admin API on 8081.
// Order matters: the more specific /api/admin prefix must be matched first.
const API_PUBLIC_TARGET = "http://localhost:8080";
const API_ADMIN_TARGET = "http://localhost:8081";

export default defineConfig({
  plugins: [react()],
  resolve: {
    alias: {
      "@": fileURLToPath(new URL("./src", import.meta.url)),
    },
  },
  server: {
    port: 5173,
    strictPort: true,
    proxy: {
      "/api/admin": { target: API_ADMIN_TARGET, changeOrigin: false },
      "/api": { target: API_PUBLIC_TARGET, changeOrigin: false },
    },
  },
  preview: {
    port: 4173,
    strictPort: true,
  },
  build: {
    target: "es2022",
    sourcemap: false,
    assetsInlineLimit: 0,
  },
  test: {
    environment: "jsdom",
    setupFiles: ["./src/test/setup.ts"],
    include: ["src/**/*.test.{ts,tsx}"],
    restoreMocks: true,
    unstubGlobals: true,
    // CSS is not processed in tests, except the design tokens read by the contrast test.
    css: { include: [/tokens\.css/] },
  },
});
