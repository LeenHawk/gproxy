import { useTranslation } from "react-i18next"
import type { CredentialDto } from "@/generated/CredentialDto"
import type { CredentialQuotaCycleDto } from "@/generated/CredentialQuotaCycleDto"
import type { QuotaWindowDto } from "@/generated/QuotaWindowDto"
import type { ProviderDto } from "@/generated/ProviderDto"
import type { UsageAggregateDto } from "@/generated/UsageAggregateDto"
import { CycleWindow } from "@/components/cycle-window"
import { DataTable, type DataTableColumn } from "@/components/data-table"
import { StatusBadge } from "@/components/status-badge"
import { QuotaWindowBar } from "@/components/usage/quota-window"
import { Button } from "@/components/ui/button"
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card"
import { Empty, EmptyContent, EmptyDescription, EmptyHeader, EmptyTitle } from "@/components/ui/empty"
import { navigateAdminPath } from "@/lib/admin-route"
import { formatCost, formatCount } from "@/lib/format"

function percent(value: string | null) {
  const parsed = Number(value)
  return Number.isFinite(parsed) ? parsed : 0
}

export function OverviewDashboard({ providers, credentials, usage, quotas, cycles }: { providers: Array<ProviderDto>; credentials: Array<CredentialDto>; usage: Array<UsageAggregateDto>; quotas: Array<QuotaWindowDto>; cycles: Array<CredentialQuotaCycleDto> }) {
  const { t, i18n } = useTranslation()
  const enabled = credentials.filter((credential) => credential.enabled)
  const healthy = enabled.filter((credential) => credential.health === "healthy")
  const attention = enabled.filter((credential) => credential.health !== "healthy")
  const requests = usage.reduce((total, row) => total + row.requests, 0)
  const cost = usage.reduce((total, row) => total + Number(row.cost), 0)
  const quotaPressure = quotas.filter((window) => Number(window.cost_limit) > 0 && Number(window.cost_used) / Number(window.cost_limit) >= 0.8)
  const cyclePressure = cycles.filter((cycle) => percent(cycle.used_percent) >= 80).sort((left, right) => percent(right.used_percent) - percent(left.used_percent))
  const credentialColumns: Array<DataTableColumn<CredentialDto>> = [
    { key: "name", label: t("common.name"), header: t("common.name"), cell: (credential) => <span className="font-mono text-xs">{credential.label ?? t("providers.credentials.unnamed", { id: credential.id })}</span> },
    { key: "status", label: t("common.status.label"), header: t("common.status.label"), cell: (credential) => <StatusBadge status={credential.health} /> },
    { key: "detail", label: t("providers.credentials.healthDetail"), header: t("providers.credentials.healthDetail"), cell: (credential) => <span className="text-xs text-muted-foreground">{credential.health_detail ?? t("common.none")}</span> },
  ]
  const usageColumns: Array<DataTableColumn<UsageAggregateDto>> = [
    { key: "provider", label: t("usage.group"), header: t("usage.group"), cell: (row) => <span className="font-mono text-xs">{row.group}</span> },
    { key: "requests", label: t("usage.requests"), header: t("usage.requests"), cell: (row) => <span className="font-mono text-xs">{formatCount(row.requests, i18n.language)}</span> },
    { key: "cost", label: t("usage.cost.label"), header: t("usage.cost.label"), cell: (row) => <span className="font-mono text-xs">{formatCost(row.cost, i18n.language)}</span>, className: "text-right" },
  ]
  const metrics = [
    { key: "health", label: t("overview.metrics.healthy"), value: t("overview.metrics.ratio", { value: healthy.length, total: enabled.length }) },
    { key: "attention", label: t("overview.metrics.attention"), value: String(attention.length + quotaPressure.length + cyclePressure.length) },
    { key: "requests", label: t("overview.metrics.requests"), value: formatCount(requests, i18n.language) },
    { key: "cost", label: t("overview.metrics.cost"), value: formatCost(String(cost), i18n.language) },
  ]
  if (providers.length === 0) {
    return <Empty><EmptyHeader><EmptyTitle>{t("overview.empty.title")}</EmptyTitle><EmptyDescription>{t("overview.empty.description")}</EmptyDescription></EmptyHeader><EmptyContent><Button onClick={() => navigateAdminPath("/admin/providers")}>{t("overview.empty.action")}</Button></EmptyContent></Empty>
  }
  if (credentials.length === 0) {
    const provider = providers[0]
    const name = provider.label ?? provider.name
    return <Empty><EmptyHeader><EmptyTitle>{t("overview.noCredentials.title")}</EmptyTitle><EmptyDescription>{t("overview.noCredentials.description", { provider: name })}</EmptyDescription></EmptyHeader><EmptyContent><Button onClick={() => navigateAdminPath(`/admin/providers/${provider.id}/credentials`)}>{t("overview.noCredentials.action", { provider: name })}</Button></EmptyContent></Empty>
  }
  return (
    <div className="flex flex-col gap-6">
      <section className="grid gap-3 sm:grid-cols-2 xl:grid-cols-4" aria-label={t("overview.metrics.label")}>
        {metrics.map(({ key, label, value }) => <Card key={key} size="sm"><CardContent><p className="text-xs text-muted-foreground">{label}</p><p className="mt-1 font-mono text-xl font-semibold">{value}</p></CardContent></Card>)}
      </section>
      <div className="grid min-w-0 gap-5 xl:grid-cols-[1.05fr_0.95fr]">
        <div className="flex min-w-0 flex-col gap-5">
          <Card><CardHeader><CardTitle>{t("overview.attention.title")}</CardTitle><CardDescription>{t("overview.attention.description")}</CardDescription></CardHeader><CardContent><DataTable columns={credentialColumns} rows={attention} rowKey={(credential) => credential.id} searchText={(credential) => `${credential.label ?? ""} ${credential.health} ${credential.health_detail ?? ""}`} renderCard={(credential) => <div className="flex items-start justify-between gap-3"><div><p className="font-mono text-xs">{credential.label ?? t("providers.credentials.unnamed", { id: credential.id })}</p><p className="text-xs text-muted-foreground">{credential.health_detail ?? t("common.none")}</p></div><StatusBadge status={credential.health} /></div>} empty={t("overview.attention.empty")} storageKey="overview-attention" onRowClick={(credential) => navigateAdminPath(`/admin/providers/${credential.provider_id}/credentials/${credential.id}`)} /></CardContent></Card>
          <Card><CardHeader><CardTitle>{t("overview.spending.title")}</CardTitle><CardDescription>{t("overview.spending.description")}</CardDescription></CardHeader><CardContent><DataTable columns={usageColumns} rows={usage} rowKey={(row) => row.group} searchText={(row) => row.group} renderCard={(row) => <div className="flex items-center justify-between gap-3"><div><p className="font-mono text-xs">{row.group}</p><p className="text-xs text-muted-foreground">{formatCount(row.requests, i18n.language)} {t("usage.requests")}</p></div><p className="font-mono text-sm">{formatCost(row.cost, i18n.language)}</p></div>} empty={t("usage.empty")} storageKey="overview-spending" /></CardContent></Card>
        </div>
        <Card><CardHeader><CardTitle>{t("overview.pressure.title")}</CardTitle><CardDescription>{t("overview.pressure.description")}</CardDescription></CardHeader><CardContent className="flex flex-col gap-5">{quotaPressure.length === 0 && cyclePressure.length === 0 ? <p className="text-sm text-muted-foreground">{t("overview.pressure.empty")}</p> : null}{quotaPressure.slice(0, 4).map((window) => <QuotaWindowBar key={`${window.quota_id}-${window.window_kind}`} window={window} />)}{cyclePressure.slice(0, 4).map((cycle) => <CycleWindow key={cycle.id} cycle={cycle} />)}</CardContent></Card>
      </div>
    </div>
  )
}
