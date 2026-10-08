import { execFileSync } from "node:child_process"
import { readFileSync, readdirSync } from "node:fs"
import path from "node:path"
import { fileURLToPath } from "node:url"
import tailwindcss from "@tailwindcss/vite"
import react from "@vitejs/plugin-react"
import { defineConfig } from "vitest/config"

const consoleDir = path.dirname(fileURLToPath(import.meta.url))
const backend = process.env.GPROXY_DEV_BACKEND ?? "http://127.0.0.1:8787"
const workspace = readFileSync(path.resolve(consoleDir, "../Cargo.toml"), "utf8")
const version = /\[workspace\.package\][\s\S]*?version\s*=\s*"([^"]+)"/.exec(workspace)?.[1] ?? "unknown"
const buildHash = process.env.GPROXY_BUILD_HASH
  ?? execFileSync("git", ["rev-parse", "--short=12", "HEAD"], { cwd: path.resolve(consoleDir, ".."), encoding: "utf8" }).trim()
const fontDir = path.resolve(consoleDir, "../docs/public/fonts")
const { stylesheet: fontStylesheet } = JSON.parse(readFileSync(path.join(fontDir, "manifest.json"), "utf8")) as { stylesheet: string }
const fontCss = readFileSync(path.join(fontDir, fontStylesheet), "utf8")
const fontFiles = new Set([fontStylesheet, ...Array.from(fontCss.matchAll(/url\(\.\/([a-f0-9]{64}\.woff2)\)/g), match => match[1])])

// `gproxy-host-axum` serves the bundle under `/console`, and only under it:
// `console::asset_name` strips exactly that prefix, so a document loaded from
// `/` that asked for `/assets/app.js` would 404. The base makes every emitted
// URL absolute under the prefix the host actually owns.
export default defineConfig({
  base: "/console/",
  define: {
    __GPROXY_VERSION__: JSON.stringify(version),
    __GPROXY_BUILD_HASH__: JSON.stringify(buildHash),
  },
  plugins: [react(), tailwindcss(), {
    name: "console-fonts",
    transformIndexHtml() {
      return [{ tag: "meta", attrs: { name: "gproxy-fonts", content: "/console/fonts/active.css" }, injectTo: "head" }]
    },
    generateBundle() {
      // Native hosts download font binaries only after an explicit user action.
      this.emitFile({ type: "asset", fileName: `fonts/${fontStylesheet}`, source: fontCss })
      this.emitFile({ type: "asset", fileName: "fonts/manifest.json", source: JSON.stringify({
        stylesheet: fontStylesheet,
        files: [...fontFiles].filter(file => file.endsWith(".woff2")),
      }) })
      for (const file of readdirSync(path.join(fontDir, "licenses"))) {
        this.emitFile({ type: "asset", fileName: `licenses/fonts/${file}`, source: readFileSync(path.join(fontDir, "licenses", file)) })
      }
    },
  }],
  resolve: { alias: { "@": path.join(consoleDir, "src") } },
  server: {
    proxy: {
      "/info": { target: backend, changeOrigin: true },
      "/console/fonts/": { target: backend, changeOrigin: true },
      // `changeOrigin` plus an explicit `origin` is what gets a dev request
      // past the same-origin check the host applies to unsafe methods.
      "/admin/api": { target: backend, changeOrigin: true, headers: { origin: backend } },
      "/portal/api": { target: backend, changeOrigin: true, headers: { origin: backend } },
      "/v1/oauth": { target: backend, changeOrigin: true, headers: { origin: backend } },
    },
  },
  build: { outDir: "dist", assetsDir: "assets", emptyOutDir: true },
  test: {
    environment: "jsdom",
    setupFiles: ["./src/test/setup.ts"],
    css: true,
  },
})
