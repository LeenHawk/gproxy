// Edge serves fonts as same-origin static assets, outside native packages.
import { cp } from "node:fs/promises"
import path from "node:path"
import process from "node:process"
import { fileURLToPath, URL } from "node:url"
import { fontPackage } from "./font-package.mjs"

const target = process.argv[2]
if (!target) throw new Error("usage: node prepare-edge-fonts.mjs <console-directory>")
const fonts = fontPackage(fileURLToPath(new URL("..", import.meta.url)))
for (const file of fonts.files) await cp(file.local, path.join(target, "fonts", file.name))
await cp(path.join(target, "fonts", fonts.stylesheet), path.join(target, "fonts/active.css"))
