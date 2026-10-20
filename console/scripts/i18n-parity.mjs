import { readFile, readdir } from "node:fs/promises"
import path from "node:path"
import { fileURLToPath } from "node:url"

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "../src/locales")
const locales = ["en", "ru", "zh-CN", "zh-TW"]
const domains = ["common", "navigation", "pages"]
const flatten = (value, prefix = "") =>
  Object.entries(value).flatMap(([key, child]) => {
    const path = prefix ? `${prefix}.${key}` : key
    return child && typeof child === "object" ? flatten(child, path) : [[path, child]]
  })
const pluralBase = key => key.replace(/_(zero|one|two|few|many|other)$/, "")
const placeholders = value => [...value.matchAll(/{{\s*([^{}]+?)\s*}}/g)].map(match => match[1]).sort().join(",")

for (const locale of locales) {
  const files = (await readdir(path.join(root, locale))).filter((file) => file.endsWith(".json")).map((file) => file.slice(0, -5)).sort()
  if (files.join(",") !== [...domains].sort().join(",")) {
    throw new Error(`${locale} domain drift; expected=${domains.join(",")} actual=${files.join(",")}`)
  }
}

for (const domain of domains) {
  const keys = await Promise.all(locales.map(async (locale) => {
    const value = JSON.parse(await readFile(path.join(root, locale, `${domain}.json`), "utf8"))
    return [locale, new Map(flatten(value))]
  }))
  const reference = keys[0][1]
  for (const [locale, current] of keys.slice(1)) {
    const missing = [...reference.keys()].filter((key) => !current.has(key))
    const extra = [...current.keys()].filter((key) => !reference.has(key)
      && !reference.get(pluralBase(key))?.includes("{{count}}"))
    if (missing.length || extra.length) {
      throw new Error(`${locale}/${domain} locale drift; missing=${missing.join(",")} extra=${extra.join(",")}`)
    }
    const pluralKeys = new Set()
    for (const [key, value] of current) {
      const base = reference.has(key) ? key : pluralBase(key)
      if (typeof value !== "string" || !value.trim() || placeholders(value) !== placeholders(reference.get(base))) {
        throw new Error(`${locale}/${domain}/${key}: empty translation or interpolation drift`)
      }
      if (base !== key) pluralKeys.add(base)
    }
    for (const base of pluralKeys) {
      for (const category of new Intl.PluralRules(locale).resolvedOptions().pluralCategories) {
        if (!current.has(`${base}_${category}`)) {
          throw new Error(`${locale}/${domain}/${base}: missing plural category ${category}`)
        }
      }
    }
  }
}
