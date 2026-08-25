import { readFile } from "node:fs/promises"
import path from "node:path"
import { fileURLToPath } from "node:url"

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "../src/locales")
const locales = ["en", "zh-CN", "zh-TW"]
const flatten = (value, prefix = "") =>
  Object.entries(value).flatMap(([key, child]) => {
    const path = prefix ? `${prefix}.${key}` : key
    return child && typeof child === "object" ? flatten(child, path) : [path]
  })

const keys = await Promise.all(
  locales.map(async (locale) => {
    const value = JSON.parse(await readFile(path.join(root, `${locale}.json`), "utf8"))
    return [locale, new Set(flatten(value))]
  }),
)
const reference = keys[0][1]
for (const [locale, current] of keys.slice(1)) {
  const missing = [...reference].filter((key) => !current.has(key))
  const extra = [...current].filter((key) => !reference.has(key))
  if (missing.length || extra.length) {
    throw new Error(`${locale} locale drift; missing=${missing.join(",")} extra=${extra.join(",")}`)
  }
}
