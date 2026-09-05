import type { CredentialQuotaCycleDto } from "@/generated/CredentialQuotaCycleDto"
import type { QuotaProbeWindowDto } from "@/generated/QuotaProbeWindowDto"
import { useMemo } from "react"
import { useTranslation } from "react-i18next"
import { Meter } from "@/components/meter"
import { CredentialCycleModels } from "@/components/providers/credential-cycle-models"
import { CycleUsage } from "@/components/usage/cycle-usage"
import { Button } from "@/components/ui/button"
import { Collapsible, CollapsibleContent, CollapsibleTrigger } from "@/components/ui/collapsible"
import { formatInstant, formatNumber, formatPercent } from "@/lib/format"
import { windowName } from "@/lib/quota-window"

function CycleTile({ cycle, history }: { cycle: QuotaProbeWindowDto; history?: CredentialQuotaCycleDto }) {
  const { t, i18n } = useTranslation()
  const used = cycle.upstream_used ?? history?.upstream_used
  const nativeLimit = cycle.upstream_limit ?? history?.upstream_limit
  const limit = Number(nativeLimit)
  const percent = cycle.used_percent != null ? Number(cycle.used_percent)
    : used != null && limit > 0 ? Number(used) / limit * 100 : null
  const value = percent != null ? formatPercent(Math.round(percent) / 100, i18n.language)
    : used != null ? `${formatNumber(Number(used), i18n.language)}${nativeLimit != null ? ` / ${formatNumber(limit, i18n.language)}` : ""}` : "—"
  return <div className="grid min-w-0 gap-2 rounded-lg border bg-card px-3 py-2.5">
    <div className="flex flex-wrap items-baseline justify-between gap-3"><span className="text-sm font-medium">{windowName(cycle.window_key, t, cycle.label)}</span><span className="text-sm font-semibold tabular-nums">{value}</span></div>
    {percent != null ? <Meter percent={percent} /> : null}
    {percent != null && used != null && nativeLimit != null ? <p className="text-xs text-muted-foreground tabular-nums">{formatNumber(Number(used), i18n.language)} / {formatNumber(limit, i18n.language)} {cycle.unit}</p> : null}
    {history?.period_start != null ? <p className="text-xs text-muted-foreground">{t("usage.cycleUsage.starts", { value: formatInstant(history.period_start, i18n.language) })}</p> : null}
    {cycle.period_end != null ? <p className="text-xs text-muted-foreground">{t("window.resets", { value: formatInstant(cycle.period_end, i18n.language) })}</p> : null}
    {history ? <>
      <p className="text-xs text-muted-foreground">{t("usage.cycleUsage.observed", { value: formatInstant(history.last_observed_at, i18n.language) })}</p>
      {history.local_boundary && history.accounting_start_ms != null ? <p className="text-xs text-muted-foreground">{t("usage.cycleUsage.localBoundary", { value: formatInstant(history.accounting_start_ms / 1000, i18n.language) })}</p> : null}
      {history.status === "closed" ? <p className="text-xs text-muted-foreground">{t("common.status.closed")} · {history.close_reason ? t(`window.closeReason.${history.close_reason}`) : ""}</p> : null}
      <CycleUsage cycle={history} />
      <CredentialCycleModels values={history.models} />
    </> : null}
  </div>
}

type Props = {
  cycles: Array<CredentialQuotaCycleDto>
  windows?: Array<QuotaProbeWindowDto>
  loading: boolean
  error: boolean
  localError?: boolean
}

export function CredentialCycleList({ cycles, windows, loading, error, localError = false }: Props) {
  const { t } = useTranslation()
  const groups = useMemo(() => {
    const grouped = new Map<string, Array<CredentialQuotaCycleDto>>()
    for (const cycle of cycles) {
      const values = grouped.get(cycle.window_key) ?? []
      values.push(cycle)
      grouped.set(cycle.window_key, values)
    }
    for (const values of grouped.values()) values.sort((left, right) => right.id - left.id)
    return grouped
  }, [cycles])
  const keys = [...new Set([...groups.keys(), ...(windows ?? []).map((window) => window.window_key)])].sort()
  if (loading && !keys.length) return <p className="text-sm text-muted-foreground">{t("common.loading")}</p>
  if (error && !keys.length) return <p className="text-sm text-destructive">{t("common.errors.load")}</p>
  if (!keys.length) return <p className="text-sm text-muted-foreground">{t(windows ? "providers.credentials.quota.empty" : "providers.credentials.noQuotaCycle")}</p>
  return <div className="grid min-w-0 gap-3">
    {localError ? <p role="alert" className="text-sm text-muted-foreground">{t("usage.cycleUsage.localUnavailable")}</p> : null}
    {keys.map((key) => {
      const history = groups.get(key) ?? []
      const observed = windows?.find((window) => window.window_key === key)
      const latest = history[0]
      const cycle = localError ? observed ?? latest : latest ?? observed
      if (!cycle) return null
      return <div key={key} className="grid gap-2">
        <CycleTile cycle={cycle} history={localError ? undefined : latest} />
        {history.length > 1 ? <Collapsible><CollapsibleTrigger asChild><Button variant="ghost" size="sm">{t("usage.cycleUsage.history", { count: history.length - 1 })}</Button></CollapsibleTrigger><CollapsibleContent className="grid max-h-96 gap-2 overflow-y-auto pt-2">{history.slice(1).map((past) => <CycleTile key={past.id} cycle={past} history={past} />)}</CollapsibleContent></Collapsible> : null}
      </div>
    })}
  </div>
}
