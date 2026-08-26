import { readFile, readdir } from "node:fs/promises"
import path from "node:path"
import { fileURLToPath } from "node:url"

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "../src/locales")
const locales = ["en", "zh-CN", "zh-TW"]
const domains = ["common", "identity", "observability", "portal", "pricing", "providers", "routes", "rules", "settings", "update"]
const flatten = (value, prefix = "") =>
  Object.entries(value).flatMap(([key, child]) => {
    const path = prefix ? `${prefix}.${key}` : key
    return child && typeof child === "object" ? flatten(child, path) : [path]
  })

for (const locale of locales) {
  const files = (await readdir(path.join(root, locale))).filter((file) => file.endsWith(".json")).map((file) => file.slice(0, -5)).sort()
  if (files.join(",") !== [...domains].sort().join(",")) {
    throw new Error(`${locale} domain drift; expected=${domains.join(",")} actual=${files.join(",")}`)
  }
}

for (const domain of domains) {
  const keys = await Promise.all(locales.map(async (locale) => {
    const value = JSON.parse(await readFile(path.join(root, locale, `${domain}.json`), "utf8"))
    return [locale, new Set(flatten(value))]
  }))
  const reference = keys[0][1]
  for (const [locale, current] of keys.slice(1)) {
    const missing = [...reference].filter((key) => !current.has(key))
    const extra = [...current].filter((key) => !reference.has(key))
    if (missing.length || extra.length) {
      throw new Error(`${locale}/${domain} locale drift; missing=${missing.join(",")} extra=${extra.join(",")}`)
    }
  }
}
