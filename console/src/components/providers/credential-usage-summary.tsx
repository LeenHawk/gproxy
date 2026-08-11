import { useMemo } from "react";
import { useQuery } from "@tanstack/react-query";
import { ChevronsUpDown } from "lucide-react";
import { useTranslation } from "react-i18next";
import {
  Bar,
  BarChart,
  CartesianGrid,
  ResponsiveContainer,
  Tooltip,
  XAxis,
  YAxis,
} from "recharts";
import {
  credentialUsageSummaryQuery,
  type UsageModelTotals,
} from "@/api/credentials";
import { ApiError } from "@/api/http";
import { Button } from "@/components/ui/button";
import { Collapsible, CollapsibleContent, CollapsibleTrigger } from "@/components/ui/collapsible";
import { Skeleton } from "@/components/ui/skeleton";
import { CHART_AXIS, CHART_TOOLTIP_STYLE } from "@/components/observability/chart-theme";
import {
  formatUsageCount,
  formatUsageUsd,
  isPositiveDecimal,
  normalizeLastSevenDays,
  sumUsageTotals,
} from "@/lib/credential-usage";

function formatDay(unixSeconds: number): string {
  return new Date(unixSeconds * 1000).toLocaleDateString(undefined, {
    month: "short",
    day: "numeric",
  });
}

function StatTile({ label, value, secondary }: { label: string; value: string; secondary: string }) {
  return (
    <div className="min-w-0 rounded-lg border bg-card px-3 py-2.5">
      <p className="truncate text-xs text-muted-foreground">{label}</p>
      <p className="mt-0.5 truncate text-lg font-semibold" title={value}>{value}</p>
      <p className="truncate text-xs text-muted-foreground" title={secondary}>{secondary}</p>
    </div>
  );
}

export function modelUsageBreakdown(model: UsageModelTotals, t: (key: string) => string): string {
  const parts: Array<[number, string]> = [
    [model.input_tokens, t("usage.local.input")],
    [model.output_tokens, t("usage.local.output")],
    [model.image_output_tokens, t("usage.local.imageOutput")],
    [model.cache_read_tokens, t("usage.local.cacheRead")],
    [model.cache_creation_tokens, t("usage.local.cacheCreation")],
  ];
  return [
    `${formatUsageCount(model.requests)} ${t("usage.local.requests")}`,
    ...parts.filter(([count]) => count > 0)
      .map(([count, label]) => `${label} ${formatUsageCount(count)}`),
  ].join(" · ");
}

function ModelBreakdown({ models }: { models: UsageModelTotals[] }) {
  const { t } = useTranslation("providers");
  if (models.length === 0) return null;

  return (
    <Collapsible>
      <CollapsibleTrigger asChild>
        <Button variant="ghost" size="sm" className="h-8 px-1 text-xs text-muted-foreground">
          <ChevronsUpDown className="size-3" />
          {t("usage.local.byModel", { count: models.length })}
        </Button>
      </CollapsibleTrigger>
      <CollapsibleContent className="grid gap-1 border-t pt-2">
        {models.map((model, index) => (
          <div key={`${model.model}-${index}`} className="grid gap-0.5 rounded-md px-2 py-1.5 text-xs hover:bg-muted/50">
            <div className="flex flex-wrap items-baseline justify-between gap-x-3 gap-y-1">
              <span className="break-all font-mono font-medium">{model.model}</span>
              <span className="tabular-nums">
                {formatUsageCount(model.total_tokens)} {t("usage.local.tokens")} · {formatUsageUsd(model.cost_usd)}
              </span>
            </div>
            <span className="text-muted-foreground">{modelUsageBreakdown(model, t)}</span>
          </div>
        ))}
      </CollapsibleContent>
    </Collapsible>
  );
}

