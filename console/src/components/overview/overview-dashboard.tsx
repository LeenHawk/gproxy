import { useTranslation } from "react-i18next"
import type { CredentialDto } from "@/generated/CredentialDto"
import type { CredentialQuotaCycleDto } from "@/generated/CredentialQuotaCycleDto"
import type { QuotaWindowDto } from "@/generated/QuotaWindowDto"
import type { UsageAggregateDto } from "@/generated/UsageAggregateDto"
import { StatusBadge } from "@/components/status-badge"
import { CycleWindow } from "@/components/cycle-window"
import { QuotaWindowBar } from "@/components/usage/quota-window"
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from "@/components/ui/table"
import { formatCost } from "@/lib/format"

export function OverviewDashboard({ credentials, usage, quotas, cycles }: { credentials: Array<CredentialDto>; usage: Array<UsageAggregateDto>; quotas: Array<QuotaWindowDto>; cycles: Array<CredentialQuotaCycleDto> }) {
  const { t, i18n } = useTranslation()
  return (
    <div className="grid gap-8 xl:grid-cols-[1.1fr_0.9fr]">
      <section className="flex min-w-0 flex-col gap-3">
        <h2 className="text-sm font-semibold">{t("providers.credentials.title")}</h2>
        <div className="overflow-hidden rounded-md border bg-card">
          <Table>
            <TableHeader><TableRow><TableHead>{t("common.name")}</TableHead><TableHead>{t("common.status.label")}</TableHead></TableRow></TableHeader>
            <TableBody>
              {credentials.slice(0, 8).map((credential) => (
                <TableRow key={credential.id}>
                  <TableCell className="font-mono text-xs">{credential.label ?? `#${credential.id}`}</TableCell>
                  <TableCell><StatusBadge status={credential.health} /></TableCell>
                </TableRow>
              ))}
            </TableBody>
          </Table>
        </div>
        <h2 className="mt-3 text-sm font-semibold">{t("usage.cost.title")}</h2>
        <div className="overflow-hidden rounded-md border bg-card">
          <Table>
            <TableHeader><TableRow><TableHead>{t("usage.group")}</TableHead><TableHead className="text-right">{t("usage.cost.label")}</TableHead></TableRow></TableHeader>
            <TableBody>
              {usage.slice(0, 8).map((item) => (
                <TableRow key={item.group}><TableCell className="font-mono text-xs">{item.group}</TableCell><TableCell className="text-right font-mono text-xs">{formatCost(item.cost, i18n.language)}</TableCell></TableRow>
              ))}
            </TableBody>
          </Table>
        </div>
      </section>
      <section className="flex flex-col gap-6">
        <div className="flex flex-col gap-4">
          <h2 className="text-sm font-semibold">{t("usage.quotaWindows")}</h2>
          {quotas.slice(0, 5).map((window) => (
            <QuotaWindowBar key={`${window.quota_id}-${window.window_kind}`} window={window} />
          ))}
        </div>
        <div className="flex flex-col gap-4">
          <h2 className="text-sm font-semibold">{t("usage.credentialCycles")}</h2>
          {cycles.slice(0, 5).map((cycle) => (
            <CycleWindow key={cycle.id} cycle={cycle} />
          ))}
        </div>
      </section>
    </div>
  )
}
