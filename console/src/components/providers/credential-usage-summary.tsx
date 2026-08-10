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
  type UsageTokenTotals,
} from "@/api/credentials";
import { ApiError } from "@/api/http";
import { Button } from "@/components/ui/button";
import { Collapsible, CollapsibleContent, CollapsibleTrigger } from "@/components/ui/collapsible";
import { Skeleton } from "@/components/ui/skeleton";
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

function Metric({ label, value }: { label: string; value: string }) {
  return (
    <div className="min-w-0 rounded-md border bg-card px-3 py-2">
      <p className="text-xs text-muted-foreground">{label}</p>
      <p className="mt-1 truncate font-mono text-sm font-semibold tabular-nums" title={value}>{value}</p>
    </div>
  );
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
        {models.map((model, index) => {
          const breakdown = [
            `${formatUsageCount(model.requests)} ${t("usage.local.requests")}`,
            model.input_tokens > 0
              ? `${t("usage.local.input")} ${formatUsageCount(model.input_tokens)}`
              : undefined,
            model.output_tokens > 0
              ? `${t("usage.local.output")} ${formatUsageCount(model.output_tokens)}`
              : undefined,
            model.image_output_tokens > 0
              ? `${t("usage.local.imageOutput")} ${formatUsageCount(model.image_output_tokens)}`
              : undefined,
            model.cache_read_tokens > 0
              ? `${t("usage.local.cacheRead")} ${formatUsageCount(model.cache_read_tokens)}`
              : undefined,
            model.cache_creation_tokens > 0
              ? `${t("usage.local.cacheCreation")} ${formatUsageCount(model.cache_creation_tokens)}`
              : undefined,
          ].filter((part): part is string => part !== undefined);

          return (
            <div key={`${model.model}-${index}`} className="grid gap-0.5 rounded-md px-2 py-1.5 text-xs hover:bg-muted/50">
              <div className="flex flex-wrap items-baseline justify-between gap-x-3 gap-y-1">
                <span className="break-all font-mono font-medium">{model.model}</span>
                <span className="tabular-nums">
                  {formatUsageCount(model.total_tokens)} {t("usage.local.tokens")} · {formatUsageUsd(model.cost_usd)}
                </span>
              </div>
              <span className="text-muted-foreground">{breakdown.join(" · ")}</span>
            </div>
          );
        })}
      </CollapsibleContent>
    </Collapsible>
  );
}

function TotalsGrid({ totals, prefix }: { totals: UsageTokenTotals; prefix: string }) {
  const { t } = useTranslation("providers");
  return (
    <div className="grid grid-cols-3 gap-2">
      <Metric label={`${prefix} · ${t("usage.local.tokens")}`} value={formatUsageCount(totals.total_tokens)} />
      <Metric label={`${prefix} · ${t("usage.local.requests")}`} value={formatUsageCount(totals.requests)} />
      <Metric label={`${prefix} · USD`} value={formatUsageUsd(totals.cost_usd)} />
    </div>
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
      <div className="grid gap-3" aria-busy="true">
        <Skeleton className="h-5 w-48" />
        <div className="grid grid-cols-3 gap-2">
          <Skeleton className="h-16" /><Skeleton className="h-16" /><Skeleton className="h-16" />
        </div>
        <Skeleton className="h-32" />
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
  const hasRecentUsage = recentTotals.requests > 0
    || recentTotals.total_tokens > 0
    || isPositiveDecimal(recentTotals.cost_usd);

  return (
    <section className="grid gap-3" aria-label={t("usage.local.summaryTitle")}>
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

      <TotalsGrid totals={summary.lifetime} prefix={t("usage.local.lifetime")} />
      <TotalsGrid totals={recentTotals} prefix={t("usage.local.last7Days")} />

      <div className="rounded-md border bg-muted/20 p-2">
        {hasRecentUsage ? (
          <>
            <div className="h-32" aria-label={t("usage.local.dailyTokens")}>
              <ResponsiveContainer width="100%" height="100%">
                <BarChart data={chartData} margin={{ top: 4, right: 4, bottom: 0, left: 4 }}>
                  <CartesianGrid vertical={false} strokeDasharray="3 3" stroke="var(--border)" />
                  <XAxis
                    dataKey="day_start"
                    tickFormatter={(value: number) => formatDay(value)}
                    tick={{ fontSize: 10 }}
                    stroke="var(--muted-foreground)"
                  />
                  <YAxis hide />
                  <Tooltip
                    cursor={{ fill: "var(--muted)" }}
                    contentStyle={{
                      background: "var(--popover)",
                      border: "1px solid var(--border)",
                      borderRadius: "0.5rem",
                      fontSize: 12,
                    }}
                    labelFormatter={(value) => formatDay(Number(value))}
                    formatter={(value) => [
                      `${formatUsageCount(Number(value))} ${t("usage.local.tokens")}`,
                      t("usage.local.dailyTokens"),
                    ]}
                  />
                  <Bar dataKey="total_tokens" fill="var(--chart-1)" radius={[3, 3, 0, 0]} isAnimationActive={false} />
                </BarChart>
              </ResponsiveContainer>
            </div>
            <div className="mt-1 grid grid-cols-4 gap-1 sm:grid-cols-7">
              {days.map((day) => (
                <div key={day.day_start} className="min-w-0 rounded px-1 py-1 text-center text-[10px] text-muted-foreground">
                  <p>{formatDay(day.day_start)}</p>
                  <p className="truncate font-mono text-foreground" title={String(day.totals.total_tokens)}>
                    {formatUsageCount(day.totals.total_tokens)}
                  </p>
                </div>
              ))}
            </div>
          </>
        ) : (
          <p className="py-6 text-center text-xs text-muted-foreground">{t("usage.local.noRecordedUsage")}</p>
        )}
      </div>

      <ModelBreakdown models={summary.by_model} />
    </section>
  );
}
