import { useState } from "react"
import { useMutation, useQuery } from "@tanstack/react-query"
import { useTranslation } from "react-i18next"
import type { CredentialProviderDto } from "@/generated/app"
import { credentialQuota, probeQuota, resetUpstreamQuota } from "@/api/credentials"
import { object } from "@/components/providers/provider-model-state"
import { formatInstant } from "@/lib/format"
import { EmptyNotice, ErrorNotice, QueryState } from "@/components/state"
import { Button } from "@/components/ui/button"
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card"
import { Badge } from "@/components/ui/badge"

export function UpstreamQuota({ id, provider }: { id: string; provider: CredentialProviderDto }) {
  const { t, i18n } = useTranslation()
  const saved = useQuery({ queryKey: ["credential-quota", id], queryFn: () => credentialQuota(id) })
  const probe = useMutation({ mutationFn: () => probeQuota(id), onSuccess: () => saved.refetch() })
  const [confirm, setConfirm] = useState(false)
  const reset = useMutation({ mutationFn: () => resetUpstreamQuota(id), onSuccess: async () => { setConfirm(false); probe.reset(); await saved.refetch() } })
  const busy = probe.isPending || reset.isPending
  const entries = probe.data ? probe.data.entries.map(entry => ({
    id: entry.id, label: entry.label ?? entry.id, kind: entry.kind,
    used: entry.allowance?.used, limit: entry.allowance?.limit, remaining: entry.allowance?.remaining ?? entry.balance?.remaining,
    unlimited: entry.allowance?.unlimited, unit: entry.allowance?.unit ?? entry.balance?.unit,
    usedPercent: entry.allowance?.usedPercent, observedAtMs: probe.data!.observedAtMs, resetsAtMs: entry.allowance?.periodEndMs,
  })) : (saved.data?.cycles ?? []).map(cycle => {
    const value = object(cycle.snapshot)
    const text = (key: string) => typeof value[key] === "string" || typeof value[key] === "number" ? String(value[key]) : undefined
    return { id: cycle.id, label: text("label") ?? text("id") ?? t("limits.upstream"), kind: text("kind"), used: text("used"), limit: text("limit"), remaining: text("remaining"), unlimited: value.unlimited === true, unit: text("unit"), usedPercent: text("used_percent"), observedAtMs: cycle.observedAtMs, resetsAtMs: cycle.resetsAtMs }
  })
  return <div className="flex flex-col gap-4">
    <p className="text-sm text-muted-foreground">{t("limits.upstreamHelp")}</p>
    <div className="flex flex-wrap gap-2">
      {provider.capabilities.quotaQuery ? <Button variant="outline" disabled={busy || !provider.enabled} onClick={() => probe.mutate()}>{t("management.probe")}</Button> : null}
      {provider.capabilities.quotaReset ? <Button variant="ghost" disabled={busy || !provider.enabled} onClick={() => setConfirm(true)}>{t("management.upstreamReset")}</Button> : null}
    </div>
    {confirm ? <div role="alert" className="flex flex-col gap-3 rounded-lg border p-3"><p>{t("management.upstreamResetConfirm")}</p><div className="flex gap-2"><Button variant="outline" disabled={busy} onClick={() => setConfirm(false)}>{t("actions.cancel")}</Button><Button variant="destructive" disabled={busy} onClick={() => reset.mutate()}>{t("management.upstreamReset")}</Button></div></div> : null}
    {reset.data ? <p className="text-sm" role="status">{t(`limits.resetOutcome.${reset.data.outcome}`)}</p> : null}
    {probe.error || reset.error ? <ErrorNotice error={probe.error ?? reset.error} /> : null}
    <QueryState isPending={saved.isPending} error={saved.error}>
      {!entries.length ? <EmptyNotice title={t("limits.noObservation")} /> : null}
      {entries.map(entry => <Card key={entry.id} className="gap-3 py-4"><CardHeader className="px-4"><CardTitle className="flex flex-wrap items-center gap-2"><span className="break-all">{entry.label}</span>{entry.usedPercent != null ? <Badge variant="outline">{entry.usedPercent}%</Badge> : null}</CardTitle></CardHeader><CardContent className="px-4"><dl className="grid grid-cols-[auto_1fr] gap-x-4 gap-y-2 text-sm">
        {entry.kind !== "balance" ? <><dt className="text-muted-foreground">{t("limits.used")}</dt><dd>{entry.used ?? "—"} / {entry.unlimited ? t("limits.unlimited") : entry.limit ?? "—"} {entry.unit}</dd></> : null}
        <dt className="text-muted-foreground">{t("limits.remaining")}</dt><dd>{entry.remaining ?? "—"} {entry.unit}</dd>
        <dt className="text-muted-foreground">{t("limits.resetsAt")}</dt><dd>{formatInstant(entry.resetsAtMs, i18n.language) ?? "—"}</dd>
        <dt className="text-muted-foreground">{t("limits.observedAt")}</dt><dd>{formatInstant(entry.observedAtMs, i18n.language)}</dd>
      </dl></CardContent></Card>)}
      {(saved.data?.blocks ?? []).map(block => <div key={block.id} className="rounded-lg border p-3 text-sm"><p>{t("limits.blockedUntil", { at: formatInstant(block.untilMs, i18n.language) })}</p>{block.operation ? <p>{t(`operation.${block.operation}`, { defaultValue: block.operation })}</p> : null}</div>)}
    </QueryState>
  </div>
}
