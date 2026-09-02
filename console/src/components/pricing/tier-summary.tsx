import { useTranslation } from "react-i18next"
import { Badge } from "@/components/ui/badge"
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card"
import { Separator } from "@/components/ui/separator"
import { PRICE_FIELDS, type TierDraft } from "./tier-values"

export function TierSummary({ tiers }: { tiers: Array<TierDraft> }) {
  const { t } = useTranslation()
  const context = tiers.filter((tier) => !tier.serviceTier.trim())
  const groups = [...new Set(tiers.map((tier) => tier.serviceTier.trim()).filter(Boolean))]
  if (!tiers.length) return <p className="text-sm text-muted-foreground">{t("pricing.tiers.empty")}</p>
  return <div className="flex flex-col gap-3">
    {context.length ? <SummaryGroup title={t("pricing.tiers.contextTitle")} description={t("pricing.tiers.contextDescription")} tiers={context} /> : null}
    {groups.map((group) => <SummaryGroup key={group} title={group} description={t("pricing.tiers.serviceGroupDescription")} tiers={tiers.filter((tier) => tier.serviceTier.trim() === group)} monospace />)}
  </div>
}

function SummaryGroup({ title, description, tiers, monospace = false }: { title: string; description: string; tiers: Array<TierDraft>; monospace?: boolean }) {
  return <Card size="sm">
    <CardHeader><CardTitle className={monospace ? "font-mono" : undefined}>{title}</CardTitle><CardDescription>{description}</CardDescription></CardHeader>
    <CardContent className="flex flex-col">
      {tiers.map((tier, index) => <div key={`${tier.threshold}-${index}`}>{index ? <Separator /> : null}<TierRow tier={tier} /></div>)}
    </CardContent>
  </Card>
}

function TierRow({ tier }: { tier: TierDraft }) {
  const { t } = useTranslation()
  const fields = PRICE_FIELDS.filter((field) => tier.prices[field])
  return <div className="flex flex-col gap-3 py-3">
    <div className="flex flex-wrap items-center gap-2">
      <Badge variant="outline">{t("pricing.tiers.thresholdSummary", { value: Number(tier.threshold || 0).toLocaleString() })}</Badge>
      {tier.multiplier ? <Badge variant="secondary">{t("pricing.tiers.multiplier")}: {tier.multiplier}</Badge> : null}
    </div>
    {fields.length ? <div className="grid gap-2 text-sm sm:grid-cols-2 lg:grid-cols-3">
      {fields.map((field) => <p key={field}>{t(`pricing.tiers.${field}`)}: <span className="font-mono">{tier.prices[field]}</span></p>)}
    </div> : null}
  </div>
}
