import { useMemo, useState } from "react";
import { useInfiniteQuery, useQuery } from "@tanstack/react-query";
import { createFileRoute } from "@tanstack/react-router";
import { useTranslation } from "react-i18next";
import { myRollupsQuery, myUsageInfiniteQuery, type MyUsageFilter } from "@/api/portal";
import type { Usage } from "@/api/usage";
import { UsageChart, type Metric } from "@/components/observability/usage-chart";
import { MyUsageFilters } from "@/components/portal/my-usage-filters";
import { DataTable, type DataColumn } from "@/components/data-table";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { Skeleton } from "@/components/ui/skeleton";
import { aggregateRollups } from "@/lib/rollups";

const RANGES = [
  { key: "7d", secs: 7 * 86_400 },
  { key: "30d", secs: 30 * 86_400 },
] as const;
type RangeKey = (typeof RANGES)[number]["key"];

export const Route = createFileRoute("/_portal/account/usage/")({
  loader: ({ context }) => {
    const now = Math.floor(Date.now() / 1000);
    void context.queryClient.ensureQueryData(
      myRollupsQuery("day", now - 7 * 86_400, now),
    );
  },
  component: MyUsagePage,
});

function fmtAt(unixSecs: number): string {
  return new Date(unixSecs * 1000).toLocaleString(undefined, {
    month: "short",
    day: "numeric",
    hour: "2-digit",
    minute: "2-digit",
    second: "2-digit",
  });
}

function MyUsagePage() {
  const { t } = useTranslation("portal");
  const { t: tObs } = useTranslation("observability");

  const [range, setRange] = useState<RangeKey>("7d");
  const [metric, setMetric] = useState<Metric>("requests");
  const [filter, setFilter] = useState<MyUsageFilter>({});

  // Stable from/to snapshot — avoids re-querying on every render (Dashboard pattern)
  const rangeSecs = RANGES.find((r) => r.key === range)?.secs ?? 7 * 86_400;
  const { from, to: toUnix } = useMemo(() => {
    const now = Math.floor(Date.now() / 1000);
    return { from: now - rangeSecs, to: now };
  }, [rangeSecs]);

  const { data: rollupRows, isPending: rollupsPending } = useQuery(
    myRollupsQuery("day", from, toUnix),
  );
  const points = rollupRows ? aggregateRollups(rollupRows) : [];

  const { data, fetchNextPage, hasNextPage, isFetchingNextPage, isPending } =
    useInfiniteQuery(myUsageInfiniteQuery(filter));
  const rows: Usage[] = data?.pages.flat() ?? [];

  // Columns — reuse observability keys for shared labels; portal keys for page-specific text
  const usageCols: DataColumn<Usage>[] = [
    {
      key: "at",
      header: tObs("usage.columns.at"),
      cell: (r) => (
        <span className="whitespace-nowrap font-mono text-xs text-muted-foreground">
          {fmtAt(r.at)}
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
            {/* Range selector */}
            <div className="flex gap-1">
              {RANGES.map((r) => (
                <button
                  key={r.key}
                  type="button"
                  onClick={() => setRange(r.key)}
                  className={
                    r.key === range
                      ? "rounded-md bg-primary px-3 py-1 text-xs font-medium text-primary-foreground"
                      : "rounded-md border px-3 py-1 text-xs text-muted-foreground hover:bg-accent hover:text-accent-foreground"
                  }
                >
                  {tObs(`dashboard.range.${r.key}`)}
                </button>
              ))}
            </div>
          </div>
        </CardHeader>
        <CardContent>
          {rollupsPending ? (
            <div aria-busy="true" className="space-y-2">
              <Skeleton className="h-8 w-48" />
              <Skeleton className="h-64" />
            </div>
          ) : (
            <UsageChart data={points} metric={metric} onMetricChange={setMetric} />
          )}
        </CardContent>
      </Card>

      {/* Filters */}
      <MyUsageFilters value={filter} onChange={setFilter} />

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
          renderCard={(r) => (
            <div className="grid gap-1">
              <div className="flex items-center justify-between gap-2">
                <span className="font-mono text-xs">{r.operation}</span>
                <span className="tabular-nums text-xs">${parseFloat(r.cost || "0").toFixed(5)}</span>
              </div>
              <div className="flex flex-wrap gap-1 text-xs text-muted-foreground">
                <span>{fmtAt(r.at)}</span>
                {r.model && <span>· {r.model}</span>}
                <span>· {r.latency_ms}ms</span>
              </div>
              <div className="flex gap-1">
                <Badge variant="outline" className="text-xs">{r.usage_source}</Badge>
                <Badge variant="secondary" className="text-xs">{r.ended}</Badge>
              </div>
            </div>
          )}
        />
      )}

      {hasNextPage && (
        <div className="flex justify-center pt-2">
          <Button
            variant="outline"
            size="sm"
            onClick={() => void fetchNextPage()}
            disabled={isFetchingNextPage}
          >
            {isFetchingNextPage ? "…" : t("usage.loadMore")}
          </Button>
        </div>
      )}
    </div>
  );
}
