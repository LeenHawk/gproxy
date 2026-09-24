//! The caller's own usage.
//!
//! The API returns summary, grouped totals and a trend over the same requested range.

import { useState, type ReactNode } from "react"
import { useQuery, useQueryClient } from "@tanstack/react-query"
import { useTranslation } from "react-i18next"
import * as portal from "@/api/portal"
import * as observation from "@/api/observation"
import { HistoryFilters, type HistoryFilter } from "@/pages/observation/filters"
import { DataTable, IdCell } from "@/components/data-table"
import { Page, PageHeader, PageSection } from "@/components/page"
import { EmptyNotice, QueryState } from "@/components/state"
import { UsageSummary } from "@/components/usage-summary"
import { Button } from "@/components/ui/button"
import type { UsageGroupBy } from "@/generated/app"
import { formatCost, formatCount, formatInstant } from "@/lib/format"

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

  const peak = Math.max(1, ...(usage.data?.trend ?? []).map((point) => Number(point.summary.cost)))

  return (
    <Page>
      <PageHeader title={t(global ? "nav.globalUsage" : "nav.usage")} actions={<Button size="sm" variant="outline" onClick={() => { void usage.refetch(); if (global) void queryClient.invalidateQueries({ queryKey: ["admin", "usage-records"] }) }}>{t("observation.refresh")}</Button>} />
      {global ? <HistoryFilters summary onApply={setFilter} /> : null}
      <div className="flex flex-wrap gap-2">
        {(Object.keys(RANGES) as Array<RangeKey>).map((key) => (
          <Button
            key={key}
            size="sm"
            variant={range === key ? "secondary" : "ghost"}
            onClick={() => { setRange(key); setFilter({ ...filter, fromMs: undefined, toMs: undefined }) }}
          >
            {t(`range.${key}`)}
          </Button>
        ))}
      </div>
      <QueryState isPending={usage.isPending} error={usage.error}>
        {usage.data ? (
          <div className="space-y-6">
            <UsageSummary summary={usage.data.summary} />

            <PageSection title={t("usage.trend")}>
              <div className="flex h-32 items-end gap-1" role="img" aria-label={t("usage.trend")}>
                {usage.data.trend.map((point) => (
                  <div
                    key={point.startMs}
                    className="min-w-1 flex-1 rounded-t bg-primary/70"
                    style={{ height: `${Math.max(2, (Number(point.summary.cost) / peak) * 100)}%` }}
                    title={`${formatInstant(point.startMs, i18n.language)} · ${formatCost(point.summary.cost, i18n.language)}`}
                  />
                ))}
              </div>
            </PageSection>

            <PageSection
              title={t("usage.groups")}
              actions={
                <span className="flex flex-wrap gap-1">
                  {(global ? ["user" as const, ...GROUPS] : GROUPS).map((group) => (
                    <Button
                      key={group}
                      size="xs"
                      variant={groupBy === group ? "secondary" : "ghost"}
                      onClick={() => setGroupBy(group)}
                    >
                      {t(`values.${group}`)}
                    </Button>
                  ))}
                </span>
              }
            >
              <DataTable
                columns={[
                  { key: "key", cell: (row) => <IdCell value={row.key ?? "—"} /> },
                  { key: "requests", cell: (row) => formatCount(row.summary.requests, i18n.language) },
                  { key: "inputTokens", cell: (row) => formatCount(row.summary.inputTokens, i18n.language) },
                  { key: "outputTokens", cell: (row) => formatCount(row.summary.outputTokens, i18n.language) },
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
