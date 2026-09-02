import { useId } from "react"
import { PlusIcon, XIcon } from "lucide-react"
import { useTranslation } from "react-i18next"
import { Button } from "@/components/ui/button"
import { Field, FieldContent, FieldDescription, FieldGroup, FieldLabel, FieldLegend, FieldSet } from "@/components/ui/field"
import { Input } from "@/components/ui/input"
import { Switch } from "@/components/ui/switch"

export function ProviderModelVariants({ modelId, names, exposeBase, onNamesChange, onExposeBaseChange }: {
  modelId: string
  names: Array<string>
  exposeBase: boolean
  onNamesChange: (names: Array<string>) => void
  onExposeBaseChange: (expose: boolean) => void
}) {
  const { t } = useTranslation()
  const id = useId()
  const setName = (index: number, name: string) => onNamesChange(names.map((current, itemIndex) => itemIndex === index ? name : current))
  const remove = (index: number) => onNamesChange(names.filter((_, itemIndex) => itemIndex !== index))
  const placeholder = `${modelId.trim() || "gpt-5"}-thinking-high`

  return <FieldSet data-field-span="full">
    <FieldLegend variant="label">{t("providers.models.variants")}</FieldLegend>
    <FieldDescription>{t("providers.models.variantsHint")}</FieldDescription>
    <FieldGroup className="gap-3 sm:grid-cols-1">
      {names.length === 0 ? <FieldDescription>{t("providers.models.variantsEmpty")}</FieldDescription> : null}
      {names.map((name, index) => <Field key={index} orientation="horizontal">
        <Input
          id={`${id}-variant-${index}`}
          name="model-variant"
          className="machine-text text-xs"
          value={name}
          placeholder={placeholder}
          aria-label={t("providers.models.variantName")}
          onChange={(event) => setName(index, event.target.value)}
        />
        <Button type="button" size="icon-sm" variant="ghost" aria-label={t("providers.models.variantRemove")} onClick={() => remove(index)}>
          <XIcon aria-hidden />
        </Button>
      </Field>)}
      <Button type="button" size="sm" variant="outline" className="justify-self-start" onClick={() => onNamesChange([...names, ""])}>
        <PlusIcon data-icon="inline-start" aria-hidden />
        {t("providers.models.addVariant")}
      </Button>
      <Field orientation="horizontal">
        <FieldContent>
          <FieldLabel htmlFor={`${id}-expose-base`}>{t("providers.models.exposeBase")}</FieldLabel>
        </FieldContent>
        <Switch id={`${id}-expose-base`} checked={exposeBase} onCheckedChange={onExposeBaseChange} />
      </Field>
    </FieldGroup>
  </FieldSet>
}
