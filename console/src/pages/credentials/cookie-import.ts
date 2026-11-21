/** Cookie-Editor exports one session as many cookie rows, not many credentials. */
export function parsePlaygroundCookie(input: string): Record<string, unknown> {
  const text = input.trim()
  if (!text) throw new Error("Invalid cookie data")
  let value: unknown = text
  if (/^[[{"]/.test(text)) {
    try { value = JSON.parse(text) }
    catch { throw new Error("Invalid cookie JSON") }
  }
  if (typeof value === "string") {
    const cookie = value.replace(/^cookie:\s*/i, "").trim()
    if (!cookie.includes("=") || /[\r\n]/.test(cookie)) throw new Error("Invalid cookie header")
    return { cookie }
  }
  if (Array.isArray(value)) {
    if (!value.length || !value.every(item => item !== null && typeof item === "object" && typeof item.name === "string" && typeof item.value === "string")) {
      throw new Error("Invalid Cookie-Editor export")
    }
    return { cookies: value }
  }
  if (value !== null && typeof value === "object" && ("cookie" in value || "cookies" in value)) {
    return value as Record<string, unknown>
  }
  throw new Error("Invalid cookie data")
}

/** The top-level JSON values written one after another, as when several exports are pasted together. */
function jsonValues(text: string): unknown[] {
  const values: unknown[] = []
  let start = -1, depth = 0, quoted = false
  for (let i = 0; i < text.length; i++) {
    const c = text[i]
    if (quoted) {
      if (c === "\\") i++
      else if (c === '"') quoted = false
    } else if (c === '"') {
      if (depth === 0) throw new Error("Invalid cookie JSON")
      quoted = true
    } else if (c === "[" || c === "{") {
      if (depth++ === 0) start = i
    } else if (c === "]" || c === "}") {
      if (--depth < 0) throw new Error("Invalid cookie JSON")
      if (depth === 0) {
        try { values.push(JSON.parse(text.slice(start, i + 1))) }
        catch { throw new Error("Invalid cookie JSON") }
      }
    } else if (depth === 0 && !/[\s,]/.test(c)) throw new Error("Invalid cookie JSON")
  }
  if (depth !== 0 || quoted) throw new Error("Invalid cookie JSON")
  return values
}

/**
 * Several sessions at once: Cookie-Editor exports pasted one after another, a
 * JSON array of exports or of `{"cookies": ...}` objects, or one Cookie header
 * per line. A single export is one session.
 */
export function parsePlaygroundCookies(input: string): Record<string, unknown>[] {
  const text = input.trim()
  if (!text) return []
  if (!/^[[{]/.test(text)) {
    return text.split(/\r?\n/).map(line => line.trim()).filter(line => line && !line.startsWith("#")).map(parsePlaygroundCookie)
  }
  const sessions: unknown[] = []
  for (const value of jsonValues(text)) {
    const row = (item: unknown) => item !== null && typeof item === "object" && !Array.isArray(item) && "name" in item && "value" in item
    if (Array.isArray(value) && value.length && !value.every(row)) sessions.push(...value)
    else sessions.push(value)
  }
  return sessions.map(session => parsePlaygroundCookie(typeof session === "string" ? session : JSON.stringify(session)))
}
