import path from "node:path"
import { fileURLToPath } from "node:url"
import tailwindcss from "@tailwindcss/vite"
import react from "@vitejs/plugin-react"
import { defineConfig } from "vitest/config"

const consoleDir = path.dirname(fileURLToPath(import.meta.url))
const backend = process.env.GPROXY_DEV_BACKEND ?? "http://127.0.0.1:8787"

export default defineConfig({
  plugins: [react(), tailwindcss()],
  resolve: { alias: { "@": path.join(consoleDir, "src") } },
  server: {
    proxy: {
      "/admin": { target: backend, changeOrigin: true, headers: { origin: backend } },
    },
  },
  build: { outDir: "dist", assetsDir: "assets", emptyOutDir: true },
  test: {
    environment: "jsdom",
    setupFiles: ["./src/test/setup.ts"],
    css: true,
  },
})
