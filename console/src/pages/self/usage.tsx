//! The caller's own usage.
//!
//! One query object answers three questions at once — the totals, an optional
//! grouped cut and an optional trend — because the server computes all three
//! from one scan. Asking for the trend separately would scan the same records
//! twice and could disagree with itself at a bucket boundary.

import { useState } from "react"
import { useQuery } from "@tanstack/react-query"
import { useTranslation } from "react-i18next"
import * as portal from "@/api/portal"
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

export function UsagePage() {
  const { t, i18n } = useTranslation()
  const [range, setRange] = useState<RangeKey>("week")
  const [groupBy, setGroupBy] = useState<UsageGroupBy>("model")

  // The window is computed inside the query function, not in the render.
  // Reading the clock during a render makes the same component produce two
  // different answers for one state, and it would put a value that changes
  // every millisecond into the query key.
  const usage = useQuery({
    queryKey: ["portal", "usage", range, groupBy],
    queryFn: () => {
      const now = Date.now()
      return portal.usage({
        fromMs: now - RANGES[range],
        toMs: now,
        groupBy,
        bucketMs: Math.floor(RANGES[range] / BUCKETS),
      })
    },
  })

  const peak = Math.max(1, ...(usage.data?.trend ?? []).map((point) => Number(point.summary.cost)))

  return (
    <Page>
      <PageHeader title={t("nav.usage")} description={t("description.usage")} />
      <div className="flex flex-wrap gap-2">
        {(Object.keys(RANGES) as Array<RangeKey>).map((key) => (
          <Button
            key={key}
            size="sm"
            variant={range === key ? "secondary" : "ghost"}
            onClick={() => setRange(key)}
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
                  {GROUPS.map((group) => (
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
                empty={<EmptyNotice title={t("usage.emptyTitle")} description={t("usage.emptyDescription")} />}
              />
            </PageSection>
          </div>
        ) : null}
      </QueryState>
    </Page>
  )
}
