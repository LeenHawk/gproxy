export type BulkFormat = "json" | "tokens"
export type BulkCredential = { label: string | null; secret: Record<string, unknown>; source: number }
export type BulkError = { source: number; code: "invalidJson" | "invalidSecret" | "invalidLabel" }

function object(value: unknown): value is Record<string, unknown> {
  return value !== null && typeof value === "object" && !Array.isArray(value)
}
function text(value: unknown): string | null {
  return typeof value === "string" && value.trim() ? value.trim() : null
}
function canonical(value: unknown): string {
  if (Array.isArray(value)) return `[${value.map(canonical).join(",")}]`
  if (object(value)) return `{${Object.keys(value).sort().map(key => `${JSON.stringify(key)}:${canonical(value[key])}`).join(",")}}`
  return JSON.stringify(value)
}
function identity(secret: Record<string, unknown>): string {
  // OAuth export timestamps and display metadata do not change the credential.
  if (text(secret.access_token) || text(secret.refresh_token)) {
    return canonical([secret.api_key ?? null, secret.access_token ?? null, secret.refresh_token ?? null, secret.account_id ?? secret.chatgpt_account_id ?? null])
  }
  return canonical(secret)
}

/** Parse locally: neither malformed input nor secret values enter error messages. */
export function parseBulkCredentials(input: string, format: BulkFormat) {
  const entries: { value: unknown; source: number }[] = []
  const errors: BulkError[] = []
  if (format === "tokens") {
    input.split(/\r?\n/).forEach((line, index) => {
      const token = line.trim()
      if (token && !token.startsWith("#")) entries.push({ value: { api_key: token }, source: index + 1 })
    })
  } else if (input.trim()) {
    try {
      const parsed: unknown = JSON.parse(input)
      const values = Array.isArray(parsed) ? parsed : [parsed]
      values.forEach((value, index) => entries.push({ value, source: index + 1 }))
    } catch {
      if (input.trimStart().startsWith("[")) errors.push({ source: 1, code: "invalidJson" })
      else input.split(/\r?\n/).forEach((line, index) => {
        if (!line.trim() || line.trimStart().startsWith("#")) return
        try { entries.push({ value: JSON.parse(line), source: index + 1 }) }
        catch { errors.push({ source: index + 1, code: "invalidJson" }) }
      })
    }
  }
  const items: BulkCredential[] = []
  const seen = new Set<string>()
  let duplicates = 0
  for (const { value, source } of entries) {
    const wrapped = object(value) && Object.hasOwn(value, "secret")
    const secret = wrapped ? value.secret : value
    if (!object(secret) || !Object.keys(secret).length) { errors.push({ source, code: "invalidSecret" }); continue }
    if (wrapped && value.label != null && typeof value.label !== "string") { errors.push({ source, code: "invalidLabel" }); continue }
    const key = identity(secret)
    if (seen.has(key)) { duplicates++; continue }
    seen.add(key)
    const label = (wrapped ? text(value.label) : null) ?? text(secret.email) ?? text(secret.client_email) ?? text(secret.account_id) ?? text(secret.chatgpt_account_id)
    items.push({ label, secret, source })
  }
  return { items, errors, duplicates }
}
