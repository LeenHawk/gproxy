//! Every credential's open upstream windows on one page, filtered by provider.
//!
//! One read (`GET /admin/api/credentials/usage`) carries the credentials and
//! their open cycles, so the page costs the same with five credentials or five
//! hundred. Nothing here asks the upstream: the figures are the last readings
//! this instance recorded. A live probe stays in the credential's own dialog,
//! which a row opens.

import { useMemo, useState } from "react"
import { useQuery } from "@tanstack/react-query"
import { useTranslation } from "react-i18next"
import type { TFunction } from "i18next"
import { credentialDirectory, credentialUsage } from "@/api/credentials"
import type { CredentialCycleDto, CredentialDto, CredentialUsageDto } from "@/generated/sdk"
import type { CredentialProviderDto } from "@/generated/app"
import { Page, PageHeader } from "@/components/page"
import { EmptyNotice, QueryState } from "@/components/state"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { Progress } from "@/components/ui/progress"
import { ToggleGroup, ToggleGroupItem } from "@/components/ui/toggle-group"
import { formatCost, formatDurationMs, formatInstant, formatPercent } from "@/lib/format"
import { CredentialDetails } from "@/pages/credentials/details"
import { quotaWindowName } from "@/pages/credentials/upstream-label"

export function CredentialUsagePage() {
  const { t } = useTranslation()
  const usage = useQuery({ queryKey: ["admin", "/credentials", "usage"], queryFn: credentialUsage, refetchInterval: 60_000 })
  const providers = useQuery({ queryKey: ["credential-providers"], queryFn: credentialDirectory })
  // Empty means every provider: the default, and what clearing the last tab returns to.
  const [selected, setSelected] = useState<string[]>([])
  const [detail, setDetail] = useState<{ row: CredentialDto; provider: CredentialProviderDto } | null>(null)

  const groups = useMemo(() => {
    const byProvider = new Map<string, CredentialUsageDto[]>()
    for (const row of usage.data ?? []) {
      const list = byProvider.get(row.credential.providerId) ?? []
      list.push(row)
      byProvider.set(row.credential.providerId, list)
    }
    return [...byProvider.entries()].map(([id, rows]) => ({ id, rows, provider: providers.data?.find(p => p.id === id) }))
      .sort((a, b) => providerName(a.provider, a.id).localeCompare(providerName(b.provider, b.id)))
  }, [usage.data, providers.data])
  const active = selected.filter(id => groups.some(group => group.id === id))
  const shown = active.length ? groups.filter(group => active.includes(group.id)) : groups

  return <Page>
    <PageHeader title={t("nav.credential-usage")} actions={
      <Button variant="outline" disabled={usage.isFetching} onClick={() => void usage.refetch()}>{t("actions.refresh")}</Button>
    } />
    <p className="text-sm text-muted-foreground">{t("credentialUsage.description")}</p>
    <QueryState isPending={usage.isPending || providers.isPending} error={usage.error ?? providers.error}>
      {groups.length ? <div className="flex flex-wrap items-center gap-2" role="group" aria-label={t("credentialUsage.filter")}>
        <Button size="sm" variant={active.length ? "outline" : "secondary"} aria-pressed={!active.length} onClick={() => setSelected([])}>
          {t("credentialUsage.all")} <span className="text-muted-foreground tabular-nums">{usage.data?.length ?? 0}</span>
        </Button>
        <ToggleGroup type="multiple" variant="outline" size="sm" spacing={2} className="flex-wrap" value={active} onValueChange={setSelected}>
          {groups.map(group => <ToggleGroupItem key={group.id} value={group.id} className="data-[state=on]:bg-secondary">
            {providerName(group.provider, group.id)} <span className="text-muted-foreground tabular-nums">{group.rows.length}</span>
          </ToggleGroupItem>)}
        </ToggleGroup>
      </div> : <EmptyNotice title={t("credentialUsage.empty")} />}
      {shown.map(group => <section key={group.id} className="flex flex-col gap-2" aria-labelledby={`credential-usage-${group.id}`}>
        <h2 id={`credential-usage-${group.id}`} className="text-sm font-medium">
          {providerName(group.provider, group.id)}
          {group.provider ? <span className="ml-2 font-normal text-muted-foreground">{group.provider.channel}</span> : null}
        </h2>
        <ul className="flex flex-col divide-y rounded-lg border">
          {group.rows.map(row => <CredentialRow key={row.credential.id} row={row} provider={group.provider}
            onOpen={group.provider ? () => setDetail({ row: row.credential, provider: group.provider! }) : undefined} />)}
        </ul>
      </section>)}
    </QueryState>
    {detail ? <CredentialDetails credential={detail.row} provider={detail.provider} initialTab="upstream" onClose={() => { setDetail(null); void usage.refetch() }} /> : null}
  </Page>
}

