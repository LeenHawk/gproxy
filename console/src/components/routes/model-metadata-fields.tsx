import { useId } from "react"
import { useTranslation } from "react-i18next"
import { Field, FieldDescription, FieldLabel } from "@/components/ui/field"
import { Input } from "@/components/ui/input"
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select"
import { Textarea } from "@/components/ui/textarea"
import type { ModelMetadataState, TriState } from "@/components/routes/model-metadata"

export function ModelMetadataFields({ value, onChange }: { value: ModelMetadataState; onChange: (value: ModelMetadataState) => void }) {
  const { t } = useTranslation()
  const id = useId()
  const set = <K extends keyof ModelMetadataState>(key: K, next: ModelMetadataState[K]) => onChange({ ...value, [key]: next })
  return <>
    <Field>
      <FieldLabel htmlFor={`${id}-display`}>{t("routes.aliases.displayName")}</FieldLabel>
      <Input id={`${id}-display`} value={value.displayName} onChange={(event) => set("displayName", event.target.value)} />
    </Field>
    <Field>
      <FieldLabel htmlFor={`${id}-context`}>{t("routes.aliases.contextWindow")}</FieldLabel>
      <Input id={`${id}-context`} type="number" min="1" inputMode="numeric" value={value.contextWindow} onChange={(event) => set("contextWindow", event.target.value)} />
    </Field>
    <Field>
      <FieldLabel htmlFor={`${id}-output`}>{t("routes.aliases.maxOutputTokens")}</FieldLabel>
      <Input id={`${id}-output`} type="number" min="1" inputMode="numeric" value={value.maxOutputTokens} onChange={(event) => set("maxOutputTokens", event.target.value)} />
    </Field>
    <ThinkingField label={t("routes.aliases.thinkingSupported")} value={value.thinkingSupported} onChange={(next) => set("thinkingSupported", next)} />
    <ThinkingField label={t("routes.aliases.thinkingAdaptiveSupported")} value={value.thinkingAdaptiveSupported} onChange={(next) => set("thinkingAdaptiveSupported", next)} />
    <ThinkingField label={t("routes.aliases.thinkingEnabledSupported")} value={value.thinkingEnabledSupported} onChange={(next) => set("thinkingEnabledSupported", next)} />
    <Field>
      <FieldLabel htmlFor={`${id}-variants`}>{t("routes.aliases.variants")}</FieldLabel>
      <Textarea id={`${id}-variants`} className="min-h-28 font-mono text-xs" value={value.variants} onChange={(event) => set("variants", event.target.value)} />
      <FieldDescription>{t("routes.aliases.variantsHint")}</FieldDescription>
    </Field>
  </>
}

function ThinkingField({ label, value, onChange }: { label: string; value: TriState; onChange: (value: TriState) => void }) {
  const { t } = useTranslation()
  const id = useId()
  return <Field>
    <FieldLabel htmlFor={id}>{label}</FieldLabel>
    <Select value={value} onValueChange={(next) => onChange(next as TriState)}>
      <SelectTrigger id={id} className="w-full"><SelectValue /></SelectTrigger>
      <SelectContent>
        <SelectItem value="unset">{t("common.none")}</SelectItem>
        <SelectItem value="true">{t("common.yes")}</SelectItem>
        <SelectItem value="false">{t("common.no")}</SelectItem>
      </SelectContent>
    </Select>
  </Field>
}
