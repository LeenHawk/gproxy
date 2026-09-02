import { useState } from "react"
import { ChevronDownIcon, Trash2Icon } from "lucide-react"
import { useTranslation } from "react-i18next"
import { Button } from "@/components/ui/button"
import { Card, CardAction, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card"
import { Collapsible, CollapsibleContent, CollapsibleTrigger } from "@/components/ui/collapsible"
import { Field, FieldDescription, FieldGroup, FieldLabel } from "@/components/ui/field"
import { Input } from "@/components/ui/input"
import { PRICE_FIELDS, type TierDraft } from "./tier-values"

type Entry = { index: number; row: TierDraft }

export function TierBand({ entry, kind, onPatch, onRemove }: {
  entry: Entry
  kind: "context" | "service"
  onPatch: (index: number, next: Partial<TierDraft>) => void
  onRemove: (index: number) => void
}) {
  const { t } = useTranslation()
  const count = PRICE_FIELDS.filter((field) => entry.row.prices[field].trim()).length
  const [pricesOpen, setPricesOpen] = useState(false)
  const id = `tier-band-${entry.index}`
  const threshold = entry.row.threshold.trim()
  return <Card size="sm">
    <CardHeader>
      <CardTitle>{threshold ? t("pricing.tiers.thresholdSummary", { value: Number(threshold).toLocaleString() }) : t("pricing.tiers.newThreshold")}</CardTitle>
      <CardDescription>{t(kind === "context" ? "pricing.tiers.contextBandDescription" : "pricing.tiers.serviceBandDescription")}</CardDescription>
      <CardAction><Button type="button" size="icon-sm" variant="ghost" aria-label={t("common.actions.delete")} onClick={() => onRemove(entry.index)}><Trash2Icon /></Button></CardAction>
    </CardHeader>
    <CardContent className="flex flex-col gap-4">
      <FieldGroup>
        <Field>
          <FieldLabel htmlFor={`${id}-threshold`}>{t("pricing.tiers.threshold")}</FieldLabel>
          <Input id={`${id}-threshold`} type="number" min={0} step={1} required placeholder="0" value={entry.row.threshold} onChange={(event) => onPatch(entry.index, { threshold: event.target.value })} />
          <FieldDescription>{t(kind === "context" ? "pricing.tiers.contextThresholdHint" : "pricing.tiers.serviceThresholdHint")}</FieldDescription>
        </Field>
        {kind === "service" ? <Field>
          <FieldLabel htmlFor={`${id}-multiplier`}>{t("pricing.tiers.multiplier")}</FieldLabel>
          <Input id={`${id}-multiplier`} inputMode="decimal" placeholder="1" value={entry.row.multiplier} onChange={(event) => onPatch(entry.index, { multiplier: event.target.value })} />
          <FieldDescription>{t("pricing.tiers.multiplierHint")}</FieldDescription>
        </Field> : null}
      </FieldGroup>
      <Collapsible open={pricesOpen} onOpenChange={setPricesOpen}>
        <CollapsibleTrigger asChild>
          <Button type="button" variant="ghost" className="group w-full justify-start">
            {t("pricing.tiers.explicitPrices")}{count ? ` · ${count}` : ""}
            <ChevronDownIcon data-icon="inline-end" className="ml-auto group-data-[state=open]:rotate-180" />
          </Button>
        </CollapsibleTrigger>
        <CollapsibleContent className="pt-3">
          <FieldGroup>
            {PRICE_FIELDS.map((field) => <Field key={field}>
              <FieldLabel htmlFor={`${id}-${field}`}>{t(`pricing.tiers.${field}`)}</FieldLabel>
              <Input id={`${id}-${field}`} inputMode="decimal" value={entry.row.prices[field]} onChange={(event) => onPatch(entry.index, { prices: { ...entry.row.prices, [field]: event.target.value } })} />
            </Field>)}
          </FieldGroup>
          <p className="mt-3 text-sm text-muted-foreground">{t("pricing.tiers.explicitPricesHint")}</p>
        </CollapsibleContent>
      </Collapsible>
    </CardContent>
  </Card>
}
