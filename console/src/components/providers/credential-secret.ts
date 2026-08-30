import type { ChannelFieldDto } from "@/generated/ChannelFieldDto"

// A credential's secret is one thing to the operator — a key, or the JSON a vendor hands out.
// Rendering one input per declared field turns a paste into a transcription exercise.
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
