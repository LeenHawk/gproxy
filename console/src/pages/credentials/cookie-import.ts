/**
 * Cookie-Editor exports one session as many cookie rows, not many credentials.
 * An `accounts.google.com` export pasted after the AI Studio one belongs to the
 * same session and is merged into it.
 */
export function parsePlaygroundCookie(input: string): Record<string, unknown> {
  const text = input.trim()
  if (!text) throw new Error("Invalid cookie data")
  if (/^[[{]/.test(text) && jsonValues(text).length > 1) {
    const sessions = parsePlaygroundCookies(text)
    if (sessions.length !== 1) throw new Error("Invalid cookie data")
    return sessions[0]
  }
  return parseOne(text)
}

function parseOne(text: string): Record<string, unknown> {
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
    return mergeAccountCookies(text.split(/\r?\n/).map(line => line.trim()).filter(line => line && !line.startsWith("#")).map(parseOne))
  }
  const sessions: unknown[] = []
  for (const value of jsonValues(text)) {
    const row = (item: unknown) => item !== null && typeof item === "object" && !Array.isArray(item) && "name" in item && "value" in item
    if (Array.isArray(value) && value.length && !value.every(row)) sessions.push(...value)
    else sessions.push(value)
  }
  return mergeAccountCookies(sessions.map(session => parseOne(typeof session === "string" ? session : JSON.stringify(session))))
}

/** The cookies that make a session; an export without any of them is not one. */
const SESSION_COOKIES = new Set(["SID", "__Secure-1PSID", "__Secure-3PSID"])

type CookieRow = Record<string, unknown> & { name: string, value: string }

function cookieRows(session: Record<string, unknown>): CookieRow[] | null {
  if (Array.isArray(session.cookies)) return session.cookies as CookieRow[]
  if (typeof session.cookie === "string") {
    return session.cookie.split(";").flatMap(pair => {
      const at = pair.indexOf("=")
      return at > 0 ? [{ name: pair.slice(0, at).trim(), value: pair.slice(at + 1).trim() }] : []
    })
  }
  return null
}

/**
 * `LSID` and its siblings are host-only cookies of `accounts.google.com`, so
 * they arrive as a separate export with no session cookies of its own. Fold
 * each such export into the session pasted before it (or after it, when it
 * comes first) instead of treating it as another account.
 */
function mergeAccountCookies(sessions: Record<string, unknown>[]): Record<string, unknown>[] {
  const accountOnly = (session: Record<string, unknown>) => {
    const rows = cookieRows(session)
    return rows !== null && rows.length > 0 && !rows.some(row => SESSION_COOKIES.has(row.name))
  }
  const merge = (session: Record<string, unknown>, extra: Record<string, unknown>) => {
    const rest = { ...session }
    delete rest.cookie
    return { ...rest, cookies: [...(cookieRows(session) ?? []), ...(cookieRows(extra) ?? [])] }
  }
  const merged: Record<string, unknown>[] = []
  let pending: Record<string, unknown>[] = []
  for (const session of sessions) {
    if (accountOnly(session)) {
      if (merged.length) merged[merged.length - 1] = merge(merged[merged.length - 1], session)
      else pending.push(session)
      continue
    }
    merged.push(pending.reduce(merge, session))
    pending = []
  }
  return pending.length ? [...merged, ...pending] : merged
}
