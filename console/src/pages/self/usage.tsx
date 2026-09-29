//! The caller's own usage.
//!
//! The API returns summary, grouped totals and a trend over the same requested range.

import { lazy, Suspense, useState, type ReactNode } from "react"
import { useQuery, useQueryClient } from "@tanstack/react-query"
import { useTranslation } from "react-i18next"
import * as portal from "@/api/portal"
import * as observation from "@/api/observation"
import { HistoryFilters, type HistoryFilter } from "@/pages/observation/filters"
import { DataTable, IdCell } from "@/components/data-table"
import { Page, PageHeader, PageSection } from "@/components/page"
import { EmptyNotice, LoadingRows, QueryState } from "@/components/state"
import { UsageSummary } from "@/components/usage-summary"
import { Button } from "@/components/ui/button"
import { ToggleGroup, ToggleGroupItem } from "@/components/ui/toggle-group"
import { CACHE_TOKEN_FIELDS, formatCacheHitRate, formatUsageTokens } from "@/lib/usage"
import type { UsageGroupDto } from "@/generated/sdk"
import type { UsageGroupBy } from "@/generated/app"
import { formatCost, formatCount } from "@/lib/format"

const UsageTrend = lazy(() => import("@/components/usage-trend"))

const RANGES = { day: 86_400_000, week: 604_800_000, month: 2_592_000_000 } as const
type RangeKey = keyof typeof RANGES

const GROUPS: ReadonlyArray<UsageGroupBy> = ["model", "operation", "provider", "apiKey"]

/** Twenty-four buckets over whatever the range is: a readable trend at any width. */
const BUCKETS = 24

export function UsagePage({ global = false, renderRecords }: { global?: boolean; renderRecords?: (filter: HistoryFilter) => ReactNode } = {}) {
  const { t, i18n } = useTranslation()
  const queryClient = useQueryClient()
  const [filter, setFilter] = useState<HistoryFilter>({})
  const [range, setRange] = useState<RangeKey>("week")
  const [groupBy, setGroupBy] = useState<UsageGroupBy>("model")

  // The window is computed inside the query function, not in the render.
  // Reading the clock during a render makes the same component produce two
  // different answers for one state, and it would put a value that changes
  // every millisecond into the query key.
  const usage = useQuery({
    queryKey: [global ? "admin" : "portal", "usage", range, groupBy, filter],
    queryFn: () => {
      const toMs = filter.toMs ?? Date.now()
      const fromMs = filter.fromMs ?? toMs - RANGES[range]
      return (global ? observation.usage : portal.usage)({
        ...filter,
        fromMs,
        toMs,
        groupBy,
        bucketMs: Math.max(1, Math.ceil((toMs - fromMs) / BUCKETS)),
      })
    },
  })


  return (
    <Page>
      <PageHeader title={t(global ? "nav.globalUsage" : "nav.usage")} actions={<Button size="sm" variant="outline" onClick={() => { void usage.refetch(); if (global) void queryClient.invalidateQueries({ queryKey: ["admin", "usage-records"] }) }}>{t("observation.refresh")}</Button>} />
      {global ? <HistoryFilters summary onApply={setFilter} /> : null}
      <ToggleGroup type="single" value={range} onValueChange={value => {
        if (value) { setRange(value as RangeKey); setFilter({ ...filter, fromMs: undefined, toMs: undefined }) }
      }} size="sm" className="flex-wrap" aria-label={t("usage.timeRange")}>
        {(Object.keys(RANGES) as Array<RangeKey>).map(key => (
          <ToggleGroupItem key={key} value={key}>{t(`range.${key}`)}</ToggleGroupItem>
        ))}
      </ToggleGroup>
      <QueryState isPending={usage.isPending} error={usage.error}>
        {usage.data ? (
          <div className="flex flex-col gap-6">
            <UsageSummary summary={usage.data.summary} />

            <PageSection title={t("usage.trend")}>
              <Suspense fallback={<LoadingRows />}><UsageTrend points={usage.data.trend} /></Suspense>
            </PageSection>

            <PageSection
              title={t("usage.groups")}
              actions={
                <ToggleGroup type="single" value={groupBy} onValueChange={value => { if (value) setGroupBy(value as UsageGroupBy) }} size="sm" className="flex-wrap" aria-label={t("usage.groups")}>
                  {(global ? ["user" as const, ...GROUPS] : GROUPS).map(group => (
                    <ToggleGroupItem key={group} value={group}>{t(`values.${group}`)}</ToggleGroupItem>
                  ))}
                </ToggleGroup>
              }
            >
              <DataTable
                columns={[
                  { key: "key", cell: (row) => <IdCell value={row.key ?? "—"} /> },
                  { key: "requests", cell: (row) => formatCount(row.summary.requests, i18n.language) },
                  { key: "inputTokens", cell: (row) => formatCount(row.summary.inputTokens, i18n.language) },
                  { key: "outputTokens", cell: (row) => formatCount(row.summary.outputTokens, i18n.language) },
                  ...CACHE_TOKEN_FIELDS.map(key => ({ key, cell: (row: UsageGroupDto) => formatUsageTokens(row.summary[key], i18n.language) })),
                  { key: "cacheHitRate", cell: (row) => formatCacheHitRate(row.summary, i18n.language) },
                  { key: "cost", cell: (row) => formatCost(row.summary.cost, i18n.language) },
                ]}
                rows={usage.data.groups}
                rowKey={(row) => row.key ?? "none"}
                empty={<EmptyNotice title={t("usage.emptyTitle")} />}
              />
            </PageSection>
          </div>
        ) : null}
      </QueryState>
      {usage.data && renderRecords ? renderRecords({ ...filter, fromMs: usage.data.fromMs ?? undefined, toMs: usage.data.toMs ?? undefined }) : null}
    </Page>
  )
}
