import type { CredentialQuotaCycleDto } from "@/generated/CredentialQuotaCycleDto"
import type { QuotaProbeWindowDto } from "@/generated/QuotaProbeWindowDto"
import { useMemo } from "react"
import { useTranslation } from "react-i18next"
import { Meter } from "@/components/meter"
import { CredentialCycleModels } from "@/components/providers/credential-cycle-models"
import { formatInstant, formatNumber, formatPercent } from "@/lib/format"
import { windowName } from "@/lib/quota-window"

function latestCycles(cycles: Array<CredentialQuotaCycleDto>) {
  const byKey = new Map<string, CredentialQuotaCycleDto>()
  for (const cycle of cycles) {
    const current = byKey.get(cycle.window_key)
    if (!current || cycle.last_observed_at > current.last_observed_at) byKey.set(cycle.window_key, cycle)
  }
  return [...byKey.values()].sort((left, right) => left.window_key.localeCompare(right.window_key))
}

function cyclePercent(cycle: QuotaProbeWindowDto, history?: CredentialQuotaCycleDto) {
  const limit = Number(history?.upstream_limit)
  const percent = cycle.used_percent != null
    ? Number(cycle.used_percent)
    : history?.upstream_used != null && history.upstream_limit != null && limit > 0
      ? (Number(history.upstream_used) / limit) * 100
      : null
  return percent != null && Number.isFinite(percent) ? Math.min(100, Math.max(0, percent)) : null
}

function CycleTile({ cycle, history }: { cycle: QuotaProbeWindowDto; history?: CredentialQuotaCycleDto }) {
  const { t, i18n } = useTranslation()
  const percent = cyclePercent(cycle, history)
  const value = percent != null
    ? formatPercent(Math.round(percent) / 100, i18n.language)
    : history?.upstream_used != null
      ? `${formatNumber(Number(history.upstream_used), i18n.language)}${history.upstream_limit != null ? ` / ${formatNumber(Number(history.upstream_limit), i18n.language)}` : ""}`
      : "—"
  const reset = history?.status !== "closed" && cycle.period_end != null ? formatInstant(cycle.period_end, i18n.language) : null
  const closed = history?.status === "closed"
    ? [t("common.status.closed"), history.close_reason ? t(`window.closeReason.${history.close_reason}`) : null].filter(Boolean).join(" · ")
    : null
  return (
    <div className="grid min-w-0 gap-1.5 rounded-lg border bg-card px-3 py-2.5">
      <div className="flex flex-wrap items-baseline justify-between gap-x-3 gap-y-0.5">
        <span className="text-sm font-medium">{windowName(cycle.window_key, t, cycle.label)}</span>
        <span className="text-sm font-semibold tabular-nums">{value}</span>
      </div>
      {percent != null ? <Meter percent={percent} /> : null}
      {reset ? <p className="text-xs text-muted-foreground">{t("window.resets", { value: reset })}</p> : null}
      {closed ? <p className="text-xs text-muted-foreground">{closed}</p> : null}
      {history ? <CredentialCycleModels values={history.models} /> : null}
    </div>
  )
}

type Props = {
  cycles: Array<CredentialQuotaCycleDto>
  windows?: Array<QuotaProbeWindowDto>
  loading: boolean
  error: boolean
}

export function CredentialCycleList({ cycles, windows, loading, error }: Props) {
  const { t } = useTranslation()
  const latest = useMemo(() => latestCycles(cycles), [cycles])

  if (windows) {
    if (!windows.length) return <p className="text-sm text-muted-foreground">{t("providers.credentials.quota.empty")}</p>
    return <div className="grid min-w-0 gap-2">{windows.map((window) => <CycleTile key={window.window_key} cycle={window} />)}</div>
  }
  if (loading) return <p className="text-sm text-muted-foreground">{t("common.loading")}</p>
  if (error) return <p className="text-sm text-destructive">{t("common.errors.load")}</p>
  if (!latest.length) return <p className="text-sm text-muted-foreground">{t("providers.credentials.noQuotaCycle")}</p>
  return <div className="grid min-w-0 gap-2">{latest.map((cycle) => <CycleTile key={cycle.id} cycle={cycle} history={cycle} />)}</div>
}
