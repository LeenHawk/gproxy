import { useId } from "react"
import { useTranslation } from "react-i18next"
import { Field, FieldContent, FieldDescription, FieldLabel, FieldLegend, FieldSet } from "@/components/ui/field"
import { Input } from "@/components/ui/input"
import { Switch } from "@/components/ui/switch"
import type { ModelMetadataState } from "@/components/providers/provider-model-state"
import { ProviderModelVariants } from "@/components/providers/provider-model-variants"

export function ProviderModelFields({ modelId, channel, value, onChange }: { modelId: string; channel: string; value: ModelMetadataState; onChange: (value: ModelMetadataState) => void }) {
  const { t } = useTranslation()
  const id = useId()
  const set = <K extends keyof ModelMetadataState>(key: K, next: ModelMetadataState[K]) => onChange({ ...value, [key]: next })
  return <>
    <Field>
      <FieldLabel htmlFor={`${id}-display`}>{t("providers.models.displayName")}</FieldLabel>
      <Input id={`${id}-display`} value={value.displayName} onChange={(event) => set("displayName", event.target.value)} />
    </Field>
    <Field>
      <FieldLabel htmlFor={`${id}-context`}>{t("providers.models.contextWindow")}</FieldLabel>
      <Input id={`${id}-context`} type="number" min="1" inputMode="numeric" value={value.contextWindow} onChange={(event) => set("contextWindow", event.target.value)} />
    </Field>
    <Field>
      <FieldLabel htmlFor={`${id}-output`}>{t("providers.models.maxOutputTokens")}</FieldLabel>
      <Input id={`${id}-output`} type="number" min="1" inputMode="numeric" value={value.maxOutputTokens} onChange={(event) => set("maxOutputTokens", event.target.value)} />
    </Field>
    <FieldSet data-field-span="full">
      <FieldLegend variant="label">{t("providers.models.thinking")}</FieldLegend>
      <FieldDescription>{t("providers.models.thinkingHint")}</FieldDescription>
      <div className="grid gap-2 sm:grid-cols-3">
        <ThinkingField label={t("providers.models.thinkingSupported")} value={value.thinkingSupported} onChange={(next) => set("thinkingSupported", next)} />
        <ThinkingField label={t("providers.models.thinkingAdaptiveSupported")} value={value.thinkingAdaptiveSupported} onChange={(next) => set("thinkingAdaptiveSupported", next)} />
        <ThinkingField label={t("providers.models.thinkingEnabledSupported")} value={value.thinkingEnabledSupported} onChange={(next) => set("thinkingEnabledSupported", next)} />
      </div>
    </FieldSet>
    <ProviderModelVariants
      modelId={modelId}
      channel={channel}
      rows={value.variants}
      exposeBase={value.exposeBase}
      onRowsChange={(rows) => set("variants", rows)}
      onExposeBaseChange={(expose) => set("exposeBase", expose)}
    />
  </>
}

function ThinkingField({ label, value, onChange }: { label: string; value: boolean | null; onChange: (value: boolean) => void }) {
  const id = useId()
  return <Field orientation="horizontal" className="rounded-lg border px-3 py-2.5">
    <FieldContent>
      <FieldLabel htmlFor={id}>{label}</FieldLabel>
    </FieldContent>
    <Switch id={id} checked={value === true} onCheckedChange={onChange} />
  </Field>
}
