import fs from "node:fs";
import path from "node:path";
import { execSync } from "node:child_process";
import { fileURLToPath } from "node:url";
import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import tailwindcss from "@tailwindcss/vite";

function parseBuildInfo() {
  const currentFile = fileURLToPath(import.meta.url);
  const currentDir = path.dirname(currentFile);
  const workspaceRoot = path.resolve(currentDir, "../..");
  const cargoToml = fs.readFileSync(path.join(workspaceRoot, "Cargo.toml"), "utf-8");
  const version = cargoToml.match(/^version\s*=\s*"([^"]+)"/m)?.[1] ?? "dev";

  let commit = "unknown";
  try {
    commit = execSync("git rev-parse --short HEAD", {
      cwd: workspaceRoot,
      stdio: ["ignore", "pipe", "ignore"],
    })
      .toString("utf-8")
      .trim();
  } catch {
    commit = "unknown";
  }

  return { version, commit };
}

const buildInfo = parseBuildInfo();

export default defineConfig({
  plugins: [react(), tailwindcss()],
  base: "/console/",
  define: {
    __APP_VERSION__: JSON.stringify(buildInfo.version),
    __APP_COMMIT__: JSON.stringify(buildInfo.commit),
  },
  build: {
    outDir: "dist",
    assetsDir: "assets",
    emptyOutDir: true,
    rolldownOptions: {
      output: {
        codeSplitting: {
          groups: [
            {
              name: "react-vendor",
              test: /node_modules[\\/](react|react-dom|scheduler)[\\/]/,
              priority: 30,
            },
            {
              name: "chart-vendor",
              test: /node_modules[\\/](recharts|react-smooth|d3-[^\\/]+|internmap)[\\/]/,
              priority: 20,
            },
            {
              name: "vendor",
              test: /node_modules[\\/]/,
              priority: 10,
            },
          ],
        },
      },
    },
  },
});
