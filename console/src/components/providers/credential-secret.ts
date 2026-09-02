import type { ChannelFieldDto } from "@/generated/ChannelFieldDto"

const COOKIE_FIELD: ChannelFieldDto = {
  key: "cookie",
  i18n_key: "cookie",
  control: "secret",
  required: true,
  advanced: false,
  default_value: null,
  options: [],
}

export function defaultCredentialKind(fields: Array<ChannelFieldDto>) {
  if (fields.some((field) => field.key === "cookie")) return "cookie"
  if (fields.some((field) => field.key === "access_token")
    && !fields.some((field) => ["api_key", "private_key"].includes(field.key))) return "oauth"
  return "api_key"
}

export function fieldsForCredentialKind(fields: Array<ChannelFieldDto>, kind: string) {
  if (kind === "cookie") {
    const cookie = fields.find((field) => field.key === "cookie")
    return [cookie ?? COOKIE_FIELD]
  }
  if (kind === "api_key" && fields.some((field) => field.key === "api_key")) {
    return fields.filter((field) => field.key === "api_key")
  }
  if (kind === "oauth" && fields.some((field) => field.key === "api_key")) {
    return fields.filter((field) => field.key !== "api_key")
  }
  return fields
}

export function isSingleKey(fields: Array<ChannelFieldDto>) {
  return fields.length === 1 && fields[0].control === "secret"
}

export function secretTemplate(fields: Array<ChannelFieldDto>) {
  if (isSingleKey(fields)) return ""
  return JSON.stringify(Object.fromEntries(fields.map((field) => [field.key, field.default_value ?? ""])), null, 2)
}

/** Returns the secret object to submit, or null when the text is empty or unparseable. */
export function buildSecret(fields: Array<ChannelFieldDto>, text: string): Record<string, unknown> | null {
  const trimmed = text.trim()
  if (trimmed === "") return null
  if (isSingleKey(fields)) return { [fields[0].key]: trimmed }
  try {
    const parsed: unknown = JSON.parse(trimmed)
    return typeof parsed === "object" && parsed !== null && !Array.isArray(parsed) ? parsed as Record<string, unknown> : null
  } catch {
    return null
  }
}
