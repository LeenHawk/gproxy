import { useId, useState, type FormEvent } from "react"
import { useTranslation } from "react-i18next"
import type { ProviderModelDto } from "@/generated/sdk"
import { ErrorNotice } from "@/components/state"
import { Button } from "@/components/ui/button"
import { Dialog, DialogBody, DialogContent, DialogFooter, DialogHeader, DialogTitle } from "@/components/ui/dialog"
import { Field, FieldGroup, FieldLabel } from "@/components/ui/field"
import { Input } from "@/components/ui/input"
import { Switch } from "@/components/ui/switch"
import { Textarea } from "@/components/ui/textarea"

type ObjectValue = Record<string, unknown>
type ModelField = { path: string; label: string; kind?: "number" | "switch" | "lines"; required?: boolean }
const field = (path: string, label: string, kind?: ModelField["kind"], required?: boolean): ModelField => ({ path, label, kind, required })
const sections: Array<{ key: string; fields: ModelField[] }> = [
  { key: "openai", fields: [
    field("owned_by", "owner", undefined, true), field("created", "createdUnix", "number"),
    field("display_name", "displayName"), field("description", "description"),
    field("context_window", "contextWindow", "number"), field("max_output_tokens", "outputLimit", "number"),
  ] },
  { key: "claude", fields: [
    field("display_name", "displayName"), field("created_at", "createdIso"),
    field("max_input_tokens", "inputLimit", "number"), field("max_tokens", "outputLimit", "number"),
    field("allowed_fallback_models", "fallbackModels", "lines"),
    ...["batch", "citations", "code_execution", "image_input", "pdf_input", "structured_outputs"].map((key) => field(`capabilities.${key}`, key, "switch")),
    ...["supported", "clear_thinking_20251015", "clear_tool_uses_20250919", "compact_20260112"].map((key) => field(`capabilities.context_management.${key}`, `context_${key}`, "switch")),
    ...["supported", "low", "medium", "high", "xhigh", "max"].map((key) => field(`capabilities.effort.${key}`, `effort_${key}`, "switch")),
    ...["supported", "adaptive", "enabled"].map((key) => field(`capabilities.thinking.${key}`, `thinking_${key}`, "switch")),
  ] },
  { key: "gemini", fields: [
    field("base_model_id", "baseModel", undefined, true), field("version", "version", undefined, true),
    field("displayName", "displayName"), field("description", "description"),
    field("inputTokenLimit", "inputLimit", "number"), field("outputTokenLimit", "outputLimit", "number"),
    field("supportedGenerationMethods", "generationMethods", "lines"),
  ] },
]

function object(value: unknown): ObjectValue {
  return value && typeof value === "object" && !Array.isArray(value) ? value as ObjectValue : {}
}
function get(value: ObjectValue, path: string): unknown {
  return path.split(".").reduce<unknown>((current, key) => object(current)[key], value)
}
function set(value: ObjectValue, path: string, next: unknown) {
  const keys = path.split(".")
  let parent = value
  for (const key of keys.slice(0, -1)) {
    parent[key] = { ...object(parent[key]) }
    parent = parent[key] as ObjectValue
  }
  if (next === undefined) delete parent[keys.at(-1)!]
  else parent[keys.at(-1)!] = next
}

type Props = {
  open: boolean; onOpenChange: (open: boolean) => void; model?: ProviderModelDto
  onSubmit: (body: Record<string, unknown>) => void; pending: boolean; error: unknown
}
export function ProviderModelDialog({ open, onOpenChange, model, ...props }: Props) {
  const { t } = useTranslation()
  return <Dialog open={open} onOpenChange={onOpenChange}>
    <DialogContent className="sm:max-w-2xl" aria-describedby={undefined}>
      <DialogHeader><DialogTitle>{t(model ? "edit.provider-models" : "create.provider-models")}</DialogTitle></DialogHeader>
      {open ? <ModelForm key={model?.id ?? "new"} model={model} {...props} onCancel={() => onOpenChange(false)} /> : null}
    </DialogContent>
  </Dialog>
}

