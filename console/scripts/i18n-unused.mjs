import { readFile, readdir, writeFile } from "node:fs/promises"
import path from "node:path"
import process from "node:process"
import { fileURLToPath } from "node:url"
import ts from "typescript"

const consoleRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..")
const localeRoot = path.join(consoleRoot, "src/locales")
const locales = ["en", "zh-CN", "zh-TW"]
const domains = ["common", "identity", "observability", "portal", "pricing", "providers", "routes", "rules", "settings", "update"]

async function sourceFiles(directory) {
  const entries = await readdir(directory, { withFileTypes: true })
  const files = await Promise.all(entries.map(async (entry) => {
    const target = path.join(directory, entry.name)
    if (entry.isDirectory()) return ["generated", "locales"].includes(entry.name) ? [] : sourceFiles(target)
    return /\.[cm]?[jt]sx?$/.test(entry.name) ? [target] : []
  }))
  return files.flat()
}

const flatten = (value, prefix = "") => Object.entries(value).flatMap(([key, child]) => {
  const name = prefix ? `${prefix}.${key}` : key
  return child && typeof child === "object" ? flatten(child, name) : [[name, child]]
})

function prune(value, used, prefix = "") {
  return Object.fromEntries(Object.entries(value).flatMap(([key, child]) => {
    const name = prefix ? `${prefix}.${key}` : key
    if (child && typeof child === "object") {
      const nested = prune(child, used, name)
      return Object.keys(nested).length ? [[key, nested]] : []
    }
    return used.has(name) ? [[key, child]] : []
  }))
}

function collectArgument(node, exact, prefixes) {
  if (ts.isStringLiteral(node) || ts.isNoSubstitutionTemplateLiteral(node)) {
    exact.add(node.text)
    return true
  }
  if (ts.isTemplateExpression(node) && node.head.text) {
    prefixes.add(node.head.text)
    return true
  }
  if (ts.isConditionalExpression(node)) {
    return collectArgument(node.whenTrue, exact, prefixes)
      && collectArgument(node.whenFalse, exact, prefixes)
  }
  if (ts.isParenthesizedExpression(node) || ts.isAsExpression(node) || ts.isNonNullExpression(node)) {
    return collectArgument(node.expression, exact, prefixes)
  }
  return false
}

function translationKeys(file, source, exact, prefixes, unsupported) {
  const tree = ts.createSourceFile(
    file,
    source,
    ts.ScriptTarget.Latest,
    true,
    ts.getScriptKindFromFileName(file),
  )
  const visit = (node) => {
    if (ts.isCallExpression(node)
      && ts.isIdentifier(node.expression)
      && node.expression.text === "t"
      && node.arguments[0]
      && !collectArgument(node.arguments[0], exact, prefixes)) {
      const position = tree.getLineAndCharacterOfPosition(node.arguments[0].getStart(tree))
      unsupported.push(`${path.relative(consoleRoot, file)}:${position.line + 1}`)
    }
    ts.forEachChild(node, visit)
  }
  visit(tree)
}

const exact = new Set()
const prefixes = new Set()
const unsupported = []
for (const file of await sourceFiles(path.join(consoleRoot, "src"))) {
  translationKeys(file, await readFile(file, "utf8"), exact, prefixes, unsupported)
}
if (unsupported.length) {
  throw new Error(`translation keys must be static strings, conditionals, or prefixed templates: ${unsupported.join(", ")}`)
}

async function localeDomains(locale) {
  return Promise.all(domains.map(async (domain) => [
    domain,
    JSON.parse(await readFile(path.join(localeRoot, locale, `${domain}.json`), "utf8")),
  ]))
}

const englishDomains = await localeDomains("en")
const english = Object.assign({}, ...englishDomains.map(([, value]) => value))
const keys = flatten(english).map(([key]) => key)
const known = new Set(keys)
const missing = [...exact].filter((key) => !known.has(key)).sort()
if (missing.length) throw new Error(`missing locale keys: ${missing.join(", ")}`)

const used = new Set(keys.filter((key) => exact.has(key) || [...prefixes].some((prefix) => key.startsWith(prefix))))
const unused = keys.filter((key) => !used.has(key))

if (process.argv.includes("--write")) {
  for (const locale of locales) {
    for (const [domain, value] of await localeDomains(locale)) {
      const file = path.join(localeRoot, locale, `${domain}.json`)
      await writeFile(file, `${JSON.stringify(prune(value, used), null, 2)}\n`)
    }
  }
} else if (unused.length) {
  throw new Error(`unused locale keys: ${unused.join(", ")}`)
}
