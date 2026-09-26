//! What this deployment spent in one upstream window: the open cycle's USD,
//! the allowance that spending implies, and the window's recent closed cycles.
//!
//! Every figure here belongs to one window. Windows overlap — a request
//! counts toward the 5-hour and the weekly window at once — so nothing in
//! this file ever adds two windows together.

import { ChevronDown } from "lucide-react"
import { useTranslation } from "react-i18next"
import type { CredentialCycleDto } from "@/generated/sdk"
import { formatCost, formatPercent } from "@/lib/format"
import { Button } from "@/components/ui/button"
import { Collapsible, CollapsibleContent, CollapsibleTrigger } from "@/components/ui/collapsible"

export function UpstreamCycles({ open, closed }: { open?: CredentialCycleDto; closed: CredentialCycleDto[] }) {
  const { t, i18n } = useTranslation()
  if (!open && !closed.length) return null
  const moment = new Intl.DateTimeFormat(i18n.language, { month: "2-digit", day: "2-digit", hour: "2-digit", minute: "2-digit", hour12: false })
  const percent = (value: string | null | undefined) => value == null ? "—" : formatPercent(Number(value) / 100, i18n.language)
  return <div className="flex flex-col gap-1 text-xs text-muted-foreground">
    {open ? <dl className="flex flex-wrap gap-x-4 gap-y-1 tabular-nums">
      <div className="flex gap-1"><dt>{t("limits.cycleSpent")}</dt><dd className="text-foreground">{formatCost(open.costUsd, i18n.language)}</dd></div>
      <div className="flex gap-1" title={t("limits.estimateHint")}>
        <dt>{t("limits.estimatedAllowance")}</dt>
        <dd className="text-foreground">{open.estimatedAllowanceUsd != null ? `≈ ${formatCost(open.estimatedAllowanceUsd, i18n.language)}` : "—"}</dd>
      </div>
    </dl> : null}
    {closed.length ? <Collapsible>
      <CollapsibleTrigger asChild><Button type="button" variant="ghost" size="xs" className="group -ml-2 w-fit text-muted-foreground">
        {t("limits.recentCycles", { count: closed.length })}
        <ChevronDown className="transition-transform group-data-[state=open]:rotate-180" aria-hidden="true" />
      </Button></CollapsibleTrigger>
      <CollapsibleContent>
        <table className="w-full tabular-nums">
          <thead><tr className="text-left"><th className="font-normal">{t("limits.cyclePeriod")}</th><th className="text-right font-normal">{t("limits.cycleCost")}</th><th className="text-right font-normal">{t("limits.cycleFinalPercent")}</th></tr></thead>
          <tbody className="text-foreground">{closed.map(cycle => {
            const end = cycle.closedAtMs ?? cycle.endsAtMs
            return <tr key={cycle.id}>
              <td><time dateTime={new Date(cycle.startsAtMs).toISOString()}>{moment.format(cycle.startsAtMs)}</time>{end != null ? <> – <time dateTime={new Date(end).toISOString()}>{moment.format(end)}</time></> : null}</td>
              <td className="text-right">{formatCost(cycle.costUsd, i18n.language)}</td>
              <td className="text-right">{percent(cycle.sample?.usedPercent)}</td>
            </tr>
          })}</tbody>
        </table>
      </CollapsibleContent>
    </Collapsible> : null}
  </div>
}
