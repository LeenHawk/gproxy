import { useTranslation } from "react-i18next"
import { CartesianGrid, ErrorBar, Line, LineChart, Scatter, ScatterChart, XAxis, YAxis } from "recharts"
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card"
import { ChartContainer, ChartTooltip, ChartTooltipContent } from "@/components/ui/chart"
import { formatCost, formatCount, formatInstant, formatPercent } from "@/lib/format"
import { cyclePoints, roundRange, type QuotaMetric, type QuotaSeries } from "./quota-history-data"

export function QuotaHistoryChart({ series, metric, mode }: { series: Array<QuotaSeries>; metric: QuotaMetric; mode: "within" | "across" }) {
  const { t, i18n } = useTranslation()
  const format = (value: number) => metric === "cost" ? formatCost(value, i18n.language) : metric === "percent" ? formatPercent(value / 100, i18n.language) : formatCount(Math.round(value), i18n.language)
  const plots = series.flatMap((series) => series.cycles.map((cycle) => ({
    id: `cycle_${cycle.id}`, color: series.color,
    label: `${series.label} · ${formatInstant(cycle.accounting_start_ms / 1000, i18n.language)} · #${cycle.id}`,
    points: cyclePoints(cycle, metric), range: roundRange(cycle, metric),
  })))
  const data = plots.flatMap((plot) => mode === "within" ? plot.points : plot.range ? [plot.range] : [])
  const config = Object.fromEntries(plots.map((plot) => [plot.id, { label: plot.label, color: plot.color }]))
  const hasValues = data.some((point) => point.value != null)
  const date = (value: number) => new Intl.DateTimeFormat(i18n.language, { month: "numeric", day: "numeric", hour: "2-digit", minute: "2-digit" }).format(value)
  const Plot = mode === "within" ? LineChart : ScatterChart
  return <Card>
    <CardHeader>
      <CardTitle>{t(`usage.quotaHistory.${mode}Title`)}</CardTitle>
      <CardDescription>{t(`usage.quotaHistory.${mode}Description`)}</CardDescription>
    </CardHeader>
    <CardContent className="flex min-w-0 flex-col gap-4">
      {hasValues ? <ChartContainer config={config} className="h-80 w-full aspect-auto" aria-label={t(`usage.quotaHistory.${mode}Title`)}>
        <Plot accessibilityLayer data={data} margin={{ top: 12, right: 20, left: 0, bottom: 8 }}>
          <CartesianGrid vertical={false} />
          <XAxis dataKey="at" type="number" domain={["dataMin", "dataMax"]} tickFormatter={date} minTickGap={50} tickLine={false} axisLine={false} />
          <YAxis dataKey="value" type="number" domain={metric === "percent" ? [0, 100] : [0, "auto"]} width={64} tickFormatter={format} tickLine={false} axisLine={false} />
          <ChartTooltip content={({ active, label, payload }) => {
            const selected = mode === "within" ? plots.flatMap((plot) => {
              const point = plot.points.find((point) => point.at === Number(label))
              return point?.value != null ? [{ graphicalItemId: plot.id, name: plot.id, value: point.value, payload: point }] : []
            }) : plots.flatMap((plot) => plot.range && payload[0]?.payload.cycleId === plot.range.cycleId ? [{ graphicalItemId: plot.id, name: plot.id, value: plot.range.value, payload: plot.range }] : [])
            if (!active || !selected.length) return null
            return <ChartTooltipContent active={active} payload={selected} label={formatInstant(Number(selected[0]?.payload.at) / 1000, i18n.language)} className="max-w-[min(32rem,85vw)]" formatter={(value, name, item) => {
              const range = item.payload as { minimum?: number; maximum?: number; count?: number; observedAt?: number }
              return <div className="flex min-w-0 flex-col gap-1">
                <span className="break-words text-muted-foreground">{config[String(name)]?.label ?? name}</span>
                <span className="font-mono">{format(Number(value))}</span>
                {mode === "across" && range.minimum != null && range.maximum != null && <>
                  <span>{t("usage.quotaHistory.range", { min: format(range.minimum), max: format(range.maximum), count: range.count })}</span>
                  <span>{t("usage.cycleUsage.observed", { value: formatInstant(Number(range.observedAt) / 1000, i18n.language) })}</span>
                </>}
              </div>
            }} />
          }} />
          {plots.map((plot) => mode === "within" ? <Line key={plot.id} name={plot.id} data={plot.points} dataKey="value" type="linear" stroke={plot.color} strokeWidth={2} dot={{ r: 2 }} connectNulls={false} isAnimationActive={false} /> : plot.range ? <Scatter key={plot.id} name={plot.id} data={[plot.range]} dataKey="value" fill={plot.color} isAnimationActive={false}>
            <ErrorBar dataKey="range" direction="y" width={12} stroke={plot.color} strokeWidth={3} />
          </Scatter> : null)}
        </Plot>
      </ChartContainer> : <p className="flex h-48 items-center justify-center text-sm text-muted-foreground">{t(series.length ? "usage.quotaHistory.noSamples" : "usage.quotaHistory.noSelection")}</p>}
      <ul className="flex flex-wrap gap-x-5 gap-y-2" aria-label={t("usage.quotaHistory.series")}>
        {series.map((series) => <li key={series.id} className="flex min-w-0 items-center gap-2 text-xs"><span className="size-2 shrink-0 rounded-full" style={{ backgroundColor: series.color }} /><span className="break-words">{series.label}</span></li>)}
      </ul>
    </CardContent>
  </Card>
}
