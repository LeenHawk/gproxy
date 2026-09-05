import { useMemo, useState } from "react"
import { useTranslation } from "react-i18next"
import type { CredentialDto } from "@/generated/CredentialDto"
import type { CredentialQuotaCycleDto } from "@/generated/CredentialQuotaCycleDto"
import type { ProviderDto } from "@/generated/ProviderDto"
import { QueryState } from "@/components/query-state"
import { ToggleGroup, ToggleGroupItem } from "@/components/ui/toggle-group"
import { WindowList } from "@/components/usage/window-list"
import { formatInstant } from "@/lib/format"
import { QuotaHistoryChart } from "./quota-history-chart"
import { quotaSeries, type QuotaMetric } from "./quota-history-data"
import { QuotaHistoryFilter } from "./quota-history-filter"

export function QuotaHistory({ cycles, providers, credentials, loading, error }: {
  cycles: Array<CredentialQuotaCycleDto>; providers: Array<ProviderDto>; credentials: Array<CredentialDto>; loading: boolean; error: boolean
}) {
  const { t, i18n } = useTranslation()
  const [metric, setMetric] = useState<QuotaMetric>("percent")
  const [excludedProviders, setExcludedProviders] = useState(new Set<string>())
  const [excludedSeries, setExcludedSeries] = useState(new Set<string>())
  const [excludedRounds, setExcludedRounds] = useState(new Set<string>())
  const series = useMemo(() => quotaSeries(cycles, credentials, providers, t), [cycles, credentials, providers, t])
  const providerOptions = Array.from(new Map(series.map((series) => [series.providerId, { value: series.providerId, label: series.provider }])).values())
  const available = series.filter((series) => !excludedProviders.has(series.providerId))
  const selected = available.filter((series) => !excludedSeries.has(series.id))
  const roundOptions = selected.flatMap((series) => series.cycles.map((cycle) => ({ value: String(cycle.id), label: `${series.label} · ${formatInstant(cycle.accounting_start_ms / 1000, i18n.language)} · #${cycle.id}` })))
  const visible = selected.map((series) => ({ ...series, cycles: series.cycles.filter((cycle) => !excludedRounds.has(String(cycle.id))) })).filter((series) => series.cycles.length > 0)
  const shown = visible.flatMap((series) => series.cycles).sort((left, right) => right.accounting_start_ms - left.accounting_start_ms || right.id - left.id)
  const labels = new Map(visible.flatMap((series) => series.cycles.map((cycle) => [cycle.id, series.label] as const)))
  return <section className="flex min-w-0 flex-col gap-5" aria-label={t("usage.quotaHistory.title")}>
    <h2 className="text-base font-semibold">{t("usage.quotaHistory.title")}</h2>
    <QueryState loading={loading} error={error ? t("common.loadError") : ""}>
      <div className="grid min-w-0 gap-3 sm:grid-cols-3">
        <QuotaHistoryFilter label={t("usage.quotaHistory.providers")} options={providerOptions} excluded={excludedProviders} onChange={setExcludedProviders} />
        <QuotaHistoryFilter label={t("usage.quotaHistory.series")} options={available.map((series) => ({ value: series.id, label: series.label }))} excluded={excludedSeries} onChange={setExcludedSeries} />
        <QuotaHistoryFilter label={t("usage.quotaHistory.rounds")} options={roundOptions} excluded={excludedRounds} onChange={setExcludedRounds} />
      </div>
      <ToggleGroup type="single" variant="outline" value={metric} className="flex-wrap justify-start" onValueChange={(value) => { if (value) setMetric(value as QuotaMetric) }} aria-label={t("usage.quotaHistory.metric")}>
        {(["percent", "tokens", "cost"] as const).map((metric) => <ToggleGroupItem key={metric} value={metric}>{t(`usage.quotaHistory.metrics.${metric}`)}</ToggleGroupItem>)}
      </ToggleGroup>
      <QuotaHistoryChart series={visible} metric={metric} mode="within" />
      <QuotaHistoryChart series={visible} metric={metric} mode="across" />
      <WindowList cycles={shown} labels={labels} />
    </QueryState>
  </section>
}
