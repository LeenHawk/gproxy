// The console's optional font pack, built from the pinned Fontsource
// devDependencies. The stylesheet names every file by its SHA-256, which is how
// native hosts store and verify it; each file also records the npm path a
// public CDN serves it under, which is where native hosts download it from.
import { createHash } from "node:crypto"
import { readFileSync } from "node:fs"
import { createRequire } from "node:module"
import path from "node:path"

const families = ["noto-sans", "noto-sans-sc", "noto-sans-tc", "noto-sans-mono"]
const digest = bytes => createHash("sha256").update(bytes).digest("hex")

export function fontPackage(consoleDir) {
  const require = createRequire(path.join(consoleDir, "package.json"))
  const packages = {}
  const files = new Map()
  const licenses = []
  const css = families.map(family => {
    const name = `@fontsource-variable/${family}`
    const root = path.dirname(require.resolve(`${name}/package.json`))
    const { version } = JSON.parse(readFileSync(path.join(root, "package.json"), "utf8"))
    packages[name] = version
    const license = `${family}-${version}.txt`
    licenses.push({ fileName: license, source: readFileSync(path.join(root, "LICENSE")) })
    const source = readFileSync(path.join(root, "index.css"), "utf8")
      .replaceAll(/url\(\.\/files\/([^)]+\.woff2)\)/g, (_, file) => {
        const local = path.join(root, "files", file)
        const hashed = `${digest(readFileSync(local))}.woff2`
        files.set(hashed, { name: hashed, package: name, path: `files/${file}`, local })
        return `url(./${hashed})`
      })
    return `/* ${name}@${version}; OFL-1.1; licenses/fonts/${license} */\n${source}`
  }).join("\n")
  return { stylesheet: `${digest(css)}.css`, css, packages, files: [...files.values()], licenses }
}
