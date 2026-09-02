import { useState } from "react"
import { ChevronDownIcon, PlusIcon, Trash2Icon } from "lucide-react"
import { useTranslation } from "react-i18next"
import type { PriceProfileDto } from "@/generated/PriceProfileDto"
import type { PriceRateDto } from "@/generated/PriceRateDto"
import type { PriceRuleDto } from "@/generated/PriceRuleDto"
import { PriceRateDialog } from "@/components/pricing/price-rate-dialog"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { Card, CardAction, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card"
import { Collapsible, CollapsibleContent, CollapsibleTrigger } from "@/components/ui/collapsible"
import { Separator } from "@/components/ui/separator"

export function PriceRateGroup({ profile, title, description, customRates, rule, rules, rates, deleting, onDelete, collapsible = false }: {
  profile?: PriceProfileDto
  title?: string
  description?: string
  customRates?: Array<PriceRateDto>
  rule: PriceRuleDto
  rules: Array<PriceRuleDto>
  rates: Array<PriceRateDto>
  deleting: boolean
  onDelete: (id: number) => void
  collapsible?: boolean
}) {
  const { t } = useTranslation()
  const configured = customRates ?? rates.filter((rate) => profile?.metrics.some((metric) => metric.metric === rate.metric))
  const [open, setOpen] = useState(!collapsible || configured.length > 0)
  const heading = title ?? t(`pricing.profiles.${profile?.kind}.title`)
  const detail = description ?? t(`pricing.profiles.${profile?.kind}.description`)
  return <Card>
    <Collapsible open={open} onOpenChange={setOpen}>
      <CardHeader>
        <CardTitle>{heading}</CardTitle>
        <CardDescription>{detail}</CardDescription>
        {collapsible ? <CardAction><CollapsibleTrigger asChild><Button size="icon-sm" variant="ghost" aria-label={t("pricing.rates.toggleGroup")}><ChevronDownIcon className="group-data-[state=open]:rotate-180" /></Button></CollapsibleTrigger></CardAction> : null}
      </CardHeader>
      <CollapsibleContent>
        <CardContent className="flex flex-col">
          {customRates ? [...new Set(customRates.map((rate) => rate.metric))].map((metric, index) => <div key={metric}>{index ? <Separator /> : null}<MetricRow metric={metric} unitSize={customRates.find((rate) => rate.metric === metric)?.unit_size ?? 1} configured={customRates.filter((rate) => rate.metric === metric)} rules={rules} rule={rule} deleting={deleting} onDelete={onDelete} /></div>) : profile?.metrics.map((metric, index) => <div key={metric.metric}>{index ? <Separator /> : null}<MetricRow metric={metric.metric} unitSize={metric.unit_size} configured={rates.filter((rate) => rate.metric === metric.metric)} rules={rules} rule={rule} deleting={deleting} onDelete={onDelete} /></div>)}
        </CardContent>
      </CollapsibleContent>
    </Collapsible>
  </Card>
}

function MetricRow({ metric, unitSize, configured, rules, rule, deleting, onDelete }: {
  metric: string
  unitSize: number
  configured: Array<PriceRateDto>
  rules: Array<PriceRuleDto>
  rule: PriceRuleDto
  deleting: boolean
  onDelete: (id: number) => void
}) {
  const { t } = useTranslation()
  return <div className="flex flex-col gap-3 py-3 sm:flex-row sm:items-center sm:justify-between">
    <div className="min-w-0"><p className="font-medium">{t(`pricing.metrics.${metric}`, { defaultValue: metric })}</p><p className="font-mono text-xs text-muted-foreground">{metric} · {t(unitSize === 1 ? "pricing.rates.perUnit" : "pricing.rates.perMillion")}</p></div>
    <div className="flex flex-wrap items-center gap-2">
      {configured.map((rate) => <RateValue key={rate.id} rate={rate} rules={rules} rule={rule} deleting={deleting} onDelete={onDelete} />)}
      <PriceRateDialog rules={rules} fixedRuleId={rule.id} initialMetric={metric} initialUnitSize={unitSize} lockedMetric trigger={<Button size="sm" variant="outline"><PlusIcon data-icon="inline-start" />{t("common.actions.add")}</Button>} />
    </div>
  </div>
}

function RateValue({ rate, rules, rule, deleting, onDelete }: { rate: PriceRateDto; rules: Array<PriceRuleDto>; rule: PriceRuleDto; deleting: boolean; onDelete: (id: number) => void }) {
  const { t } = useTranslation()
  const conditions = rate.conditions && typeof rate.conditions === "object" && !Array.isArray(rate.conditions) ? Object.keys(rate.conditions as object).length : 0
  return <div className="flex items-center gap-1">
    <PriceRateDialog rate={rate} rules={rules} fixedRuleId={rule.id} lockedMetric trigger={<Button size="sm" variant="ghost"><Badge variant="secondary">{rate.price}{conditions ? ` · ${t("pricing.rates.conditionCount", { count: conditions })}` : ""}</Badge></Button>} />
    <Button size="icon-xs" variant="ghost" disabled={deleting} aria-label={t("common.actions.delete")} onClick={() => onDelete(rate.id)}><Trash2Icon /></Button>
  </div>
}
