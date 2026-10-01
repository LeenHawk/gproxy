import { readFile, writeFile } from "node:fs/promises"
import process from "node:process"

const filename = process.argv[2]
if (!filename) throw new Error("usage: node prepare-edge-fonts.mjs <index.html>")
const html = await readFile(filename, "utf8")
const rewritten = html.replace(
  /(<meta name="gproxy-fonts" content=")\/console\/fonts\//,
  "$1https://gproxy.leenhawk.com/fonts/",
)
if (html === rewritten) throw new Error("missing native font metadata in console document")
await writeFile(filename, rewritten)
