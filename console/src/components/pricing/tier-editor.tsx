import { PlusIcon, Trash2Icon } from "lucide-react"
import { useTranslation } from "react-i18next"
import { SearchableSelect } from "@/components/searchable-select"
import { TierBand } from "@/components/pricing/tier-band"
import { Button } from "@/components/ui/button"
import { Card, CardAction, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card"
import { Empty, EmptyDescription, EmptyHeader, EmptyTitle } from "@/components/ui/empty"
import { Field, FieldLabel } from "@/components/ui/field"
import { emptyTier, type TierDraft } from "./tier-values"

export function TierEditor({ rows, serviceTiers, onChange }: {
  rows: Array<TierDraft>
  serviceTiers: Array<string>
  onChange: (rows: Array<TierDraft>) => void
}) {
  const { t } = useTranslation()
  const entries = rows.map((row, index) => ({ row, index }))
  const context = entries.filter(({ row }) => !row.serviceTier.trim())
  const groups = [...new Set(rows.map((row) => row.serviceTier.trim()).filter(Boolean))]
  const available = serviceTiers.filter((tier) => !groups.includes(tier))
  const patch = (index: number, next: Partial<TierDraft>) => onChange(
    rows.map((row, rowIndex) => rowIndex === index ? { ...row, ...next } : row),
  )
  const remove = (index: number) => onChange(rows.filter((_, rowIndex) => rowIndex !== index))
  const removeGroup = (tier: string) => onChange(rows.filter((row) => row.serviceTier.trim() !== tier))
  const renameGroup = (from: string, to: string) => onChange(rows.map((row) =>
    row.serviceTier.trim() === from ? { ...row, serviceTier: to } : row))
  const options = (current: string) => [...new Set([current, ...serviceTiers])]
    .filter((tier) => tier === current || !groups.includes(tier))
    .map((tier) => ({ value: tier, label: t(`pricing.tiers.values.${tier}`, { defaultValue: tier }), keywords: tier }))

  return <div data-field-span="full" className="flex flex-col gap-5">
    <Card>
      <CardHeader>
        <CardTitle>{t("pricing.tiers.contextTitle")}</CardTitle>
        <CardDescription>{t("pricing.tiers.contextDescription")}</CardDescription>
        <CardAction><Button type="button" size="sm" variant="outline" onClick={() => onChange([...rows, emptyTier()])}><PlusIcon data-icon="inline-start" />{t("pricing.tiers.addContext")}</Button></CardAction>
      </CardHeader>
      <CardContent className="flex flex-col gap-3">
        {context.length ? context.map((entry) => <TierBand key={entry.index} entry={entry} kind="context" onPatch={patch} onRemove={remove} />) : <TierEmpty title={t("pricing.tiers.contextEmpty")} description={t("pricing.tiers.contextEmptyDescription")} />}
      </CardContent>
    </Card>

    <div className="flex items-center justify-between gap-3">
      <div>
        <h3 className="font-medium">{t("pricing.tiers.serviceTitle")}</h3>
        <p className="text-sm text-muted-foreground">{t("pricing.tiers.serviceDescription")}</p>
      </div>
      <Button type="button" size="sm" variant="outline" disabled={!available.length} onClick={() => onChange([...rows, emptyTier(available[0], "0")])}><PlusIcon data-icon="inline-start" />{t("pricing.tiers.addService")}</Button>
    </div>
    {groups.length ? groups.map((tier) => {
      const tierRows = entries.filter(({ row }) => row.serviceTier.trim() === tier)
      return <Card key={tier}>
        <CardHeader>
          <CardTitle className="font-mono">{tier}</CardTitle>
          <CardDescription>{t("pricing.tiers.serviceGroupDescription")}</CardDescription>
          <CardAction><Button type="button" size="icon-sm" variant="ghost" aria-label={t("common.actions.delete")} onClick={() => removeGroup(tier)}><Trash2Icon /></Button></CardAction>
        </CardHeader>
        <CardContent className="flex flex-col gap-4">
          <Field>
            <FieldLabel htmlFor={`service-tier-${tier}`}>{t("pricing.tiers.serviceTier")}</FieldLabel>
            <SearchableSelect id={`service-tier-${tier}`} value={tier} options={options(tier)} placeholder={t("pricing.tiers.selectService")} searchPlaceholder={t("pricing.tiers.searchService")} emptyLabel={t("pricing.tiers.noServiceMatches")} ariaLabel={t("pricing.tiers.serviceTier")} onChange={(next) => renameGroup(tier, next)} />
          </Field>
          <div className="flex flex-col gap-3">
            {tierRows.map((entry) => <TierBand key={entry.index} entry={entry} kind="service" onPatch={patch} onRemove={remove} />)}
          </div>
          <Button type="button" size="sm" variant="ghost" className="self-start" onClick={() => onChange([...rows, emptyTier(tier)])}><PlusIcon data-icon="inline-start" />{t("pricing.tiers.addServiceContext")}</Button>
        </CardContent>
      </Card>
    }) : <TierEmpty title={t("pricing.tiers.serviceEmpty")} description={t("pricing.tiers.serviceEmptyDescription")} />}
  </div>
}

function TierEmpty({ title, description }: { title: string; description: string }) {
  return <Empty className="min-h-28 border"><EmptyHeader><EmptyTitle>{title}</EmptyTitle><EmptyDescription>{description}</EmptyDescription></EmptyHeader></Empty>
}
