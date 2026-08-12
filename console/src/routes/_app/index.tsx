import { useMemo, useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { createFileRoute } from "@tanstack/react-router";
import { useTranslation } from "react-i18next";
import { rollupsQuery } from "@/api/usage";
import { UsageChart, type Metric } from "@/components/observability/usage-chart";
import { HealthPanel } from "@/components/observability/health-panel";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { Skeleton } from "@/components/ui/skeleton";
import { aggregateRollups } from "@/lib/rollups";

const RANGES = [
  { key: "7d", secs: 7 * 86_400 },
  { key: "30d", secs: 30 * 86_400 },
] as const;
type RangeKey = (typeof RANGES)[number]["key"];

export const Route = createFileRoute("/_app/")({
  component: DashboardPage,
});

function DashboardPage() {
  const { t } = useTranslation(["common", "observability"]);
  const [range, setRange] = useState<RangeKey>("7d");
  const [metric, setMetric] = useState<Metric>("requests");

  const rangeDays = RANGES.find((r) => r.key === range)?.secs ?? 7 * 86_400;
  const { from, to } = useMemo(() => {
    const now = Math.floor(Date.now() / 1000);
    return { from: now - rangeDays, to: now };
  }, [rangeDays]);

  const { data: rollupRows, isPending: rollupsPending } = useQuery(
    rollupsQuery("day", from, to),
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
        {/* Time range selector */}
        <div className="flex items-center gap-2">
          <span className="text-sm text-muted-foreground">
            {t("observability:dashboard.range.label")}
          </span>
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
                {t(`observability:dashboard.range.${r.key}`)}
              </button>
            ))}
          </div>
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
            <UsageChart data={points} metric={metric} onMetricChange={setMetric} />
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
