import { Trash2Icon } from "lucide-react"
import { useTranslation } from "react-i18next"
import { Button } from "@/components/ui/button"
import { Field, FieldLabel } from "@/components/ui/field"
import { Input } from "@/components/ui/input"
import { emptyTier, PRICE_FIELDS, type TierDraft } from "./tier-values"

export function TierEditor({ rows, onChange }: {
  rows: Array<TierDraft>
  onChange: (rows: Array<TierDraft>) => void
}) {
  const { t } = useTranslation()
  const patch = (index: number, next: Partial<TierDraft>) => {
    onChange(rows.map((row, rowIndex) => rowIndex === index ? { ...row, ...next } : row))
  }
  return (
    <Field>
      <div className="flex items-center justify-between gap-2">
        <FieldLabel>{t("pricing.tiers.title")}</FieldLabel>
        <Button type="button" size="sm" variant="outline" onClick={() => onChange([...rows, emptyTier()])}>
          {t("pricing.tiers.add")}
        </Button>
      </div>
      <div className="flex flex-col gap-3">
        {rows.map((row, index) => (
          <div key={index} className="flex flex-col gap-3 rounded-lg border p-3">
            <div className="grid gap-3 sm:grid-cols-[1fr_1fr_1fr_auto]">
              <Field><FieldLabel htmlFor={`tier-${index}-service`}>{t("pricing.tiers.serviceTier")}</FieldLabel><Input id={`tier-${index}-service`} value={row.serviceTier} onChange={(event) => patch(index, { serviceTier: event.target.value })} /></Field>
              <Field><FieldLabel htmlFor={`tier-${index}-threshold`}>{t("pricing.tiers.threshold")}</FieldLabel><Input id={`tier-${index}-threshold`} type="number" min={0} step={1} value={row.threshold} onChange={(event) => patch(index, { threshold: event.target.value })} /></Field>
              <Field><FieldLabel htmlFor={`tier-${index}-multiplier`}>{t("pricing.tiers.multiplier")}</FieldLabel><Input id={`tier-${index}-multiplier`} inputMode="decimal" value={row.multiplier} onChange={(event) => patch(index, { multiplier: event.target.value })} /></Field>
              <Button type="button" size="icon-sm" variant="ghost" aria-label={t("common.actions.delete")} onClick={() => onChange(rows.filter((_, rowIndex) => rowIndex !== index))}><Trash2Icon /></Button>
            </div>
            <div className="grid gap-3 sm:grid-cols-2 lg:grid-cols-3">
              {PRICE_FIELDS.map((field) => (
                <Field key={field}>
                  <FieldLabel htmlFor={`tier-${index}-${field}`}>{t(`pricing.tiers.${field}`)}</FieldLabel>
                  <Input id={`tier-${index}-${field}`} inputMode="decimal" value={row.prices[field]} onChange={(event) => patch(index, { prices: { ...row.prices, [field]: event.target.value } })} />
                </Field>
              ))}
            </div>
          </div>
        ))}
      </div>
    </Field>
  )
}
