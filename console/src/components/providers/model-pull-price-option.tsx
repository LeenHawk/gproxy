import { useTranslation } from "react-i18next"
import { Field, FieldContent, FieldDescription, FieldLabel } from "@/components/ui/field"
import { Switch } from "@/components/ui/switch"

export function ModelPullPriceOption({ checked, disabled, onCheckedChange }: {
  checked: boolean
  disabled: boolean
  onCheckedChange: (checked: boolean) => void
}) {
  const { t } = useTranslation()
  return <Field orientation="horizontal" className="rounded-md border px-3 py-2">
    <FieldContent>
      <FieldLabel htmlFor="pull-default-prices">{t("providers.models.pullDefaultPrices")}</FieldLabel>
      <FieldDescription>{t("providers.models.pullDefaultPricesHint")}</FieldDescription>
    </FieldContent>
    <Switch id="pull-default-prices" checked={checked} disabled={disabled} onCheckedChange={onCheckedChange} />
  </Field>
}
