import { useState } from "react"
import { useTranslation } from "react-i18next"
import type { ProviderModelDto } from "@/generated/ProviderModelDto"
import type { ProviderModelWriteRequest } from "@/generated/ProviderModelWriteRequest"
import { ProviderModelFields } from "@/components/providers/provider-model-fields"
import { providerModelRequest, providerModelState } from "@/components/providers/provider-model-state"
import { Button } from "@/components/ui/button"
import { Dialog, DialogBody, DialogClose, DialogContent, DialogFooter, DialogHeader, DialogTitle } from "@/components/ui/dialog"
import { Field, FieldError, FieldGroup, FieldLabel } from "@/components/ui/field"
import { Input } from "@/components/ui/input"

export function ProviderModelDialog({ open, onOpenChange, providerId, model, saving, onSave }: {
  open: boolean
  onOpenChange: (open: boolean) => void
  providerId: number
  model?: ProviderModelDto
  saving: boolean
  onSave: (value: ProviderModelWriteRequest, id?: number) => Promise<void>
}) {
  const { t } = useTranslation()
  const [modelId, setModelId] = useState(model?.model_id ?? "")
  const [metadata, setMetadata] = useState(() => providerModelState(model ?? null))
  const [error, setError] = useState("")

  const submit = async (event: React.FormEvent) => {
    event.preventDefault()
    if (!modelId.trim()) {
      setError(t("common.errors.required"))
      return
    }
    const fields = providerModelRequest(metadata)
    setError("")
    await onSave({ provider_id: providerId, model_id: modelId.trim(), enabled: model?.enabled ?? true, ...fields }, model?.id)
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
              <ProviderModelFields modelId={modelId} value={metadata} onChange={setMetadata} />
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
