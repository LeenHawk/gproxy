import { useId, useState } from "react"
import { useTranslation } from "react-i18next"
import { Area, AreaChart, CartesianGrid, XAxis, YAxis } from "recharts"
import type { UsageTrendPointDto } from "@/generated/UsageTrendPointDto"
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card"
import { ChartContainer, ChartTooltip, ChartTooltipContent, type ChartConfig } from "@/components/ui/chart"
import { Skeleton } from "@/components/ui/skeleton"
import { ToggleGroup, ToggleGroupItem } from "@/components/ui/toggle-group"
import { formatCost, formatCount } from "@/lib/format"

type Metric = "requests" | "input_tokens" | "output_tokens" | "cached_input_tokens" | "cost"
type ChartPoint = Omit<UsageTrendPointDto, "cost"> & { cost: number }

const METRICS: Array<Metric> = ["requests", "input_tokens", "output_tokens", "cached_input_tokens", "cost"]

export function UsageTrendChart({ data, from, to, loading, error }: { data: Array<UsageTrendPointDto>; from: number; to: number; loading: boolean; error: boolean }) {
  const { t, i18n } = useTranslation()
  const [metric, setMetric] = useState<Metric>("requests")
  const gradientId = useId().replaceAll(":", "")
  const series = data.length === 0 ? [] : fillHours(data, from, to)
  const labels: Record<Metric, string> = {
    requests: t("usage.requests"),
    input_tokens: t("usage.inputTokens"),
    output_tokens: t("usage.outputTokens"),
    cached_input_tokens: t("usage.cachedTokens"),
    cost: t("usage.cost.label"),
  }
  const label = labels[metric]
  const config = { value: { label, color: "var(--primary)" } } satisfies ChartConfig
  const formatValue = (value: number) => metric === "cost" ? formatCost(value, i18n.language) : formatCount(value, i18n.language)
  const compact = new Intl.NumberFormat(i18n.language, { notation: "compact", maximumFractionDigits: 1 })

  return <Card>
    <CardHeader>
      <CardTitle>{t("overview.trend.title")}</CardTitle>
      <CardDescription>{t("overview.trend.description")}</CardDescription>
    </CardHeader>
    <CardContent className="flex min-w-0 flex-col gap-4">
      <ToggleGroup type="single" variant="outline" size="sm" value={metric} className="w-full flex-wrap justify-start" onValueChange={(value) => { if (value) setMetric(value as Metric) }}>
        {METRICS.map((value) => <ToggleGroupItem key={value} value={value}>{labels[value]}</ToggleGroupItem>)}
      </ToggleGroup>
      {loading ? <Skeleton className="h-80 w-full" /> : error ? <p className="flex h-80 items-center justify-center text-sm text-muted-foreground">{t("overview.trend.error")}</p> : series.length === 0 ? <p className="flex h-80 items-center justify-center text-sm text-muted-foreground">{t("overview.trend.empty")}</p> : <ChartContainer config={config} className="h-80 w-full aspect-auto">
        <AreaChart accessibilityLayer data={series} margin={{ top: 8, right: 8, left: 0, bottom: 0 }}>
          <defs><linearGradient id={gradientId} x1="0" y1="0" x2="0" y2="1"><stop offset="5%" stopColor="var(--color-value)" stopOpacity={0.3} /><stop offset="95%" stopColor="var(--color-value)" stopOpacity={0.02} /></linearGradient></defs>
          <CartesianGrid vertical={false} />
          <XAxis dataKey="bucket_start" tickLine={false} axisLine={false} tickMargin={10} minTickGap={42} tickFormatter={(value) => formatTick(Number(value), i18n.language)} />
          <YAxis width={52} tickLine={false} axisLine={false} tickFormatter={(value) => metric === "cost" ? formatCost(Number(value), i18n.language) : compact.format(Number(value))} />
          <ChartTooltip cursor={false} content={<ChartTooltipContent indicator="line" labelFormatter={(_, payload) => formatTooltip(Number(payload[0]?.payload.bucket_start), i18n.language)} formatter={(value) => <><span className="text-muted-foreground">{label}</span><span className="ml-auto font-mono font-medium text-foreground tabular-nums">{formatValue(Number(value))}</span></>} />} />
          <Area dataKey={metric} type="monotone" fill={`url(#${gradientId})`} stroke="var(--color-value)" strokeWidth={2} dot={false} isAnimationActive={false} />
        </AreaChart>
      </ChartContainer>}
    </CardContent>
  </Card>
}

function fillHours(rows: Array<UsageTrendPointDto>, from: number, to: number): Array<ChartPoint> {
  const byHour = new Map(rows.map((row) => [row.bucket_start, row]))
  const points: Array<ChartPoint> = []
  for (let bucket = from; bucket < to; bucket += 3_600) {
    const row = byHour.get(bucket)
    points.push(row ? { ...row, cost: Number(row.cost) } : { bucket_start: bucket, requests: 0, input_tokens: 0, output_tokens: 0, cached_input_tokens: 0, cost: 0 })
  }
  return points
}

function formatTick(value: number, locale: string) {
  return new Intl.DateTimeFormat(locale, { month: "numeric", day: "numeric" }).format(new Date(value * 1_000))
}

function formatTooltip(value: number, locale: string) {
  return new Intl.DateTimeFormat(locale, { dateStyle: "medium", timeStyle: "short" }).format(new Date(value * 1_000))
}
