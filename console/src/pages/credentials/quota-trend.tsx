import { useState } from "react"
import { useInfiniteQuery } from "@tanstack/react-query"
import { useTranslation } from "react-i18next"
import { CartesianGrid, Line, LineChart, XAxis, YAxis } from "recharts"
import { credentialQuotaObservations } from "@/api/credentials"
import { ChartContainer, ChartTooltip, ChartTooltipContent } from "@/components/ui/chart"
import { Button } from "@/components/ui/button"
import { ToggleGroup, ToggleGroupItem } from "@/components/ui/toggle-group"
import { EmptyNotice, QueryState } from "@/components/state"
import { formatInstant, formatPercent } from "@/lib/format"
import type { CredentialCycleDto } from "@/generated/sdk"
import { quotaTrendPoints } from "./quota-trend-data"

const RANGES = { day: 86_400_000, week: 604_800_000 } as const

type Range = "current" | keyof typeof RANGES

export default function QuotaTrend({ id, windowId, title, cycle }: { id: string; windowId: string; title: string; cycle?: CredentialCycleDto }) {
  const { t, i18n } = useTranslation()
  const [range, setRange] = useState<Range>("current")
  const history = useInfiniteQuery({
    queryKey: ["credential-quota-observations", id, range, range === "current" ? [cycle?.id, cycle?.startsAtMs, cycle?.endsAtMs] : null],
    enabled: range !== "current" || !!cycle,
    initialPageParam: { page: 1, untilMs: 0 },
    queryFn: async ({ pageParam, signal }) => {
      // Freeze the time range while paging so a new observation cannot shift the offsets.
      const untilMs = pageParam.untilMs || Math.min(Date.now(), range === "current" ? cycle?.endsAtMs ?? Infinity : Infinity)
      const sinceMs = range === "current" ? cycle!.startsAtMs : untilMs - RANGES[range]
      const result = await credentialQuotaObservations(id, { sinceMs, untilMs, page: pageParam.page, pageSize: 500 }, signal)
      return { ...result, sinceMs, untilMs }
    },
    getNextPageParam: last => last.offset + last.items.length < last.total
      ? { page: Math.floor(last.offset / last.limit) + 2, untilMs: last.untilMs } : undefined,
  })
  const rows = history.data?.pages.flatMap(page => page.items) ?? []
  const points = quotaTrendPoints(range === "current" ? rows.filter(row => row.cycleId === cycle?.id) : rows, windowId)
  const hasValues = points.some(point => point.percent !== null)
  const bounds = history.data?.pages[0]
  const ticks = bounds ? Array.from({ length: 5 }, (_, index) => bounds.sinceMs + (bounds.untilMs - bounds.sinceMs) * index / 4) : []
  const percent = (value: number) => formatPercent(value / 100, i18n.language)
  const date = new Intl.DateTimeFormat(i18n.language, { month: "numeric", day: "numeric", hour: "2-digit", minute: "2-digit", hour12: false })
  return <div className="flex min-w-0 flex-col gap-3 pt-3">
    <div className="flex flex-wrap items-center justify-between gap-2">
      <ToggleGroup type="single" size="sm" value={range} onValueChange={value => { if (value) setRange(value as Range) }} aria-label={t("usage.timeRange")}>
        <ToggleGroupItem value="current">{t("limits.currentCycle")}</ToggleGroupItem>
        {(Object.keys(RANGES) as Array<keyof typeof RANGES>).map(key => <ToggleGroupItem key={key} value={key}>{t(`range.${key}`)}</ToggleGroupItem>)}
      </ToggleGroup>
      <Button type="button" variant="ghost" size="sm" disabled={history.isFetching || (range === "current" && !cycle)} onClick={() => void history.refetch()}>{t("observation.refresh")}</Button>
    </div>
    {range === "current" && !cycle ? <EmptyNotice title={t("limits.noCurrentCycle")} /> : <QueryState isPending={history.isPending} error={history.error}>
      {hasValues && bounds ? <ChartContainer className="h-56 w-full" role="img" aria-label={`${title} · ${t("limits.quotaTrend")}`} config={{ percent: { label: t("limits.quotaUsedPercent"), color: "var(--state-info)" } }}>
        <LineChart accessibilityLayer data={points} margin={{ left: 0, right: 12, top: 8, bottom: 0 }}>
          <CartesianGrid vertical={false} />
          <XAxis dataKey="time" type="number" scale="time" domain={[bounds.sinceMs, bounds.untilMs]} ticks={ticks} tickFormatter={value => date.format(Number(value))} minTickGap={48} tickLine={false} axisLine={false} />
          <YAxis domain={[0, (max: number) => Math.max(100, max)]} width={52} tickFormatter={value => percent(Number(value))} tickLine={false} axisLine={false} />
          <ChartTooltip content={<ChartTooltipContent labelFormatter={(_, payload) => formatInstant(payload[0]?.payload.time, i18n.language)} formatter={value => <span>{t("limits.quotaUsedPercent")}: {percent(Number(value))}</span>} />} />
          <Line dataKey="percent" type="linear" stroke="var(--color-percent)" strokeWidth={2} dot={{ r: 2 }} activeDot={{ r: 4 }} connectNulls={false} isAnimationActive={false} />
        </LineChart>
      </ChartContainer> : <EmptyNotice title={t("limits.quotaTrendEmpty")} />}
    </QueryState>}
    {history.hasNextPage ? <Button type="button" variant="outline" size="sm" className="self-start" disabled={history.isFetching} onClick={() => void history.fetchNextPage()}>{t("limits.quotaTrendOlder")}</Button> : null}
    <p className="text-xs text-muted-foreground">{t("limits.quotaTrendHint")}</p>
  </div>
}