function providerName(provider: CredentialProviderDto | undefined, id: string) {
  return provider?.displayName || provider?.name || id
}

function CredentialRow({ row, provider, onOpen }: { row: CredentialUsageDto; provider?: CredentialProviderDto; onOpen?: () => void }) {
  const { t, i18n } = useTranslation()
  const { credential, cycles } = row
  const label = credential.label ?? credential.id
  const moment = new Intl.DateTimeFormat(i18n.language, { month: "2-digit", day: "2-digit", hour: "2-digit", minute: "2-digit", hour12: false })
  return <li className="grid gap-2 px-3 py-2.5 md:grid-cols-[minmax(10rem,16rem)_minmax(0,1fr)] md:items-start">
    <div className="flex min-w-0 flex-wrap items-center gap-2">
      {onOpen ? <Button variant="link" className="h-auto min-w-0 truncate p-0 font-medium" title={label} onClick={onOpen}>{label}</Button>
        : <span className="min-w-0 truncate font-medium" title={label}>{label}</span>}
      {credential.status !== "active" ? <Badge variant={credential.status === "dead" ? "destructive" : "warning"}>{t(`values.${credential.status}`, { defaultValue: credential.status })}</Badge> : null}
      {!credential.enabled ? <Badge variant="outline">{t("management.disabled")}</Badge> : null}
    </div>
    {cycles.length ? <ul className="flex flex-col gap-1.5">
      {cycles.map(cycle => {
        const percent = cycle.sample?.usedPercent == null ? null : Number(cycle.sample.usedPercent)
        const tone = percent == null ? undefined : percent >= 100 ? "destructive" : percent >= 80 ? "warning" : "success"
        const name = windowTitle(cycle, provider?.channel, t, i18n.language)
        return <li key={cycle.id} className="grid grid-cols-[minmax(0,1fr)_auto] items-center gap-x-3 gap-y-1 text-sm sm:grid-cols-[minmax(8rem,12rem)_minmax(4rem,1fr)_4rem_minmax(5rem,auto)_minmax(6rem,auto)]">
          <span className="min-w-0 truncate text-muted-foreground" title={name}>{name}</span>
          {percent != null ? <Progress className="col-span-2 row-start-2 sm:col-span-1 sm:row-start-auto" tone={tone} value={Math.min(100, Math.max(0, percent))} aria-label={`${label} · ${name}`} /> : <span className="hidden sm:block" />}
          <span className="justify-self-end tabular-nums">{percent != null ? formatPercent(percent / 100, i18n.language) : "—"}</span>
          <span className="justify-self-end whitespace-nowrap text-xs tabular-nums text-muted-foreground" title={t("limits.cycleSpent")}>{formatCost(cycle.costUsd, i18n.language)}</span>
          {cycle.endsAtMs != null ? <time className="justify-self-end whitespace-nowrap text-xs tabular-nums text-muted-foreground" dateTime={new Date(cycle.endsAtMs).toISOString()} title={`${t("limits.resetsAt")}: ${formatInstant(cycle.endsAtMs, i18n.language)}`}>{moment.format(cycle.endsAtMs)}</time> : <span className="hidden sm:block" />}
        </li>
      })}
    </ul> : <p className="text-sm text-muted-foreground">{t("limits.noObservation")}</p>}
  </li>
}

/** The same names the credential dialog gives its windows; see `upstream.tsx`. */
function windowTitle(cycle: CredentialCycleDto, channel: string | undefined, t: TFunction, language: string) {
  const id = cycle.windowId
  if (id === "codex_credits") return t("limits.codexCredits")
  if (id === "month") return t("limits.monthCycle")
  if (channel === "codex" && /_(primary|secondary)$/.test(id)) {
    const period = cycle.endsAtMs != null ? cycle.endsAtMs - cycle.startsAtMs : 0
    return period > 0 ? t("limits.periodQuota", { period: formatDurationMs(period, language) })
      : t(id.endsWith("_primary") ? "limits.primaryQuota" : "limits.secondaryQuota")
  }
  return quotaWindowName(id, t)
}