function ModelForm({ model, onSubmit, pending, error, onCancel }: Omit<Props, "open" | "onOpenChange"> & { onCancel: () => void }) {
  const { t } = useTranslation()
  const id = useId()
  const [name, setName] = useState(model?.upstreamName ?? "")
  const [modelId, setModelId] = useState(model?.modelId ?? "")
  const [enabled, setEnabled] = useState(model?.enabled ?? true)
  const initial = object(object(model?.metadata).supplements)
  const [active, setActive] = useState(() => Object.fromEntries(sections.map(({ key }) => [key, key in initial])))
  const [values, setValues] = useState<Record<string, string | boolean>>(() => Object.fromEntries(sections.flatMap(({ key, fields }) => fields.map((f) => {
    const value = get(object(initial[key]), f.path)
    return [`${key}.${f.path}`, f.kind === "switch" ? value === true : Array.isArray(value) ? value.join("\n") : value == null ? "" : String(value)]
  }))))
  function submit(event: FormEvent) {
    event.preventDefault()
    const metadata = structuredClone(object(model?.metadata))
    const supplements = { ...object(metadata.supplements) }
    for (const { key, fields } of sections) {
      if (!active[key]) { delete supplements[key]; continue }
      const data = { ...object(supplements[key]) }
      for (const f of fields) {
        const value = values[`${key}.${f.path}`]
        const text = String(value).trim()
        set(data, f.path, f.kind === "switch" ? value : f.kind === "lines" ? text.split("\n").map((line) => line.trim()).filter(Boolean) : !text ? undefined : f.kind === "number" ? Number(text) : text)
      }
      supplements[key] = data
    }
    if (Object.keys(supplements).length) metadata.supplements = supplements
    else delete metadata.supplements
    const body: Record<string, unknown> = {}
    if (!model || name.trim() !== model.upstreamName) body.upstreamName = name.trim()
    if (!model || (modelId.trim() || null) !== model.modelId) body.modelId = modelId.trim() || null
    if (!model || enabled !== model.enabled) body.enabled = enabled
    if (!model || JSON.stringify(metadata) !== JSON.stringify(model.metadata)) body.metadata = metadata
    onSubmit(body)
  }
  return <form onSubmit={submit} className="flex min-h-0 flex-col">
    <DialogBody><FieldGroup>
      <Field><FieldLabel htmlFor={`${id}-name`}>{t("fields.upstreamName")}</FieldLabel><Input id={`${id}-name`} required value={name} onChange={(e) => setName(e.target.value)} /></Field>
      <Field><FieldLabel htmlFor={`${id}-model`}>{t("fields.modelId")}</FieldLabel><Input id={`${id}-model`} value={modelId} onChange={(e) => setModelId(e.target.value)} /></Field>
      <Field orientation="horizontal" data-field-span="full"><FieldLabel htmlFor={`${id}-enabled`}>{t("fields.enabled")}</FieldLabel><Switch id={`${id}-enabled`} checked={enabled} onCheckedChange={setEnabled} /></Field>
      {sections.map(({ key, fields }) => <div key={key} data-field-span="full" className="flex flex-col gap-4 rounded-lg border p-4">
        <Field orientation="horizontal"><FieldLabel htmlFor={`${id}-${key}`}>{t(`rules.protocols.${key}`)}</FieldLabel><Switch id={`${id}-${key}`} checked={active[key]} onCheckedChange={(next) => setActive({ ...active, [key]: next })} /></Field>
        {active[key] ? <FieldGroup>{fields.map((f) => {
          const path = `${key}.${f.path}`, controlId = `${id}-${path}`, value = values[path]
          const change = (next: string | boolean) => setValues((previous) => ({ ...previous, [path]: next }))
          return <Field key={path} data-field-span={f.kind === "lines" ? "full" : undefined} orientation={f.kind === "switch" ? "horizontal" : "vertical"}>
            <FieldLabel htmlFor={controlId}>{t(`modelForm.${f.label}`)}</FieldLabel>
            {f.kind === "switch" ? <Switch id={controlId} checked={value === true} onCheckedChange={change} /> : f.kind === "lines" ? <Textarea id={controlId} rows={3} value={String(value)} onChange={(e) => change(e.target.value)} /> : <Input id={controlId} type={f.kind === "number" ? "number" : "text"} min={f.kind === "number" ? 0 : undefined} step={f.kind === "number" ? 1 : undefined} required={f.required} value={String(value)} onChange={(e) => change(e.target.value)} />}
          </Field>
        })}</FieldGroup> : null}
      </div>)}
      {error ? <ErrorNotice error={error} /> : null}
    </FieldGroup></DialogBody>
    <DialogFooter><Button type="button" variant="outline" disabled={pending} onClick={onCancel}>{t("actions.cancel")}</Button><Button type="submit" disabled={pending || !name.trim()}>{t("actions.save")}</Button></DialogFooter>
  </form>
}
