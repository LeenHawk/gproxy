import { useState } from "react"
import { useTranslation } from "react-i18next"
import type { ProviderModelDto } from "@/generated/ProviderModelDto"
import type { ProviderModelWriteRequest } from "@/generated/ProviderModelWriteRequest"
import type { ProviderDto } from "@/generated/ProviderDto"
import type { ProviderRuleSetDto } from "@/generated/ProviderRuleSetDto"
import type { RuleDto } from "@/generated/RuleDto"
import type { RuleSetDto } from "@/generated/RuleSetDto"
import { ProviderModelFields } from "@/components/providers/provider-model-fields"
import { providerModelRequest, providerModelState, readVariants } from "@/components/providers/provider-model-state"
import { variantRuleActions, type VariantRuleChanges } from "@/components/providers/provider-model-variant-rules"
import { Button } from "@/components/ui/button"
import { Dialog, DialogBody, DialogClose, DialogContent, DialogFooter, DialogHeader, DialogTitle } from "@/components/ui/dialog"
import { Field, FieldError, FieldGroup, FieldLabel } from "@/components/ui/field"
import { Input } from "@/components/ui/input"

export function ProviderModelDialog({ open, onOpenChange, provider, model, ruleSets, rules, attachments, saving, onSave }: {
  open: boolean
  onOpenChange: (open: boolean) => void
  provider: ProviderDto
  model?: ProviderModelDto
  ruleSets: Array<RuleSetDto>
  rules: Array<RuleDto>
  attachments: Array<ProviderRuleSetDto>
  saving: boolean
  onSave: (value: ProviderModelWriteRequest, changes: VariantRuleChanges, id?: number) => Promise<void>
}) {
  const { t } = useTranslation()
  const [modelId, setModelId] = useState(model?.model_id ?? "")
  const [metadata, setMetadata] = useState(() => providerModelState(model ?? null, variantRuleActions(provider.id, ruleSets, rules, attachments)))
  const [error, setError] = useState("")

  const submit = async (event: React.FormEvent) => {
    event.preventDefault()
    if (!modelId.trim()) {
      setError(t("common.errors.required"))
      return
    }
    const fields = providerModelRequest(metadata)
    setError("")
    await onSave(
      { provider_id: provider.id, model_id: modelId.trim(), enabled: model?.enabled ?? true, ...fields },
      { oldNames: readVariants(model?.variants).names, rows: metadata.variants },
      model?.id,
    )
    onOpenChange(false)
  }

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="sm:max-w-2xl" showCloseButton={false}>
        <form className="flex min-h-0 flex-1 flex-col" onSubmit={(event) => void submit(event)}>
          <DialogHeader><DialogTitle>{t(model ? "providers.models.edit" : "providers.models.add")}</DialogTitle></DialogHeader>
          <DialogBody>
            <FieldGroup>
              <Field data-field-span="full">
                <FieldLabel htmlFor="provider-model-id">{t("providers.models.modelId")}</FieldLabel>
                <Input id="provider-model-id" className="machine-text" value={modelId} onChange={(event) => setModelId(event.target.value)} />
              </Field>
              <ProviderModelFields modelId={modelId} channel={provider.channel} value={metadata} onChange={setMetadata} />
            </FieldGroup>
            {error ? <FieldError>{error}</FieldError> : null}
          </DialogBody>
          <DialogFooter>
            <DialogClose asChild><Button type="button" variant="outline">{t("common.actions.cancel")}</Button></DialogClose>
            <Button type="submit" disabled={saving}>{t(saving ? "common.actions.saving" : "common.actions.save")}</Button>
          </DialogFooter>
        </form>
      </DialogContent>
    </Dialog>
  )
}