export function CredentialUsageSummaryCard({ credentialId }: { credentialId: number }) {
  const { t } = useTranslation("providers");
  const query = useQuery(credentialUsageSummaryQuery(credentialId));
  const days = useMemo(
    () => normalizeLastSevenDays(query.data?.last_7_days ?? []),
    [query.data?.last_7_days],
  );
  const recentTotals = useMemo(
    () => sumUsageTotals(days.map((day) => day.totals)),
    [days],
  );
  const chartData = useMemo(
    () => days.map((day) => ({
      day_start: day.day_start,
      total_tokens: day.totals.total_tokens,
    })),
    [days],
  );

  if (query.isPending) {
    return (
      <div className="grid min-w-0 gap-3" aria-busy="true">
        <Skeleton className="h-5 w-48" />
        <div className="grid grid-cols-3 gap-2">
          <Skeleton className="h-20" /><Skeleton className="h-20" /><Skeleton className="h-20" />
        </div>
        <Skeleton className="h-36" />
      </div>
    );
  }

  if (query.isError || !query.data) {
    const message = query.error instanceof ApiError ? query.error.message : String(query.error ?? "");
    return (
      <div className="rounded-md border border-destructive/30 bg-destructive/5 px-3 py-2 text-sm text-destructive">
        {t("usage.local.summaryLoadError")}{message ? `: ${message}` : ""}
      </div>
    );
  }

  const summary = query.data;
  const recentPrefix = t("usage.local.last7Days");
  const hasRecentUsage = recentTotals.requests > 0
    || recentTotals.total_tokens > 0
    || isPositiveDecimal(recentTotals.cost_usd);

  return (
    <section className="grid min-w-0 gap-3" aria-label={t("usage.local.summaryTitle")}>
      <div className="flex flex-wrap items-start justify-between gap-2">
        <div>
          <h3 className="text-sm font-medium">{t("usage.local.summaryTitle")}</h3>
          <p className="text-xs text-muted-foreground">{t("usage.local.summaryDescription")}</p>
        </div>
        {summary.coverage_start !== undefined && (
          <p className="text-xs text-muted-foreground">
            {t("usage.local.trackedSince", {
              time: new Date(summary.coverage_start * 1000).toLocaleString(),
            })}
          </p>
        )}
      </div>

      <div className="grid min-w-0 grid-cols-3 gap-2">
        <StatTile
          label={t("usage.local.metric.tokens")}
          value={formatUsageCount(summary.lifetime.total_tokens)}
          secondary={`${recentPrefix} · ${formatUsageCount(recentTotals.total_tokens)}`}
        />
        <StatTile
          label={t("usage.local.metric.requests")}
          value={formatUsageCount(summary.lifetime.requests)}
          secondary={`${recentPrefix} · ${formatUsageCount(recentTotals.requests)}`}
        />
        <StatTile
          label={t("usage.local.metric.cost")}
          value={formatUsageUsd(summary.lifetime.cost_usd)}
          secondary={`${recentPrefix} · ${formatUsageUsd(recentTotals.cost_usd)}`}
        />
      </div>

      <div className="min-w-0 rounded-lg border bg-card p-2">
        {hasRecentUsage ? (
          <div className="h-36" aria-label={t("usage.local.dailyTokens")}>
            <ResponsiveContainer width="100%" height="100%">
              <BarChart data={chartData} margin={{ top: 8, right: 4, bottom: 0, left: 4 }}>
                <CartesianGrid vertical={false} stroke="var(--border)" />
                <XAxis dataKey="day_start" tickFormatter={formatDay} {...CHART_AXIS} />
                <YAxis
                  width={44}
                  tickFormatter={(value: number) => formatUsageCount(value)}
                  {...CHART_AXIS}
                />
                <Tooltip
                  cursor={{ fill: "var(--muted)", fillOpacity: 0.5 }}
                  contentStyle={CHART_TOOLTIP_STYLE}
                  labelFormatter={(value) => formatDay(Number(value))}
                  formatter={(value) => [
                    `${formatUsageCount(Number(value))} ${t("usage.local.tokens")}`,
                    t("usage.local.dailyTokens"),
                  ]}
                />
                <Bar
                  dataKey="total_tokens"
                  fill="var(--chart-1)"
                  radius={[4, 4, 0, 0]}
                  maxBarSize={24}
                  isAnimationActive={false}
                />
              </BarChart>
            </ResponsiveContainer>
          </div>
        ) : (
          <p className="py-6 text-center text-xs text-muted-foreground">{t("usage.local.noRecordedUsage")}</p>
        )}
      </div>

      <ModelBreakdown models={summary.by_model} />
    </section>
  );
}
