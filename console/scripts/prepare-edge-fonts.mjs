// Edge serves fonts as same-origin static assets, outside native packages.
import { cp, readFile } from "node:fs/promises"
import path from "node:path"
import process from "node:process"
import { fileURLToPath, URL } from "node:url"

const target = process.argv[2]
if (!target) throw new Error("usage: node prepare-edge-fonts.mjs <console-directory>")
const source = fileURLToPath(new URL("../../docs/public/fonts/", import.meta.url))
const manifest = JSON.parse(await readFile(path.join(target, "fonts/manifest.json"), "utf8"))
for (const file of manifest.files) await cp(path.join(source, file), path.join(target, "fonts", file))
await cp(path.join(target, "fonts", manifest.stylesheet), path.join(target, "fonts/active.css"))
