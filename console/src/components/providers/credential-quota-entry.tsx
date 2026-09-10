import type { QuotaAllowance } from "@/generated/QuotaAllowance"
import type { QuotaEntry } from "@/generated/QuotaEntry"
import { useTranslation } from "react-i18next"
import { Meter } from "@/components/meter"
import { Badge } from "@/components/ui/badge"
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card"
import { formatInstant, formatPercent } from "@/lib/format"

function amount(value: string | null, unit: string | null) {
  return value == null ? "—" : `${value}${unit ? ` ${unit}` : ""}`
}

function Allowance({ value }: { value: QuotaAllowance }) {
  const { t, i18n } = useTranslation()
  const percent = value.unlimited ? null : value.used_percent != null ? Number(value.used_percent)
    : value.used != null && value.limit != null && Number(value.limit) > 0
      ? Number(value.used) / Number(value.limit) * 100 : null
  return <>
    <dl className="grid grid-cols-2 gap-x-4 gap-y-1 text-sm tabular-nums">
      <dt className="text-muted-foreground">{t("upstreamQuota.used")}</dt><dd className="text-right">{amount(value.used, value.unit)}</dd>
      <dt className="text-muted-foreground">{t("upstreamQuota.limit")}</dt><dd className="text-right">{value.unlimited ? t("upstreamQuota.unlimited") : amount(value.limit, value.unit)}</dd>
      <dt className="text-muted-foreground">{t("upstreamQuota.remaining")}</dt><dd className="text-right">{amount(value.remaining, value.unit)}</dd>
    </dl>
    {percent != null && Number.isFinite(percent) ? <div className="flex flex-col gap-2"><p className="text-right text-xs tabular-nums">{formatPercent(percent / 100, i18n.language)}</p><Meter percent={percent} /></div> : null}
    {value.period_start != null ? <p className="text-xs text-muted-foreground">{t("usage.cycleUsage.starts", { value: formatInstant(value.period_start, i18n.language) })}</p> : null}
    {value.period_end != null ? <p className="text-xs text-muted-foreground">{t("window.resets", { value: formatInstant(value.period_end, i18n.language) })}</p> : null}
  </>
}

export function CredentialQuotaEntry({ entry }: { entry: QuotaEntry }) {
  const { t, i18n } = useTranslation()
  const value = entry.value
  return <Card size="sm">
    <CardHeader>
      <CardTitle headingLevel={4}>{entry.label ? t(`upstreamQuota.entryLabels.${entry.label}`, { defaultValue: entry.label }) : t(`upstreamQuota.kinds.${value.kind}`)}</CardTitle>
      <CardDescription>{t(`upstreamQuota.subjects.${entry.subject}`)} · {entry.model_scope.kind === "models" ? entry.model_scope.models.join(", ") : t(`upstreamQuota.modelScope.${entry.model_scope.kind}`)}</CardDescription>
    </CardHeader>
    <CardContent className="flex flex-col gap-2">
      {value.kind === "balance" ? <>
        <p className="text-xl font-semibold tabular-nums">{amount(value.remaining, value.unit)}</p>
        <Badge className="self-start" variant={value.availability === "unavailable" ? "destructive" : "secondary"}>{t(`upstreamQuota.availability.${value.availability}`)}</Badge>
        {value.components.length ? <dl className="grid grid-cols-2 gap-x-4 gap-y-1 text-sm">
          {value.components.map((component) => <div className="contents" key={component.kind}><dt className="text-muted-foreground">{t(`upstreamQuota.components.${component.kind}`, { defaultValue: component.kind })}</dt><dd className="text-right tabular-nums">{amount(component.amount, value.unit)}</dd></div>)}
        </dl> : null}
      </> : value.kind === "usage_report" ? <>
        <p className="text-xl font-semibold tabular-nums">{amount(value.used, value.unit)}</p>
        <p className="text-xs text-muted-foreground">{t("upstreamQuota.reportPeriod", { start: formatInstant(value.period_start, i18n.language), end: formatInstant(value.period_end, i18n.language) })}</p>
      </> : <Allowance value={value} />}
      <p className="text-xs text-muted-foreground">{t("upstreamQuota.observed", { time: formatInstant(entry.observed_at_ms / 1000, i18n.language) })}</p>
    </CardContent>
  </Card>
}
