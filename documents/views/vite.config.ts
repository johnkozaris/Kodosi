import { defineConfig } from "vite";

export default defineConfig({
  root: "kit",
  base: "./",
  build: {
    outDir: "../dist",
    emptyOutDir: true,
    target: "es2023",
    chunkSizeWarningLimit: 16_000,
  },
});
