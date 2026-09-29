import { useState } from "react"
import { useTranslation } from "react-i18next"
import { CartesianGrid, Line, LineChart, XAxis, YAxis } from "recharts"
import { ChartContainer, ChartTooltip, ChartTooltipContent } from "@/components/ui/chart"
import { ToggleGroup, ToggleGroupItem } from "@/components/ui/toggle-group"
import { Table, TableBody, TableCaption, TableCell, TableHead, TableHeader, TableRow } from "@/components/ui/table"
import { formatCost, formatCount, formatInstant } from "@/lib/format"
import type { UsageTrendPointDto } from "@/generated/sdk"

const METRICS = ["requests", "cost", "inputTokens", "outputTokens"] as const
export default function UsageTrend({ points }: { points: UsageTrendPointDto[] }) {
  const { t, i18n } = useTranslation()
  const [metric, setMetric] = useState<typeof METRICS[number]>("requests")
  const data = points.map(point => ({ time: point.startMs, value: Number(point.summary[metric]) }))
  const label = t(`fields.${metric}`)
  const format = (value: number) => metric === "cost" ? formatCost(String(value), i18n.language) : formatCount(value, i18n.language)
  const date = new Intl.DateTimeFormat(i18n.language, { month: "numeric", day: "numeric", hour: "2-digit" })
  return <div className="flex min-w-0 flex-col gap-3">
    <ToggleGroup type="single" size="sm" value={metric} onValueChange={value => { if (value) setMetric(value as typeof metric) }} className="flex-wrap" aria-label={t("usage.trendMetric")}>
      {METRICS.map(key => <ToggleGroupItem key={key} value={key}>{t(`fields.${key}`)}</ToggleGroupItem>)}
    </ToggleGroup>
    <ChartContainer className="h-56 w-full" config={{ value: { label, color: "var(--state-info)" } }}>
      <LineChart accessibilityLayer data={data} margin={{ left: 0, right: 12, top: 8, bottom: 0 }}>
        <CartesianGrid vertical={false} />
        <XAxis dataKey="time" tickFormatter={value => date.format(Number(value))} minTickGap={48} tickLine={false} axisLine={false} />
        <YAxis width={62} tickFormatter={value => new Intl.NumberFormat(i18n.language, { notation: "compact" }).format(Number(value))} tickLine={false} axisLine={false} allowDecimals={metric === "cost"} />
        <ChartTooltip content={<ChartTooltipContent labelFormatter={(_, payload) => formatInstant(payload[0]?.payload.time, i18n.language)} formatter={value => <span>{label}: {format(Number(value))}</span>} />} />
        <Line dataKey="value" type="linear" stroke="var(--color-value)" strokeWidth={2} dot={false} isAnimationActive={false} />
      </LineChart>
    </ChartContainer>
    <details><summary className="w-fit cursor-pointer py-2 text-sm">{t("usage.trendData")}</summary>
      <Table><TableCaption className="sr-only">{t("usage.trend")}</TableCaption>
        <TableHeader><TableRow><TableHead>{t("usage.periodStart")}</TableHead><TableHead>{label}</TableHead></TableRow></TableHeader>
        <TableBody>{data.map(point => <TableRow key={point.time}><TableCell>{formatInstant(point.time, i18n.language)}</TableCell><TableCell>{format(point.value)}</TableCell></TableRow>)}</TableBody>
      </Table>
    </details>
  </div>
}
