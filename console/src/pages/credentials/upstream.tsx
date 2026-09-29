import { Badge } from "@/components/ui/badge"
import { Alert, AlertDescription } from "@/components/ui/alert"
import type { QuotaBreakdownRowDto } from "@/generated/sdk"
import { useQuery } from "@tanstack/react-query"
import { useTranslation } from "react-i18next"
import type { CredentialProviderDto } from "@/generated/app"
import { credentialQuota, probeQuota } from "@/api/credentials"
import { formatInstant, formatNumber, formatPercent, formatDurationMs } from "@/lib/format"
import { EmptyNotice, ErrorNotice, QueryState } from "@/components/state"
import { Button } from "@/components/ui/button"
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card"
import { Progress } from "@/components/ui/progress"

import { UpstreamBreakdown } from "./upstream-breakdown"
import { UpstreamCycles } from "./upstream-cycles"
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
  const cycles = saved.data?.cycles ?? []
  const openCycles = cycles.filter(cycle => cycle.closedAtMs == null)
  const reported = probe.data ? probe.data.entries.map(entry => ({
    id: entry.id, label: entry.label, kind: entry.kind, breakdown: entry.breakdown,
    startsAtMs: entry.allowance?.periodStartMs,
    used: entry.allowance?.used, limit: entry.allowance?.limit, remaining: entry.allowance?.remaining ?? entry.balance?.remaining,
    unlimited: entry.allowance?.unlimited, unit: entry.allowance?.unit ?? entry.balance?.unit,
    usedPercent: entry.allowance?.usedPercent, resetsAtMs: entry.allowance?.periodEndMs,
  })) : []
  // Without a live answer, each open cycle's last reading stands in for its
  // window; with one, a cycle the upstream does not report (a credential's
  // calendar-month cycle, say) still gets its own row.
  const entries = [...reported, ...openCycles.filter(cycle => !reported.some(entry => entry.id === cycle.windowId)).map(cycle => ({
    id: cycle.windowId, label: null, kind: "window", breakdown: null as QuotaBreakdownRowDto[] | null,
    startsAtMs: cycle.startsAtMs, used: cycle.sample?.used, limit: cycle.sample?.limit, remaining: undefined,
    unlimited: false, unit: undefined, usedPercent: cycle.sample?.usedPercent, resetsAtMs: cycle.endsAtMs,
  }))]
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
    if (entry.id === "month") return t("limits.monthCycle")
    return quotaWindowName(entry.id, t, entry.label) !== entry.id ? quotaWindowName(entry.id, t, entry.label) : entry.label ?? entry.id
  }
  const amount = (value: string | null | undefined, unit: string | null | undefined) => value == null ? "—"
    : unit === "percent" ? formatPercent(Number(value) / 100, i18n.language)
      : `${formatNumber(value, i18n.language)}${unit ? ` ${unit === "credits" ? t("limits.creditUnit") : unit}` : ""}`
  const estimated = openCycles.some(cycle => cycle.estimatedAllowanceUsd != null)
  const resetTime = new Intl.DateTimeFormat(i18n.language, { month: "2-digit", day: "2-digit", hour: "2-digit", minute: "2-digit", hour12: false })
  return <div className="flex flex-col gap-4">
    <div className="flex flex-wrap justify-end gap-2">
      {provider.capabilities.quotaQuery ? <Button variant="outline" disabled={busy || !provider.enabled} onClick={() => void probe.refetch()}>{t("management.probe")}</Button> : null}
    </div>
    {provider.capabilities.quotaReset ? <UpstreamReset id={id} enabled={provider.enabled} busy={busy} onReset={() => probe.refetch()} /> : null}
    {probe.error ? <ErrorNotice error={probe.error} /> : null}
    <QueryState isPending={saved.isPending || (probe.isFetching && !entries.length)} error={saved.error}>
      {!windows.length ? <EmptyNotice title={t("limits.noObservation")} /> : null}
      {windows.map(entry => { const open = openCycles.find(cycle => cycle.windowId === entry.id); const closed = cycles.filter(cycle => cycle.closedAtMs != null && cycle.windowId === entry.id); return <Card key={entry.id} size="sm" className="gap-1 py-2"><CardHeader className="grid-cols-[minmax(0,1fr)_auto] sm:grid-cols-[minmax(0,1fr)_minmax(3rem,1fr)_auto_minmax(7rem,auto)] items-center gap-2 px-3">
        <CardTitle className="min-w-0 truncate" title={title(entry)}>{title(entry)}</CardTitle>
        {entry.usedPercent != null ? <Progress className="col-span-2 row-start-2 sm:col-span-1 sm:row-start-auto" tone={Number(entry.usedPercent) >= 100 ? "destructive" : Number(entry.usedPercent) >= 80 ? "warning" : "success"} value={Math.min(100, Math.max(0, Number(entry.usedPercent)))} aria-label={title(entry)} /> : <span className="hidden sm:block" />}
        <Badge variant={entry.usedPercent != null ? Number(entry.usedPercent) >= 100 ? "destructive" : Number(entry.usedPercent) >= 80 ? "warning" : "success" : entry.remaining != null && Number(entry.remaining) <= 0 ? "destructive" : "secondary"} className="justify-self-end whitespace-nowrap tabular-nums">{entry.usedPercent != null ? formatPercent(Number(entry.usedPercent) / 100, i18n.language)
          : entry.kind === "balance" ? amount(entry.remaining, entry.unit)
            : entry.used == null && entry.limit == null ? entry.unlimited ? t("limits.unlimited") : "—"
              : `${amount(entry.used, entry.unit)} / ${entry.unlimited ? t("limits.unlimited") : amount(entry.limit, entry.unit)}`}</Badge>
        {entry.resetsAtMs != null ? <time className="col-span-2 justify-self-end whitespace-nowrap text-xs tabular-nums text-muted-foreground sm:col-span-1" dateTime={new Date(entry.resetsAtMs).toISOString()} title={`${t("limits.resetsAt")}: ${formatInstant(entry.resetsAtMs, i18n.language)}`}>{resetTime.format(entry.resetsAtMs)}</time> : <span className="hidden sm:block" />}
      </CardHeader>
      {open || closed.length ? <CardContent className="px-3"><UpstreamCycles open={open} closed={closed} /></CardContent> : null}
      </Card> })}
      {estimated ? <p className="text-xs text-muted-foreground">{t("limits.estimateHint")}</p> : null}
      {(saved.data?.blocks ?? []).map(block => <Alert key={block.id} variant="destructive"><AlertDescription><p>{t("limits.blockedUntil", { at: formatInstant(block.untilMs, i18n.language) })}</p>{block.operation ? <p>{t(`operation.${block.operation}`, { defaultValue: block.operation })}</p> : null}</AlertDescription></Alert>)}
      {breakdown?.length ? <UpstreamBreakdown rows={breakdown} /> : null}
    </QueryState>
  </div>
}
