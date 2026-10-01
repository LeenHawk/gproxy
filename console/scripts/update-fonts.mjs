// Font URLs are part of released clients. Add new content-addressed files;
// keep previous files so older installations can still fill their cache.
import { createHash } from "node:crypto"
import console from "node:console"
import { mkdirSync, readFileSync, writeFileSync } from "node:fs"
import { createRequire } from "node:module"
import path from "node:path"
import { fileURLToPath, URL } from "node:url"

const require = createRequire(import.meta.url)
const target = fileURLToPath(new URL("../../docs/public/fonts/", import.meta.url))
const families = ["noto-sans", "noto-sans-sc", "noto-sans-tc", "noto-sans-mono"]
const packages = {}
const files = new Set()
const digest = bytes => createHash("sha256").update(bytes).digest("hex")
mkdirSync(path.join(target, "licenses"), { recursive: true })

const css = families.map(family => {
  const name = `@fontsource-variable/${family}`
  const root = path.dirname(require.resolve(`${name}/package.json`))
  const { version } = JSON.parse(readFileSync(path.join(root, "package.json"), "utf8"))
  packages[name] = version
  writeFileSync(path.join(target, "licenses", `${family}-${version}.txt`), readFileSync(path.join(root, "LICENSE")))
  const source = readFileSync(path.join(root, "index.css"), "utf8")
    .replaceAll(/url\(\.\/files\/([^)]+\.woff2)\)/g, (_, file) => {
      const bytes = readFileSync(path.join(root, "files", file))
      const filename = `${digest(bytes)}.woff2`
      writeFileSync(path.join(target, filename), bytes)
      files.add(filename)
      return `url(./${filename})`
    })
  return `/* ${name}@${version}; OFL-1.1; licenses/${family}-${version}.txt */\n${source}`
}).join("\n")

const stylesheet = `${digest(css)}.css`
writeFileSync(path.join(target, stylesheet), css)
writeFileSync(path.join(target, "manifest.json"), JSON.stringify({ stylesheet, packages }, null, 2) + "\n")
console.log(`Published ${files.size} font files and ${stylesheet} to docs/public/fonts/`)
