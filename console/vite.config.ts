import fs from "node:fs";
import path from "node:path";
import { execSync } from "node:child_process";
import { fileURLToPath } from "node:url";
import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import tailwindcss from "@tailwindcss/vite";
import { tanstackRouter } from "@tanstack/router-plugin/vite";

const consoleDir = path.dirname(fileURLToPath(import.meta.url));
const v2Root = path.resolve(consoleDir, "..");

function buildInfo() {
  const cargo = fs.readFileSync(path.join(v2Root, "Cargo.toml"), "utf-8");
  const version = cargo.match(/^version\s*=\s*"([^"]+)"/m)?.[1] ?? "dev";
  let commit = "unknown";
  try {
    commit = execSync("git rev-parse --short HEAD", {
      cwd: v2Root,
      stdio: ["ignore", "pipe", "ignore"],
    })
      .toString("utf-8")
      .trim();
  } catch {
    /* not a git checkout */
  }
  return { version, commit };
}

const info = buildInfo();
const BACKEND = "http://127.0.0.1:8787";
// Backend CSRF requires Origin authority == Host on state-changing requests, so
// the dev proxy rewrites Origin to the backend's own authority.
const proxyEntry = { target: BACKEND, changeOrigin: true, headers: { origin: BACKEND } };

export default defineConfig({
  plugins: [tanstackRouter({ target: "react", autoCodeSplitting: true }), react(), tailwindcss()],
  base: "/console/",
  resolve: { alias: { "@": path.join(consoleDir, "src") } },
  define: {
    __APP_VERSION__: JSON.stringify(info.version),
    __APP_COMMIT__: JSON.stringify(info.commit),
  },
  server: {
    proxy: {
      "/admin": proxyEntry,
      "/healthz": proxyEntry,
      "/version": proxyEntry,
      "/metrics": proxyEntry,
    },
  },
  build: {
    outDir: "dist",
    assetsDir: "assets",
    emptyOutDir: true,
    rolldownOptions: {
      output: {
        codeSplitting: {
          groups: [
            { name: "react-vendor", test: /node_modules[\\/](react|react-dom|scheduler)[\\/]/, priority: 30 },
            { name: "chart-vendor", test: /node_modules[\\/](recharts|react-smooth|d3-[^\\/]+|victory-vendor|internmap)[\\/]/, priority: 20 },
            { name: "vendor", test: /node_modules[\\/]/, priority: 10 },
          ],
        },
      },
    },
  },
});
