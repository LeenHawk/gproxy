import path from "node:path"
import { fileURLToPath } from "node:url"
import tailwindcss from "@tailwindcss/vite"
import react from "@vitejs/plugin-react"
import { defineConfig } from "vitest/config"

const consoleDir = path.dirname(fileURLToPath(import.meta.url))
const backend = process.env.GPROXY_DEV_BACKEND ?? "http://127.0.0.1:8787"
const workspace = readFileSync(path.resolve(consoleDir, "../Cargo.toml"), "utf8")
const version = /\[workspace\.package\][\s\S]*?version\s*=\s*"([^"]+)"/.exec(workspace)?.[1] ?? "unknown"
const buildHash = process.env.GPROXY_BUILD_HASH ?? execFileSync("git", ["rev-parse", "--short=12", "HEAD"], { cwd: path.resolve(consoleDir, ".."), encoding: "utf8" }).trim()

export default defineConfig({
  define: {
    __GPROXY_VERSION__: JSON.stringify(version),
    __GPROXY_BUILD_HASH__: JSON.stringify(buildHash),
  },
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
import { execFileSync } from "node:child_process"
import { readFileSync } from "node:fs"
