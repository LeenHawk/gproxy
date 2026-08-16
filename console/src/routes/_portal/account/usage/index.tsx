import { useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { createFileRoute } from "@tanstack/react-router";
import { useTranslation } from "react-i18next";
import { myRollupsQuery, myUsagePageQuery, type MyUsageFilter } from "@/api/portal";
import type { Usage } from "@/api/usage";
import { UsageChart, type Metric } from "@/components/observability/usage-chart";
import {
  formatUsageTimestamp,
  UsageMobileCard,
} from "@/components/observability/usage-mobile-card";
import { MyUsageFilters } from "@/components/portal/my-usage-filters";
import { TimeRangePicker } from "@/components/time-range-picker";
import { DataTable, type DataColumn } from "@/components/data-table";
import { Pagination } from "@/components/pagination";
import { Badge } from "@/components/ui/badge";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { Skeleton } from "@/components/ui/skeleton";
import { aggregateRollups } from "@/lib/rollups";
import { pickGranularity, type BoundedTimeRange } from "@/lib/time-range";

const DEFAULT_SPAN_SECS = 7 * 86_400;

function defaultRange(): BoundedTimeRange {
  const now = Math.floor(Date.now() / 1000);
  return { from: now - DEFAULT_SPAN_SECS, to: now };
}

export const Route = createFileRoute("/_portal/account/usage/")({
  loader: ({ context }) => {
    const now = Math.floor(Date.now() / 1000);
    void context.queryClient.ensureQueryData(
      myRollupsQuery("day", now - 7 * 86_400, now),
    );
  },
  component: MyUsagePage,
});

function MyUsagePage() {
  const { t } = useTranslation("portal");
  const { t: tObs } = useTranslation("observability");

  // Initializer, not useMemo — the snapshot must not move between renders.
  const [range, setRange] = useState<BoundedTimeRange>(defaultRange);
  const [metric, setMetric] = useState<Metric>("requests");
  const [filter, setFilter] = useState<MyUsageFilter>({});
  const [page, setPage] = useState(1);

  const granularity = pickGranularity(range.from, range.to);
  const { data: rollupRows, isPending: rollupsPending } = useQuery(
    myRollupsQuery(granularity, range.from, range.to),
  );
  const points = rollupRows ? aggregateRollups(rollupRows) : [];

  const { data, isFetching, isPending } = useQuery(myUsagePageQuery(filter, page));
  const rows: Usage[] = data?.items ?? [];

  function changeFilter(next: MyUsageFilter) {
    setPage(1);
    setFilter(next);
  }

  // Columns — reuse observability keys for shared labels; portal keys for page-specific text
  const usageCols: DataColumn<Usage>[] = [
    {
      key: "at",
      header: tObs("usage.columns.at"),
      cell: (r) => (
        <span className="whitespace-nowrap font-mono text-xs text-muted-foreground">
          {formatUsageTimestamp(r.at)}
        </span>
      ),
    },
    {
      key: "operation",
      header: tObs("usage.columns.operation"),
      cell: (r) => (
        <span className="font-mono text-xs">
          {r.operation}
          {r.kind && r.kind !== r.operation && (
            <span className="text-muted-foreground"> / {r.kind}</span>
          )}
        </span>
      ),
    },
    {
      key: "model",
      header: tObs("usage.columns.model"),
      cell: (r) => <span className="font-mono text-xs">{r.model ?? "—"}</span>,
    },
    {
      key: "tokens",
      header: `${tObs("usage.columns.inputTokens")} / ${tObs("usage.columns.outputTokens")}`,
      cell: (r) => (
        <span className="tabular-nums text-xs">
          {r.input_tokens} / {r.output_tokens}
        </span>
      ),
    },
    {
      key: "imageOutputTokens",
      header: tObs("usage.columns.imageOutputTokens"),
      cell: (r) => <span className="tabular-nums text-xs">{r.image_output_tokens}</span>,
    },
    {
      key: "cache",
      header: `${tObs("usage.columns.cacheWrite")} (5m/1h)`,
      cell: (r) => (
        <span className="tabular-nums text-xs">
          {r.cache_creation_5m_tokens} / {r.cache_creation_1h_tokens}
        </span>
      ),
    },
    {
      key: "cache30m",
      header: `${tObs("usage.columns.cacheWrite")} (30m)`,
      cell: (r) => <span className="tabular-nums text-xs">{r.cache_creation_30m_tokens}</span>,
    },
    {
      key: "cacheRead",
      header: tObs("usage.columns.cacheRead"),
      cell: (r) => <span className="tabular-nums text-xs">{r.cache_read_tokens}</span>,
    },
    {
      key: "cost",
      header: tObs("usage.columns.cost"),
      cell: (r) => (
        <span className="tabular-nums text-xs">
          ${parseFloat(r.cost || "0").toFixed(5)}
        </span>
      ),
    },
    {
      key: "latency",
      header: tObs("usage.columns.latency"),
      cell: (r) => <span className="tabular-nums text-xs">{r.latency_ms}ms</span>,
    },
    {
      key: "badges",
      header: "",
      cell: (r) => (
        <div className="flex gap-1">
          <Badge variant="outline" className="text-xs">{r.usage_source}</Badge>
          <Badge variant="secondary" className="text-xs">{r.ended}</Badge>
        </div>
      ),
    },
  ];

  return (
    <div className="grid gap-6 p-4 md:p-6">
      {/* Header */}
      <div>
        <h1 className="text-xl font-semibold">{t("pages.usage.title")}</h1>
        <p className="text-sm text-muted-foreground">{t("pages.usage.subtitle")}</p>
      </div>

      {/* Usage chart */}
      <Card>
        <CardHeader>
          <div className="flex flex-wrap items-center justify-between gap-3">
            <CardTitle>{tObs(`chart.metric.${metric}`)}</CardTitle>
            <TimeRangePicker
              required
              align="end"
              value={range}
              onChange={(next) =>
                setRange({ from: next.from ?? range.from, to: next.to ?? range.to })
              }
            />
          </div>
        </CardHeader>
        <CardContent>
          {rollupsPending ? (
            <div aria-busy="true" className="space-y-2">
              <Skeleton className="h-8 w-48" />
              <Skeleton className="h-64" />
            </div>
          ) : (
            <UsageChart
              data={points}
              metric={metric}
              onMetricChange={setMetric}
              granularity={granularity}
            />
          )}
        </CardContent>
      </Card>

      {/* Filters */}
      <MyUsageFilters value={filter} onChange={changeFilter} />

      {/* Usage table — rows NOT clickable (no /user/logs endpoint) */}
      {isPending ? (
        <div className="space-y-2" aria-busy="true">
          {Array.from({ length: 5 }).map((_, i) => (
            <Skeleton key={i} className="h-10" />
          ))}
        </div>
      ) : (
        <DataTable
          columns={usageCols}
          rows={rows}
          rowKey={(r) => r.id}
          empty={t("usage.empty")}
          renderCard={(r) => <UsageMobileCard usage={r} />}
        />
      )}

      <Pagination
        page={page}
        totalPages={data?.pagination.total_pages ?? 0}
        onPageChange={setPage}
        disabled={isFetching}
      />
    </div>
  );
}
