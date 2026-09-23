//! A form from a list of fields, because twelve families have the same form.
//!
//! Each family declares its columns and its fields; this renders them and
//! builds the request body. The body is built two different ways on purpose,
//! because the server distinguishes them:
//!
//! - a **write** is a whole new row, so an untouched field is simply absent
//!   and the server's own default applies;
//! - a **patch** changes some columns of an existing row, so only fields that
//!   actually changed are sent — and a nullable column that was emptied is
//!   sent as an explicit `null`, which is the `Option<Option<T>>`
//!   "clear it" the DTOs spell with `deserialize_with = "double_option"`.
//!
//! Sending an unchanged field would be harmless today and wrong tomorrow: two
//! operators editing two different columns of one row would overwrite each
//! other with values neither of them typed.

import { useState, type ReactNode } from "react"
import { useTranslation } from "react-i18next"
import { ErrorNotice } from "@/components/state"
import { Button } from "@/components/ui/button"
import {
  Dialog, DialogBody, DialogContent, DialogFooter, DialogHeader, DialogTitle,
} from "@/components/ui/dialog"
import { Field, FieldLabel } from "@/components/ui/field"
import { Input } from "@/components/ui/input"
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select"
import { Switch } from "@/components/ui/switch"
import { Textarea } from "@/components/ui/textarea"
import { fromLocalInput, toLocalInput } from "@/lib/format"

export type FieldKind = "text" | "password" | "number" | "switch" | "select" | "lines" | "datetime" | "json"

export type FormField = {
  /** The DTO field name, and the i18n key under `fields.`. */
  name: string
  label?: string
  kind: FieldKind
  /** Choices for `select`, labelled from `values.<option>`. */
  options?: ReadonlyArray<string>
  /** Choices whose labels are data rather than translations — a row's name. */
  choices?: ReadonlyArray<{ value: string; label: string }>
  /** Refuse an empty value on a create. */
  required?: boolean
  /** Offered when creating and not when editing: the column is immutable. */
  createOnly?: boolean
  /** The patch clears this column with `null` rather than omitting it. */
  nullable?: boolean
}

type FormValues = Record<string, string | boolean>

function readValue(field: FormField, row: Record<string, unknown> | undefined): string | boolean {
  const raw = row?.[field.name]
  if (field.kind === "switch") return raw === undefined || raw === null ? true : Boolean(raw)
  if (raw === undefined || raw === null) return ""
  if (field.kind === "json") return JSON.stringify(raw, null, 2)
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
  if (field.kind === "json") return JSON.parse(text) as unknown
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

function Control({ field, value, onChange }: {
  field: FormField
  value: string | boolean
  onChange: (value: string | boolean) => void
}) {
  const { t } = useTranslation()
  const id = `field-${field.name}`
  if (field.kind === "switch") {
    return <Switch id={id} checked={Boolean(value)} onCheckedChange={(next) => onChange(next)} />
  }
  if (field.kind === "select") {
    const choices = field.choices ?? (field.options ?? []).map((option) => ({ value: option, label: t(`values.${option}`) }))
    return (
      <Select value={String(value)} onValueChange={onChange}>
        <SelectTrigger id={id}><SelectValue placeholder={t("form.choose")} /></SelectTrigger>
        <SelectContent>
          {choices.map((choice) => (
            <SelectItem key={choice.value} value={choice.value}>{choice.label}</SelectItem>
          ))}
        </SelectContent>
      </Select>
    )
  }
  if (field.kind === "lines" || field.kind === "json") {
    return (
      <Textarea
        id={id}
        rows={3}
        autoComplete="off"
        spellCheck={false}
        value={String(value)}
        onChange={(event) => onChange(event.target.value)}
        className="font-mono"
      />
    )
  }
  const type = field.kind === "password" ? "password" : field.kind === "number" ? "number" : field.kind === "datetime" ? "datetime-local" : "text"
  return <Input id={id} type={type} value={String(value)} onChange={(event) => onChange(event.target.value)} />
}

type DialogProps = {
  open: boolean
  onOpenChange: (open: boolean) => void
  title: string
  fields: ReadonlyArray<FormField>
  /** The row being edited; absent when creating. */
  original?: Record<string, unknown>
  mode: "create" | "edit"
  onSubmit: (body: Record<string, unknown>) => void
  pending?: boolean
  error?: unknown
  /** Rendered under the fields, for the one-off controls a family needs. */
  extra?: ReactNode
}

/**
 * The fields, seeded once per mount.
 *
 * Mounted only while the dialog is open and keyed on the row, which is what
 * replaces the effect that would otherwise re-seed on every open: an operator
 * who cancels a half-typed edit and reopens it gets the row back rather than
 * their abandoned draft, and React never has to cascade a render to do it.
 */
function RecordForm({ fields, original, mode, onSubmit, pending, error, extra, onOpenChange }: DialogProps) {
  const { t } = useTranslation()
  const offered = fields.filter((field) => mode === "create" || !field.createOnly)
  const [values, setValues] = useState<FormValues>(() => {
    const seeded: FormValues = {}
    for (const field of fields) seeded[field.name] = readValue(field, original)
    return seeded
  })

  const [parseError, setParseError] = useState<Error | null>(null)
  const submit = () => {
    let body: Record<string, unknown>
    try {
      body = mode === "create" ? buildWrite(offered, values) : buildPatch(offered, values, original)
    } catch {
      setParseError(new Error(t("form.invalidJson")))
      return
    }
    setParseError(null)
    onSubmit(body)
  }

  const missing = offered.some((field) => field.required && mode === "create" && !String(values[field.name] ?? "").trim())

  return (
    <>
      <DialogBody className="space-y-4">
        {parseError || error ? <ErrorNotice error={parseError ?? error} /> : null}
        {offered.map((field) => (
          <Field key={field.name} orientation={field.kind === "switch" ? "horizontal" : "vertical"}>
            <FieldLabel htmlFor={`field-${field.name}`}>
              {field.label ?? t(`fields.${field.name}`)}
              {field.required && mode === "create" ? <span aria-hidden className="text-destructive"> *</span> : null}
            </FieldLabel>
            <Control
              field={field}
              value={values[field.name] ?? ""}
              onChange={(next) => setValues((current) => ({ ...current, [field.name]: next }))}
            />
          </Field>
        ))}
        {extra}
      </DialogBody>
      <DialogFooter>
        <Button variant="outline" onClick={() => onOpenChange(false)}>{t("actions.cancel")}</Button>
        <Button
          disabled={pending || missing}
          onClick={submit}
        >
          {mode === "create" ? t("actions.create") : t("actions.save")}
        </Button>
      </DialogFooter>
    </>
  )
}

export function RecordDialog(props: DialogProps) {
  const { t } = useTranslation()
  const { open, onOpenChange, title, original } = props
  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent aria-describedby={undefined} className="sm:max-w-md" closeLabel={t("actions.close")}>
        <DialogHeader>
          <DialogTitle>{title}</DialogTitle>
        </DialogHeader>
        {open ? <RecordForm {...props} key={String(original?.id ?? "new")} /> : null}
      </DialogContent>
    </Dialog>
  )
}
