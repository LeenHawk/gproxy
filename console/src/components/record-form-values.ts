import type { OptionSource } from "@/components/searchable-select"
import type { ProxyScope } from "@/components/proxy-control"
import { fromLocalInput, toLocalInput } from "@/lib/format"

export type FieldKind = "text" | "password" | "number" | "switch" | "select" | "searchable" | "lines" | "datetime" | "json" | "proxy"

export type FormField = {
  /** The DTO field name, and the i18n key under `fields.`. */
  name: string
  allowCustom?: boolean
  emptyValue?: string
  emptyLabel?: string
  proxyScope?: (original?: Record<string, unknown>) => ProxyScope
  label?: string
  kind: FieldKind
  /** Initial value for a switch on a new record; enabled switches default on. */
  defaultChecked?: boolean
  /** Choices for `select`, labelled from `values.<option>`. */
  options?: ReadonlyArray<string>
  /** Choices whose labels are data rather than translations — a row's name. */
  choices?: ReadonlyArray<{ value: string; label: string }>
  source?: OptionSource
  /** Refuse an empty value on a create. */
  required?: boolean
  /** Offered when creating and not when editing: the column is immutable. */
  createOnly?: boolean
  /** The patch clears this column with `null` rather than omitting it. */
  nullable?: boolean
}

export type FormValues = Record<string, string | boolean>

export function readValue(field: FormField, row: Record<string, unknown> | undefined): string | boolean {
  const raw = row?.[field.name]
  if (field.kind === "switch") return raw === undefined || raw === null ? field.defaultChecked ?? true : Boolean(raw)
  if (raw === undefined || raw === null) return ""
  if (field.kind === "json" || field.kind === "proxy") return JSON.stringify(raw, null, 2)
  if (field.kind === "lines") return Array.isArray(raw) ? raw.join("\n") : String(raw)
  if (field.kind === "datetime") return typeof raw === "number" ? toLocalInput(raw) : ""
  return String(raw)
}

function writeValue(field: FormField, value: string | boolean): unknown {
  if (field.kind === "switch") return Boolean(value)
  const text = String(value)
  if (field.kind === "lines") {
    const entries = text.split("\n").map((line) => line.trim()).filter(Boolean)
    return entries.length ? entries : null
  }
  if (!text.trim()) return null
  if (field.kind === "json" || field.kind === "proxy") return JSON.parse(text) as unknown
  if (field.kind === "number") {
    const parsed = Number(text)
    return Number.isFinite(parsed) ? parsed : null
  }
  if (field.kind === "datetime") return fromLocalInput(text)
  return text
}

/** A create body: whatever the operator actually filled in. */
export function buildWrite(fields: ReadonlyArray<FormField>, values: FormValues) {
  const body: Record<string, unknown> = {}
  for (const field of fields) {
    const value = writeValue(field, values[field.name])
    if (value !== null) body[field.name] = value
  }
  return body
}

/** A patch body: only what changed, with `null` meaning "clear it". */
export function buildPatch(
  fields: ReadonlyArray<FormField>,
  values: FormValues,
  original: Record<string, unknown> | undefined,
) {
  const body: Record<string, unknown> = {}
  for (const field of fields) {
    if (field.createOnly) continue
    const before = readValue(field, original)
    if (values[field.name] === before) continue
    const value = writeValue(field, values[field.name])
    // A non-nullable column has nothing to say about an emptied input; the
    // server would reject `null` anyway, so it is left alone.
    if (value === null && !field.nullable) continue
    body[field.name] = value
  }
  return body
}
