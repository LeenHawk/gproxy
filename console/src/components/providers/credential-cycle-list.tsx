import type { CredentialQuotaCycleDto } from "@/generated/CredentialQuotaCycleDto"
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

function cyclePercent(cycle: CredentialQuotaCycleDto) {
  const limit = Number(cycle.upstream_limit)
  const percent = cycle.used_percent != null
    ? Number(cycle.used_percent)
    : cycle.upstream_used != null && cycle.upstream_limit != null && limit > 0
      ? (Number(cycle.upstream_used) / limit) * 100
      : null
  return percent != null && Number.isFinite(percent) ? Math.min(100, Math.max(0, percent)) : null
}

function CycleTile({ cycle }: { cycle: CredentialQuotaCycleDto }) {
  const { t, i18n } = useTranslation()
  const percent = cyclePercent(cycle)
  const value = percent != null
    ? formatPercent(Math.round(percent) / 100, i18n.language)
    : cycle.upstream_used != null
      ? `${formatNumber(Number(cycle.upstream_used), i18n.language)}${cycle.upstream_limit != null ? ` / ${formatNumber(Number(cycle.upstream_limit), i18n.language)}` : ""}`
      : "—"
  const reset = cycle.status === "open" ? formatInstant(cycle.period_end, i18n.language) : null
  const closed = cycle.status === "closed"
    ? [t("common.status.closed"), cycle.close_reason ? t(`window.closeReason.${cycle.close_reason}`) : null].filter(Boolean).join(" · ")
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
      <CredentialCycleModels values={cycle.models} />
    </div>
  )
}

type Props = {
  cycles: Array<CredentialQuotaCycleDto>
  loading: boolean
  error: boolean
}

export function CredentialCycleList({ cycles, loading, error }: Props) {
  const { t } = useTranslation()
  const latest = useMemo(() => latestCycles(cycles), [cycles])

  if (loading) return <p className="text-sm text-muted-foreground">{t("common.loading")}</p>
  if (error) return <p className="text-sm text-destructive">{t("common.errors.load")}</p>
  if (!latest.length) return <p className="text-sm text-muted-foreground">{t("providers.credentials.noQuotaCycle")}</p>
  return <div className="grid min-w-0 gap-2">{latest.map((cycle) => <CycleTile key={cycle.id} cycle={cycle} />)}</div>
}
