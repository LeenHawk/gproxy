//! The caller's budget windows, as meters.
//!
//! `PortalQuotaWindowDto` reports the owner of every window because the chain
//! is `[key, user, team, organization]` and "I am out of my own
//! allowance" and "my team is out of its allowance" are two different things
//! to do something about. The owner is therefore the first thing each row
//! says, not a footnote.

import { useTranslation } from "react-i18next"
import { EmptyNotice } from "@/components/state"
import { Badge } from "@/components/ui/badge"
import { Progress } from "@/components/ui/progress"
import type { PortalQuotaWindowDto } from "@/generated/app"
import { formatCost, formatInstant, formatNumber } from "@/lib/format"

function tone(percent: number | null) {
  if (percent === null) return "outline" as const
  if (percent >= 100) return "destructive" as const
  if (percent >= 80) return "warning" as const
  return "success" as const
}

export function QuotaWindows({ windows }: { windows: Array<PortalQuotaWindowDto> }) {
  const { t, i18n } = useTranslation()
  if (windows.length === 0) {
    return <EmptyNotice title={t("quota.emptyTitle")} />
  }
  const amount = (value: string, unit: string) =>
    unit === "USD" ? formatCost(value, i18n.language) : formatNumber(value, i18n.language)

  return (
    <ul className="grid gap-3 sm:grid-cols-2">
      {windows.map((window) => {
        const percent = window.usedPercent === null ? null : Number(window.usedPercent)
        const key = `${window.ownerKind}:${window.ownerId}:${window.windowKey}:${window.period}`
        return (
          <li key={key} className="space-y-2 rounded-lg border border-border p-3">
            <div className="flex items-center justify-between gap-2">
              <span className="text-sm font-medium">{t(`values.${window.ownerKind}`)}</span>
              <Badge variant={tone(percent)}>{window.period}</Badge>
            </div>
            {/*
              `break-all`, because a window key is an opaque identifier with no
              promise of a space in it. A grid track cannot shrink below its
              item's min-content width, so one unbreakable key does not
              overflow its own card — it widens every card in the list and
              scrolls the whole document sideways. The glob below it breaks on
              words, since it has separators worth breaking at first.
            */}
            <p className="font-mono text-xs break-all text-muted-foreground">{window.windowKey}</p>
            <Progress tone={percent === null ? "default" : percent >= 100 ? "destructive" : percent >= 80 ? "warning" : "success"} value={percent === null ? 0 : Math.min(100, percent)} />
            <p className="text-sm">
              {amount(window.used, window.unit)}
              <span className="text-muted-foreground"> / {amount(window.limit, window.unit)}</span>
            </p>
            {window.modelPattern ? (
              <p className="text-xs break-words text-muted-foreground">
                {t("quota.pattern", { pattern: window.modelPattern })}
              </p>
            ) : null}
            <p className="text-xs text-muted-foreground">
              {window.resetsAtMs === null
                ? t("quota.permanent")
                : t("quota.resets", { at: formatInstant(window.resetsAtMs, i18n.language) })}
            </p>
          </li>
        )
      })}
    </ul>
  )
}
