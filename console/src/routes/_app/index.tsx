import { useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { createFileRoute } from "@tanstack/react-router";
import { useTranslation } from "react-i18next";
import { rollupsQuery } from "@/api/usage";
import { UsageChart, type Metric } from "@/components/observability/usage-chart";
import { HealthPanel } from "@/components/observability/health-panel";
import { TimeRangePicker } from "@/components/time-range-picker";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { Skeleton } from "@/components/ui/skeleton";
import { aggregateRollups } from "@/lib/rollups";
import { pickGranularity, type BoundedTimeRange } from "@/lib/time-range";

const DEFAULT_SPAN_SECS = 7 * 86_400;

function defaultRange(): BoundedTimeRange {
  const now = Math.floor(Date.now() / 1000);
  return { from: now - DEFAULT_SPAN_SECS, to: now };
}

export const Route = createFileRoute("/_app/")({
  component: DashboardPage,
});

function DashboardPage() {
  const { t } = useTranslation(["common", "observability"]);
  // Initializer, not useMemo — the snapshot must not move between renders.
  const [range, setRange] = useState<BoundedTimeRange>(defaultRange);
  const [metric, setMetric] = useState<Metric>("requests");

  const granularity = pickGranularity(range.from, range.to);
  const { data: rollupRows, isPending: rollupsPending } = useQuery(
    rollupsQuery(granularity, range.from, range.to),
  );

  const points = rollupRows ? aggregateRollups(rollupRows) : [];

  return (
    <div className="grid gap-6 p-4 md:p-6">
      {/* Header */}
      <div className="flex flex-wrap items-center justify-between gap-3">
        <div>
          <h1 className="text-xl font-semibold">{t("common:nav.dashboard")}</h1>
          <p className="text-sm text-muted-foreground">
            {t("observability:dashboard.subtitle")}
          </p>
        </div>
        {/* Time range */}
        <div className="flex items-center gap-2">
          <span className="text-sm text-muted-foreground">
            {t("observability:dashboard.range.label")}
          </span>
          <TimeRangePicker
            required
            align="end"
            value={range}
            onChange={(next) =>
              setRange({ from: next.from ?? range.from, to: next.to ?? range.to })
            }
          />
        </div>
      </div>

      {/* Usage chart */}
      <Card>
        <CardHeader>
          <CardTitle>{t(`observability:chart.metric.${metric}`)}</CardTitle>
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

      {/* Credential health */}
      <Card>
        <CardHeader>
          <CardTitle>{t("observability:health.title")}</CardTitle>
        </CardHeader>
        <CardContent>
          <HealthPanel />
        </CardContent>
      </Card>
    </div>
  );
}
