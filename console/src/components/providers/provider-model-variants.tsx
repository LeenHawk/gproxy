import { useId, useState } from "react"
import { PlusIcon, XIcon } from "lucide-react"
import { useTranslation } from "react-i18next"
import type { VariantRuleRow } from "@/components/providers/provider-model-variant-rules"
import { VariantPresetPicker } from "@/components/providers/variant-preset-picker"
import { Button } from "@/components/ui/button"
import { Field, FieldContent, FieldDescription, FieldGroup, FieldLabel, FieldLegend, FieldSet } from "@/components/ui/field"
import { Input } from "@/components/ui/input"
import { Switch } from "@/components/ui/switch"

export function ProviderModelVariants({ modelId, channel, rows, exposeBase, onRowsChange, onExposeBaseChange }: {
  modelId: string
  channel: string
  rows: Array<VariantRuleRow>
  exposeBase: boolean
  onRowsChange: (rows: Array<VariantRuleRow>) => void
  onExposeBaseChange: (expose: boolean) => void
}) {
  const { t } = useTranslation()
  const id = useId()
  const [picking, setPicking] = useState<number | null>(null)
  const setName = (index: number, name: string) => onRowsChange(rows.map((row, itemIndex) => itemIndex === index ? { ...row, name, touched: true } : row))
  const remove = (index: number) => onRowsChange(rows.filter((_, itemIndex) => itemIndex !== index))
  const placeholder = `${modelId.trim() || "gpt-5"}-thinking-high`

  return <FieldSet data-field-span="full">
    <FieldLegend variant="label">{t("providers.models.variants")}</FieldLegend>
    <FieldDescription>{t("providers.models.variantsHint")}</FieldDescription>
    <FieldGroup className="gap-3 sm:grid-cols-1">
      {rows.length === 0 ? <FieldDescription>{t("providers.models.variantsEmpty")}</FieldDescription> : null}
      {rows.map((row, index) => <div key={index} className="grid gap-2 rounded-md border bg-muted/20 p-3">
        <Field orientation="horizontal">
          <Input id={`${id}-variant-${index}`} name="model-variant" className="machine-text text-xs" value={row.name} placeholder={placeholder} aria-label={t("providers.models.variantName")} onChange={(event) => setName(index, event.target.value)} />
          <Button type="button" size="icon-sm" variant="ghost" aria-label={t("providers.models.variantRemove")} onClick={() => remove(index)}><XIcon aria-hidden /></Button>
        </Field>
        <div className="flex items-center justify-between gap-3 text-xs">
          <span className="min-w-0 truncate text-muted-foreground">{row.actions.length > 0 ? row.actions.map((action) => action.path).join(", ") : t("providers.models.variantNoBehavior")}</span>
          <Button type="button" size="sm" variant="outline" onClick={() => setPicking(index)}>{t("providers.models.variantSetBehavior")}</Button>
        </div>
        {picking === index ? <VariantPresetPicker
          modelId={modelId.trim()}
          channel={channel}
          initialActions={row.actions}
          onCancel={() => setPicking(null)}
          onApply={(actions, suffix) => {
            onRowsChange(rows.map((current, itemIndex) => itemIndex === index ? { ...current, name: current.name.trim() || `${modelId.trim()}${suffix}`, actions, touched: true } : current))
            setPicking(null)
          }}
        /> : null}
      </div>)}
      <Button type="button" size="sm" variant="outline" className="justify-self-start" onClick={() => onRowsChange([...rows, { name: "", actions: [], touched: false }])}>
        <PlusIcon data-icon="inline-start" aria-hidden />
        {t("providers.models.addVariant")}
      </Button>
      <Field orientation="horizontal">
        <FieldContent><FieldLabel htmlFor={`${id}-expose-base`}>{t("providers.models.exposeBase")}</FieldLabel></FieldContent>
        <Switch id={`${id}-expose-base`} checked={exposeBase} onCheckedChange={onExposeBaseChange} />
      </Field>
    </FieldGroup>
  </FieldSet>
}
