import { useTranslation } from "react-i18next"
import type { QuotaBreakdownRowDto } from "@/generated/sdk"
import { cn } from "@/lib/utils"
import { formatPercent } from "@/lib/format"
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card"

const colors: Record<string, string> = {
  claude_code: "bg-state-info", chat: "bg-state-healthy", cowork: "bg-state-warning", other: "bg-state-disabled",
}
const order = Object.keys(colors)

export function UpstreamBreakdown({ rows }: { rows: QuotaBreakdownRowDto[] }) {
  const { t, i18n } = useTranslation()
  const parts = [...rows].sort((a, b) => {
    const rank = (key: string) => order.includes(key) ? order.indexOf(key) : order.length
    return rank(a.key) - rank(b.key)
  }).map(row => ({
    ...row, percent: Number(row.percent), color: colors[row.key] ?? colors.other,
    name: t(`limits.usageSources.${row.key}`, { defaultValue: row.label ?? row.key }),
  }))
  const percent = (value: number) => formatPercent(value / 100, i18n.language)
  const description = parts.map(part => `${part.name} ${percent(part.percent)}`).join(" · ")
  return <Card size="sm" className="gap-2 py-3"><CardHeader className="px-3"><CardTitle>{t("limits.weeklyBreakdown")}</CardTitle></CardHeader><CardContent className="flex flex-col gap-2 px-3">
    <div role="img" aria-label={`${t("limits.weeklyBreakdown")}: ${description}`} className="flex h-2 overflow-hidden rounded-full bg-muted">
      {parts.filter(part => part.percent > 0).map(part => <span key={part.key} className={part.color} style={{ flexGrow: part.percent, flexBasis: 0 }} title={`${part.name}: ${percent(part.percent)}`} />)}
    </div>
    <div className="flex flex-wrap gap-x-4 gap-y-1 text-xs text-muted-foreground">{parts.map(part => <span key={part.key} className="inline-flex items-center gap-1.5"><span aria-hidden className={cn("size-2 shrink-0 rounded-full", part.color)} />{part.name} <span className="tabular-nums">{percent(part.percent)}</span></span>)}</div>
  </CardContent></Card>
}
