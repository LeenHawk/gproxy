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
