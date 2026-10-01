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

import { SearchableSelect } from "@/components/searchable-select"
import { Fragment, useState, type ReactNode } from "react"
import { useTranslation } from "react-i18next"
import { ProxyControl, type ProxySettings } from "@/components/proxy-control"
import { ErrorNotice } from "@/components/state"
import { Button } from "@/components/ui/button"
import {
  Dialog, DialogBody, DialogContent, DialogFooter, DialogHeader, DialogTitle,
} from "@/components/ui/dialog"
import { Field, FieldContent, FieldDescription, FieldGroup, FieldLabel } from "@/components/ui/field"
import { Input } from "@/components/ui/input"
import { Select, SelectContent, SelectGroup, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select"
import { Switch } from "@/components/ui/switch"
import { Textarea } from "@/components/ui/textarea"
import { buildPatch, buildWrite, readValue, type FormField, type FormValues } from "./record-form-values"
export type { FieldKind, FormField } from "./record-form-values"

function Control({ field, value, onChange, original }: {
  original?: Record<string, unknown>
  field: FormField
  value: string | boolean
  onChange: (value: string | boolean) => void
}) {
  const { t } = useTranslation()
  const id = `field-${field.name}`
  if (field.kind === "proxy") return <ProxyControl id={id} value={value ? JSON.parse(String(value)) as ProxySettings : null} onChange={v => onChange(v ? JSON.stringify(v) : "")} scope={field.proxyScope?.(original) ?? { scope: "global" }} />
  if (field.kind === "switch") {
    return <Switch id={id} aria-describedby={field.description ? `${id}-description` : undefined} checked={Boolean(value)} onCheckedChange={(next) => onChange(next)} />
  }
  if (field.kind === "searchable") return <SearchableSelect id={id} label={field.label ?? t(`fields.${field.name}`)} value={String(value)} options={field.choices ?? []} source={field.source} allowCustom={field.allowCustom} emptyLabel={field.emptyLabel} emptyValue={field.emptyValue} onChange={onChange} />
  if (field.kind === "select") {
    const choices = field.choices ?? (field.options ?? []).map((option) => ({ value: option, label: t(`values.${option}`) }))
    return (
      <Select value={String(value) || "__unset"} onValueChange={(next) => onChange(next === "__unset" ? "" : next)}>
        <SelectTrigger id={id}><SelectValue placeholder={t("form.choose")} /></SelectTrigger>
        <SelectContent><SelectGroup>
          {field.nullable ? <SelectItem value="__unset">{t("form.unset")}</SelectItem> : null}
          {choices.map((choice) => (
            <SelectItem key={choice.value} value={choice.value}>{choice.label}</SelectItem>
          ))}
        </SelectGroup></SelectContent>
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
  submitDisabled?: boolean
  error?: unknown
  /** Rendered under the fields, for the one-off controls a family needs. */
  extra?: ReactNode
  extraAfter?: string
  advancedExtra?: ReactNode
  advancedFields?: readonly string[]
}

/**
 * The fields, seeded once per mount.
 *
 * Mounted only while the dialog is open and keyed on the row, which is what
 * replaces the effect that would otherwise re-seed on every open: an operator
 * who cancels a half-typed edit and reopens it gets the row back rather than
 * their abandoned draft, and React never has to cascade a render to do it.
 */
function RecordForm({ fields, original, mode, onSubmit, pending, submitDisabled, error, extra, extraAfter, advancedExtra, advancedFields, onOpenChange, inline = false }: DialogProps & { inline?: boolean }) {
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

  const renderField = (field: FormField) => {
    const label = <FieldLabel htmlFor={`field-${field.name}`}>{field.label ?? t(`fields.${field.name}`)}{field.required && mode === "create" ? <span aria-hidden className="text-destructive"> *</span> : null}</FieldLabel>
    return <Fragment key={field.name}><Field orientation={field.kind === "switch" ? "horizontal" : "vertical"}>
      {field.description ? <FieldContent>{label}<FieldDescription id={`field-${field.name}-description`}>{field.description}</FieldDescription></FieldContent> : label}
      <Control field={field} original={original} value={values[field.name] ?? ""} onChange={next => setValues(current => ({ ...current, [field.name]: next }))} />
    </Field>{extraAfter === field.name ? extra : null}</Fragment>
  }
  const secondary = offered.filter(field => advancedFields?.includes(field.name))
  const controls = <>
    {parseError || error ? <ErrorNotice error={parseError ?? error} /> : null}
    <fieldset disabled={pending}><FieldGroup className="sm:grid-cols-1">
      {offered.filter(field => !advancedFields?.includes(field.name)).map(renderField)}
      {!extraAfter ? extra : null}
      {secondary.length || advancedExtra ? <details className="group">
        <summary className="cursor-pointer text-sm font-medium text-muted-foreground hover:text-foreground">{t("limits.advanced")}</summary>
        <FieldGroup className="pt-5 sm:grid-cols-1">{advancedExtra}{secondary.map(renderField)}</FieldGroup>
      </details> : null}
    </FieldGroup></fieldset>
  </>
  const actions = <>
    {!inline ? <Button variant="outline" disabled={pending} onClick={() => onOpenChange(false)}>{t("actions.cancel")}</Button> : null}
    <Button disabled={pending || missing || submitDisabled} onClick={submit}>{t(mode === "create" ? "actions.create" : "actions.save")}</Button>
  </>
  return inline ? <div className="flex flex-col gap-4">{controls}<div className="flex justify-end gap-2">{actions}</div></div>
    : <><DialogBody className="flex flex-col gap-4">{controls}</DialogBody><DialogFooter>{actions}</DialogFooter></>

}

export function RecordDialog(props: DialogProps) {
  const { t } = useTranslation()
  const { open, onOpenChange, title, original } = props
  return (
    <Dialog open={open} onOpenChange={value => { if (!props.pending) onOpenChange(value) }}>
      <DialogContent aria-describedby={undefined} className="sm:max-w-md" closeLabel={t("actions.close")}>
        <DialogHeader>
          <DialogTitle>{title}</DialogTitle>
        </DialogHeader>
        {open ? <RecordForm {...props} key={String(original?.id ?? "new")} /> : null}
      </DialogContent>
    </Dialog>
  )
}


/** The same patch semantics inside an existing detail panel, without a dialog. */
export function RecordEditor(props: Omit<DialogProps, "open" | "onOpenChange" | "title">) {
  return <RecordForm {...props} inline open title="" onOpenChange={() => {}} />
}
