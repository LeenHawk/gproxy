import type { ChannelFieldDto } from "@/generated/ChannelFieldDto"

export function objectValue(value: unknown): Record<string, unknown> {
  return value != null && typeof value === "object" && !Array.isArray(value)
    ? { ...value as Record<string, unknown> }
    : {}
}

export function settingValue(field: ChannelFieldDto, values: Record<string, unknown>) {
  return values[field.key] ?? field.default_value ?? (field.control === "boolean" ? false : "")
}

export function updateSetting(
  values: Record<string, unknown>,
  field: ChannelFieldDto,
  input: string | boolean,
) {
  const next = { ...values }
  if (field.control === "boolean") next[field.key] = input
  else if (field.control === "integer") {
    const value = typeof input === "string" && input.trim() ? Number(input) : null
    if (value == null) delete next[field.key]
    else next[field.key] = value
  } else if (field.control === "string_list") {
    const value = typeof input === "string"
      ? input.split(",").map((item) => item.trim()).filter(Boolean)
      : []
    if (value.length) next[field.key] = value
    else delete next[field.key]
  } else if (typeof input === "string" && input.trim()) next[field.key] = input.trim()
  else delete next[field.key]
  return next
}

export function inputValue(field: ChannelFieldDto, value: unknown) {
  if (field.control === "string_list" && Array.isArray(value)) return value.join(", ")
  return typeof value === "string" || typeof value === "number" ? String(value) : ""
}

export function humanizeSettingKey(key: string) {
  const words = key.replaceAll("_", " ")
  return words.charAt(0).toUpperCase() + words.slice(1)
}

export type EndpointRow = { kind: string; url: string }

export function endpointRows(values: Record<string, unknown>, allowed: Array<string>) {
  const endpoints = objectValue(values.endpoints)
  return allowed.flatMap((kind) => typeof endpoints[kind] === "string"
    ? [{ kind, url: endpoints[kind] as string }]
    : [])
}

export function updateEndpoints(
  values: Record<string, unknown>,
  allowed: Array<string>,
  rows: Array<EndpointRow>,
) {
  const endpoints = objectValue(values.endpoints)
  for (const kind of allowed) delete endpoints[kind]
  for (const row of rows) {
    if (row.kind) endpoints[row.kind] = row.url
  }
  const next = { ...values }
  if (Object.keys(endpoints).length) next.endpoints = endpoints
  else delete next.endpoints
  return next
}
