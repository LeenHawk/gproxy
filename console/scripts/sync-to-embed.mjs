import { cp, mkdir, readdir, rm, stat } from "node:fs/promises"
import path from "node:path"
import process from "node:process"
import { fileURLToPath } from "node:url"

const scriptDir = path.dirname(fileURLToPath(import.meta.url))
const consoleDir = path.resolve(scriptDir, "..")
const distDir = path.join(consoleDir, "dist")
const embedDir = path.resolve(consoleDir, "../crates/gproxy-host-axum/assets/web")
// The Tauri window serves `ui/` at its root. The bundle is built for `/console/`,
// so it goes under `ui/console/`, and its `index.html` also becomes `ui/index.html`:
// the window opens `/`, and Tauri answers any path it has no file for with that page.
const shellDir = path.resolve(consoleDir, "../crates/gproxy-host-tauri/ui")

const distStats = await stat(distDir).catch(() => null)
if (!distStats?.isDirectory()) throw new Error(`dist directory not found: ${distDir}`)

async function empty(dir) {
  await mkdir(dir, { recursive: true })
  for (const entry of await readdir(dir)) {
    if (entry !== ".gitkeep") await rm(path.join(dir, entry), { recursive: true, force: true })
  }
}

await empty(embedDir)
await cp(distDir, embedDir, { recursive: true, force: true })

await empty(shellDir)
await cp(distDir, path.join(shellDir, "console"), { recursive: true, force: true })
await cp(path.join(distDir, "index.html"), path.join(shellDir, "index.html"), { force: true })

// Store applications serve the checked-in, freely licensed font assets locally.
// The ordinary server/desktop path keeps its existing on-demand font cache.
if (process.env.VITE_GPROXY_BUNDLED_FONTS === "1") {
  await cp(path.resolve(consoleDir, "../docs/public/fonts"), path.join(shellDir, "console/fonts"), { recursive: true })
  const licenses = path.join(shellDir, "console/licenses")
  await mkdir(licenses, { recursive: true })
  await cp(path.resolve(consoleDir, "../LICENSE"), path.join(licenses, "GPROXY-AGPL.txt"))
  await cp(path.resolve(consoleDir, "../crates/gproxy-tokenizer/assets/tokenizers/LICENSE"), path.join(licenses, "DeepSeek-MIT.txt"))
  await cp(path.resolve(consoleDir, "../crates/gproxy-tokenizer/THIRD_PARTY_NOTICES.md"), path.join(licenses, "tokenizer-notices.md"))
}
