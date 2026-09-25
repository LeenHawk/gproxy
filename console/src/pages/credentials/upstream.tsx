import type { QuotaBreakdownRowDto } from "@/generated/sdk"
import { useQuery } from "@tanstack/react-query"
import { useTranslation } from "react-i18next"
import type { CredentialProviderDto } from "@/generated/app"
import { credentialQuota, probeQuota } from "@/api/credentials"
import { object } from "@/components/providers/provider-model-state"
import { formatInstant, formatNumber, formatPercent, formatDurationMs } from "@/lib/format"
import { EmptyNotice, ErrorNotice, QueryState } from "@/components/state"
import { Button } from "@/components/ui/button"
import { Card, CardHeader, CardTitle } from "@/components/ui/card"
import { Progress } from "@/components/ui/progress"

import { UpstreamBreakdown } from "./upstream-breakdown"
import { UpstreamReset } from "./upstream-reset"
import { quotaWindowName } from "./upstream-label"

export function UpstreamQuota({ id, provider }: { id: string; provider: CredentialProviderDto }) {
  const { t, i18n } = useTranslation()
  const saved = useQuery({ queryKey: ["credential-quota", id], queryFn: () => credentialQuota(id) })
  const probe = useQuery({
    queryKey: ["credential-quota-probe", id],
    queryFn: async () => { const result = await probeQuota(id); await saved.refetch(); return result },
    enabled: provider.capabilities.quotaQuery && provider.enabled,
    retry: false,
    staleTime: Infinity,
  })
  const busy = probe.isFetching
  const entries = probe.data ? probe.data.entries.map(entry => ({
    id: entry.id, label: entry.label, kind: entry.kind, breakdown: entry.breakdown,
    startsAtMs: entry.allowance?.periodStartMs,
    used: entry.allowance?.used, limit: entry.allowance?.limit, remaining: entry.allowance?.remaining ?? entry.balance?.remaining,
    unlimited: entry.allowance?.unlimited, unit: entry.allowance?.unit ?? entry.balance?.unit,
    usedPercent: entry.allowance?.usedPercent, resetsAtMs: entry.allowance?.periodEndMs,
  })) : (saved.data?.cycles ?? []).map(cycle => {
    const value = object(cycle.snapshot)
    const text = (key: string) => typeof value[key] === "string" || typeof value[key] === "number" ? String(value[key]) : undefined
    const breakdown: QuotaBreakdownRowDto[] | null = Array.isArray(value.breakdown) ? value.breakdown.map(row => {
      const item = object(row)
      return { key: String(item.key), label: typeof item.label === "string" ? item.label : null, percent: String(item.percent) }
    }) : null
    return { breakdown, id: text("id") ?? cycle.id, label: text("label"), startsAtMs: cycle.startsAtMs, kind: text("kind"), used: text("used"), limit: text("limit"), remaining: text("remaining"), unlimited: value.unlimited === true, unit: text("unit"), usedPercent: text("used_percent"), resetsAtMs: cycle.resetsAtMs }
  })
  const latest = entries.filter((entry, index) => entries.findIndex(other => other.id === entry.id) === index)
  const windows = latest.filter(entry => entry.kind !== "breakdown" && entry.id !== "seven_day_breakdown")
  const breakdown = latest.find(entry => entry.id === "seven_day_breakdown")?.breakdown
  const title = (entry: typeof entries[number]) => {
    if (entry.id === "codex_credits") return t("limits.codexCredits")
    if (provider.channel === "codex" && /_(primary|secondary)$/.test(entry.id)) {
      const period = entry.startsAtMs != null && entry.resetsAtMs != null ? entry.resetsAtMs - entry.startsAtMs : 0
      const window = period > 0 ? t("limits.periodQuota", { period: formatDurationMs(period, i18n.language) })
        : t(entry.id.endsWith("_primary") ? "limits.primaryQuota" : "limits.secondaryQuota")
      return entry.label ? `${entry.label} · ${window}` : window
    }
    return quotaWindowName(entry.id, t, entry.label) !== entry.id ? quotaWindowName(entry.id, t, entry.label) : entry.label ?? entry.id
  }
  const amount = (value: string | null | undefined, unit: string | null | undefined) => value == null ? "—"
    : unit === "percent" ? formatPercent(Number(value) / 100, i18n.language)
      : `${formatNumber(value, i18n.language)}${unit ? ` ${unit === "credits" ? t("limits.creditUnit") : unit}` : ""}`
  const resetTime = new Intl.DateTimeFormat(i18n.language, { month: "2-digit", day: "2-digit", hour: "2-digit", minute: "2-digit", hour12: false })
  return <div className="flex flex-col gap-4">
    <div className="flex flex-wrap gap-2">
      {provider.capabilities.quotaQuery ? <Button variant="outline" disabled={busy || !provider.enabled} onClick={() => void probe.refetch()}>{t("management.probe")}</Button> : null}
    </div>
    {provider.capabilities.quotaReset ? <UpstreamReset id={id} enabled={provider.enabled} busy={busy} onReset={() => probe.refetch()} /> : null}
    {probe.error ? <ErrorNotice error={probe.error} /> : null}
    <QueryState isPending={saved.isPending || (probe.isFetching && !entries.length)} error={saved.error}>
      {!windows.length ? <EmptyNotice title={t("limits.noObservation")} /> : null}
      {windows.map(entry => <Card key={entry.id} size="sm" className="py-2"><CardHeader className="grid-cols-[minmax(0,auto)_minmax(2rem,1fr)_auto_auto] items-center gap-2 px-3">
        <CardTitle className="min-w-0 max-w-32 truncate" title={title(entry)}>{title(entry)}</CardTitle>
        {entry.usedPercent != null ? <Progress value={Math.min(100, Math.max(0, Number(entry.usedPercent)))} aria-label={title(entry)} /> : <span />}
        <span className="whitespace-nowrap tabular-nums">{entry.usedPercent != null ? formatPercent(Number(entry.usedPercent) / 100, i18n.language)
          : entry.kind === "balance" ? amount(entry.remaining, entry.unit)
            : `${amount(entry.used, entry.unit)} / ${entry.unlimited ? t("limits.unlimited") : amount(entry.limit, entry.unit)}`}</span>
        {entry.resetsAtMs != null ? <time className="whitespace-nowrap text-xs tabular-nums text-muted-foreground" dateTime={new Date(entry.resetsAtMs).toISOString()} title={`${t("limits.resetsAt")}: ${formatInstant(entry.resetsAtMs, i18n.language)}`}>{resetTime.format(entry.resetsAtMs)}</time> : null}
      </CardHeader></Card>)}
      {(saved.data?.blocks ?? []).map(block => <div key={block.id} className="rounded-lg border p-3 text-sm"><p>{t("limits.blockedUntil", { at: formatInstant(block.untilMs, i18n.language) })}</p>{block.operation ? <p>{t(`operation.${block.operation}`, { defaultValue: block.operation })}</p> : null}</div>)}
      {breakdown?.length ? <UpstreamBreakdown rows={breakdown} /> : null}
    </QueryState>
  </div>
}
