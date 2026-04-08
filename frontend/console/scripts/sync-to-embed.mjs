import { cp, mkdir, readdir, rm, stat } from "node:fs/promises";
import path from "node:path";
import { fileURLToPath } from "node:url";

const scriptDir = path.dirname(fileURLToPath(import.meta.url));
const consoleDir = path.resolve(scriptDir, "..");
const distDir = path.join(consoleDir, "dist");
const embedDir = path.resolve(consoleDir, "../../apps/gproxy/web/console");

const distStats = await stat(distDir).catch(() => null);
if (!distStats?.isDirectory()) {
  throw new Error(`dist directory not found: ${distDir}`);
}

await mkdir(embedDir, { recursive: true });

for (const entry of await readdir(embedDir)) {
  if (entry === ".gitkeep") {
    continue;
  }
  await rm(path.join(embedDir, entry), { recursive: true, force: true });
}

for (const entry of await readdir(distDir)) {
  await cp(path.join(distDir, entry), path.join(embedDir, entry), {
    force: true,
    recursive: true,
  });
}
