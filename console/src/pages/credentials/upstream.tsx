import { useQuery } from "@tanstack/react-query"
import { useTranslation } from "react-i18next"
import type { CredentialProviderDto } from "@/generated/app"
import { credentialQuota, probeQuota } from "@/api/credentials"
import { object } from "@/components/providers/provider-model-state"
import { formatInstant, formatNumber, formatPercent, formatDurationMs } from "@/lib/format"
import { EmptyNotice, ErrorNotice, QueryState } from "@/components/state"
import { Button } from "@/components/ui/button"
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card"
import { Progress } from "@/components/ui/progress"

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
    id: entry.id, label: entry.label, kind: entry.kind,
    startsAtMs: entry.allowance?.periodStartMs,
    used: entry.allowance?.used, limit: entry.allowance?.limit, remaining: entry.allowance?.remaining ?? entry.balance?.remaining,
    unlimited: entry.allowance?.unlimited, unit: entry.allowance?.unit ?? entry.balance?.unit,
    usedPercent: entry.allowance?.usedPercent, observedAtMs: probe.data!.observedAtMs, resetsAtMs: entry.allowance?.periodEndMs,
  })) : (saved.data?.cycles ?? []).map(cycle => {
    const value = object(cycle.snapshot)
    const text = (key: string) => typeof value[key] === "string" || typeof value[key] === "number" ? String(value[key]) : undefined
    return { id: text("id") ?? cycle.id, label: text("label"), startsAtMs: cycle.startsAtMs, kind: text("kind"), used: text("used"), limit: text("limit"), remaining: text("remaining"), unlimited: value.unlimited === true, unit: text("unit"), usedPercent: text("used_percent"), observedAtMs: cycle.observedAtMs, resetsAtMs: cycle.resetsAtMs }
  })
  const latest = entries.filter((entry, index) => entries.findIndex(other => other.id === entry.id) === index)
  const title = (entry: typeof entries[number]) => {
    if (entry.id === "codex_credits") return t("limits.codexCredits")
    if (provider.channel === "codex" && /_(primary|secondary)$/.test(entry.id)) {
      const period = entry.startsAtMs != null && entry.resetsAtMs != null ? entry.resetsAtMs - entry.startsAtMs : 0
      const window = period > 0 ? t("limits.periodQuota", { period: formatDurationMs(period, i18n.language) })
        : t(entry.id.endsWith("_primary") ? "limits.primaryQuota" : "limits.secondaryQuota")
      return entry.label ? `${entry.label} · ${window}` : window
    }
    return quotaWindowName(entry.id, t) !== entry.id ? quotaWindowName(entry.id, t) : entry.label ?? entry.id
  }
  const amount = (value: string | null | undefined, unit: string | null | undefined) => value == null ? "—"
    : unit === "percent" ? formatPercent(Number(value) / 100, i18n.language)
      : `${formatNumber(value, i18n.language)}${unit ? ` ${unit === "credits" ? t("limits.creditUnit") : unit}` : ""}`
  return <div className="flex flex-col gap-4">
    <div className="flex flex-wrap gap-2">
      {provider.capabilities.quotaQuery ? <Button variant="outline" disabled={busy || !provider.enabled} onClick={() => void probe.refetch()}>{t("management.probe")}</Button> : null}
    </div>
    {provider.capabilities.quotaReset ? <UpstreamReset id={id} enabled={provider.enabled} busy={busy} onReset={() => probe.refetch()} /> : null}
    {probe.error ? <ErrorNotice error={probe.error} /> : null}
    <QueryState isPending={saved.isPending || (probe.isFetching && !entries.length)} error={saved.error}>
      {!entries.length ? <EmptyNotice title={t("limits.noObservation")} /> : null}
      {latest.map(entry => <Card key={entry.id} className="gap-3 py-4"><CardHeader className="px-4"><CardTitle className="flex flex-wrap items-center justify-between gap-2"><span className="break-words">{title(entry)}</span>{entry.usedPercent != null ? <span className="tabular-nums">{formatPercent(Number(entry.usedPercent) / 100, i18n.language)}</span> : null}</CardTitle></CardHeader><CardContent className="flex flex-col gap-3 px-4">
        {entry.usedPercent != null ? <Progress value={Math.min(100, Math.max(0, Number(entry.usedPercent)))} aria-label={title(entry)} /> : null}
        <dl className="grid grid-cols-[auto_minmax(0,1fr)] gap-x-4 gap-y-2 text-sm">
          {entry.kind !== "balance" && entry.unit !== "percent" ? <><dt className="text-muted-foreground">{t("limits.used")}</dt><dd>{amount(entry.used, entry.unit)} / {entry.unlimited ? t("limits.unlimited") : amount(entry.limit, entry.unit)}</dd></> : null}
          <dt className="text-muted-foreground">{t("limits.remaining")}</dt><dd>{amount(entry.remaining, entry.unit)}</dd>
          {entry.resetsAtMs != null ? <><dt className="text-muted-foreground">{t("limits.resetsAt")}</dt><dd>{formatInstant(entry.resetsAtMs, i18n.language)}</dd></> : null}
          <dt className="text-muted-foreground">{t("limits.observedAt")}</dt><dd>{entry.observedAtMs > 0 ? formatInstant(entry.observedAtMs, i18n.language) : "—"}</dd>
        </dl>
      </CardContent></Card>)}
      {(saved.data?.blocks ?? []).map(block => <div key={block.id} className="rounded-lg border p-3 text-sm"><p>{t("limits.blockedUntil", { at: formatInstant(block.untilMs, i18n.language) })}</p>{block.operation ? <p>{t(`operation.${block.operation}`, { defaultValue: block.operation })}</p> : null}</div>)}
    </QueryState>
  </div>
}
