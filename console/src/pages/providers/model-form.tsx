import { useState, type FormEvent } from "react"
import { useTranslation } from "react-i18next"
import type { ProviderModelDto } from "@/generated/sdk"
import { ProviderModelFields } from "@/components/providers/provider-model-fields"
import { modelMetadata, modelState } from "@/components/providers/provider-model-state"
import type { VariantRuleRow } from "@/components/providers/provider-model-variant-rules"
import { ErrorNotice } from "@/components/state"
import { Button } from "@/components/ui/button"
import { Dialog, DialogBody, DialogContent, DialogFooter, DialogHeader, DialogTitle } from "@/components/ui/dialog"
import { Field, FieldGroup, FieldLabel } from "@/components/ui/field"
import { Input } from "@/components/ui/input"
import { Switch } from "@/components/ui/switch"
export type ModelSave = { upstreamName: string; metadata: Record<string, unknown>; enabled: boolean; variants: VariantRuleRow[] }
export function ProviderModelDialog({ model, channel, variants, onClose, onSave, pending, error }: { model?: ProviderModelDto; channel: string; variants: VariantRuleRow[]; onClose: () => void; onSave: (value: ModelSave) => void; pending: boolean; error: unknown }) {
  const { t } = useTranslation()
  const [name, setName] = useState(model?.upstreamName ?? "")
  const [enabled, setEnabled] = useState(model?.enabled ?? true)
  const [state, setState] = useState(() => modelState(model?.metadata, variants))
  const [validation, setValidation] = useState<Error | null>(null)
  function submit(event: FormEvent) {
    event.preventDefault()
    const names = state.variants.map(v => v.name.trim())
    if (names.some(n => !n || /[?*]/.test(n)) || new Set([name.trim(), ...names]).size !== names.length + 1) { setValidation(new Error(t("modelUI.invalidVariants"))); return }
    const policy = state.metadata.truncation_policy
    if (policy && ((policy.mode !== "bytes" && policy.mode !== "tokens") || policy.limit == null)) {
      setValidation(new Error(`${t("providers.models.truncation")}: ${t("form.required")}`)); return
    }
    setValidation(null)
    onSave({ upstreamName: name.trim(), enabled, metadata: modelMetadata(state, model?.metadata), variants: state.variants })
  }
  return <Dialog open onOpenChange={open => { if (!open && !pending) onClose() }}><DialogContent className="sm:max-w-2xl" aria-describedby={undefined}>
    <form className="flex min-h-0 flex-1 flex-col" onSubmit={submit}>
      <DialogHeader><DialogTitle>{t(model ? "edit.provider-models" : "create.provider-models")}</DialogTitle></DialogHeader>
      <DialogBody><FieldGroup>
        <Field data-field-span="full"><FieldLabel htmlFor="model-name">{t("fields.upstreamName")}</FieldLabel><Input id="model-name" required value={name} onChange={e => setName(e.target.value)} /></Field>
        <ProviderModelFields modelId={name} channel={channel} value={state} onChange={setState} />
        <Field orientation="horizontal" data-field-span="full"><FieldLabel htmlFor="model-enabled">{t("fields.enabled")}</FieldLabel><Switch id="model-enabled" checked={enabled} onCheckedChange={setEnabled} /></Field>
      </FieldGroup>{error || validation ? <ErrorNotice error={error || validation} /> : null}</DialogBody>
      <DialogFooter><Button type="button" variant="outline" disabled={pending} onClick={onClose}>{t("actions.cancel")}</Button><Button type="submit" disabled={pending || !name.trim()}>{t("actions.save")}</Button></DialogFooter>
    </form>
  </DialogContent></Dialog>
}
