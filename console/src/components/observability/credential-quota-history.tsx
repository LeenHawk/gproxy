import { useEffect, useMemo, useState } from "react";
import { useInfiniteQuery } from "@tanstack/react-query";
import { RefreshCw } from "lucide-react";
import { useTranslation } from "react-i18next";
import {
  Bar,
  BarChart,
  CartesianGrid,
  Cell,
  ResponsiveContainer,
  Tooltip,
  XAxis,
  YAxis,
} from "recharts";
import {
  fetchCredentialQuotaCycles,
  type CredentialQuotaCycle,
  type CredentialQuotaCycleFilter,
} from "@/api/credentials";
import { Button } from "@/components/ui/button";
import { CredentialQuotaCycleList } from "@/components/observability/credential-quota-cycle-list";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { Skeleton } from "@/components/ui/skeleton";
import {
  CHART_AXIS,
  CHART_TOOLTIP_STYLE,
  LegendChips,
  seriesColor,
} from "@/components/observability/chart-theme";
import {
  categorizedTotalTokens,
  decimalToChartNumber,
  formatUsageCount,
  formatUsageUsd,
} from "@/lib/credential-usage";

type CycleMetric = "tokens" | "cost" | "requests" | "percent";
const CYCLE_PAGE_SIZE = 200;

interface CredentialQuotaHistoryProps {
  credentialId?: number;
  providerId?: number;
  channel?: string;
  credentialLabel?: (credentialId: number) => string;
  compact?: boolean;
  hideWhenEmpty?: boolean;
}

function cycleTime(cycle: CredentialQuotaCycle): number {
  return cycle.period_end
    ?? cycle.last_observed_at
    ?? cycle.period_start
    ?? cycle.created_at;
}

function cycleLabel(cycle: CredentialQuotaCycle): string {
  return cycle.label?.trim() || cycle.name || cycle.window_key;
}

function cycleMetric(cycle: CredentialQuotaCycle, metric: CycleMetric): number {
  if (metric === "tokens") return categorizedTotalTokens(cycle);
  if (metric === "cost") return decimalToChartNumber(cycle.cost);
  if (metric === "percent") return Number(cycle.used_percent) || 0;
  return cycle.requests;
}

function formatMetric(value: number, metric: CycleMetric): string {
  if (metric === "cost") return formatUsageUsd(String(value));
  if (metric === "percent") return `${value.toFixed(1)}%`;
  return formatUsageCount(value);
}

function formatStamp(unixSeconds: number): string {
  return new Date(unixSeconds * 1000).toLocaleString(undefined, {
    month: "short",
    day: "numeric",
    hour: "2-digit",
    minute: "2-digit",
  });
}

export function CredentialQuotaHistory({
  credentialId,
  providerId,
  channel,
  credentialLabel,
  compact = false,
  hideWhenEmpty = false,
}: CredentialQuotaHistoryProps) {
  const { t } = useTranslation("observability");
  const filter = useMemo<CredentialQuotaCycleFilter>(() => ({
    credential_id: credentialId,
    provider_id: providerId,
    channel,
  }), [channel, credentialId, providerId]);
  const query = useInfiniteQuery({
    queryKey: ["credential-quota-cycles", "history", filter],
    initialPageParam: null as number | null,
    queryFn: ({ pageParam }) => fetchCredentialQuotaCycles({
      ...filter,
      before_id: pageParam ?? undefined,
      limit: CYCLE_PAGE_SIZE,
    }),
    getNextPageParam: (lastPage) => lastPage.length === CYCLE_PAGE_SIZE
      ? (lastPage.at(-1)?.id ?? undefined)
      : undefined,
    staleTime: 30_000,
  });
  const [windowKey, setWindowKey] = useState("__all__");
  const [status, setStatus] = useState("__all__");
  const [metric, setMetric] = useState<CycleMetric>("tokens");

  useEffect(() => {
    setWindowKey("__all__");
  }, [channel, credentialId, providerId]);

  const rows = useMemo(() => query.data?.pages.flat() ?? [], [query.data?.pages]);
  const windowOptions = useMemo(() => {
    const labels = new Map<string, string>();
    for (const cycle of rows) labels.set(cycle.window_key, cycleLabel(cycle));
    return Array.from(labels, ([key, label]) => ({ key, label }))
      .sort((left, right) => left.label.localeCompare(right.label));
  }, [rows]);
  // Color follows the window identity: slots come from the stable option list,
  // so changing filters never repaints the surviving bars.
  const windowSlots = useMemo(
    () => new Map(windowOptions.map((option, index) => [option.key, index])),
    [windowOptions],
  );
  const filtered = useMemo(() => rows
    .filter((cycle) => windowKey === "__all__" || cycle.window_key === windowKey)
    .filter((cycle) => status === "__all__" || cycle.status === status)
    .sort((left, right) => cycleTime(right) - cycleTime(left)),
  [rows, status, windowKey]);
  const chartData = useMemo(() => filtered
    .slice(0, 30)
    .reverse()
    .map((cycle) => ({
      id: cycle.id,
      at: cycleTime(cycle),
      value: cycleMetric(cycle, metric),
      exactValue: metric === "cost" ? cycle.cost : undefined,
      label: cycleLabel(cycle),
      windowKey: cycle.window_key,
      credential: credentialLabel?.(cycle.credential_id) ?? `#${cycle.credential_id}`,
    })),
  [credentialLabel, filtered, metric]);
  const chartLegend = useMemo(() => {
    const present = new Set(chartData.map((point) => point.windowKey));
    return windowOptions
      .filter((option) => present.has(option.key))
      .map((option) => ({
        label: option.label,
        color: seriesColor(windowSlots.get(option.key) ?? 0),
      }));
  }, [chartData, windowOptions, windowSlots]);

  if (query.isPending) {
    return <Skeleton className={compact ? "h-40" : "h-72"} aria-busy="true" />;
  }

  if (query.isError) {
    return (
      <div className="flex flex-wrap items-center justify-between gap-2 rounded-md border border-destructive/30 bg-destructive/5 p-3 text-sm text-destructive">
        <span>{t("quotaHistory.loadError")}</span>
        <Button variant="outline" size="sm" onClick={() => void query.refetch()}>
          <RefreshCw className="size-4" /> {t("quotaHistory.retry")}
        </Button>
      </div>
    );
  }

  if (rows.length === 0) {
    if (hideWhenEmpty) return null;
    return (
      <section className="rounded-md border bg-muted/20 px-3 py-4">
        <h3 className="text-sm font-medium">{t("quotaHistory.title")}</h3>
        <p className="mt-1 text-xs text-muted-foreground">{t("quotaHistory.empty")}</p>
      </section>
    );
  }

  return (
    <section className="grid min-w-0 gap-3" aria-label={t("quotaHistory.title")}>
      <div className="flex flex-wrap items-start justify-between gap-2">
        <div>
          <h3 className="text-sm font-medium">{t("quotaHistory.title")}</h3>
          <p className="text-xs text-muted-foreground">{t("quotaHistory.description")}</p>
        </div>
        <Button variant="outline" size="sm" disabled={query.isFetching} onClick={() => void query.refetch()}>
          <RefreshCw className={query.isFetching ? "size-4 animate-spin" : "size-4"} />
          {t("quotaHistory.refresh")}
        </Button>
      </div>

      <div className="flex flex-wrap items-center gap-2">
        <Select value={windowKey} onValueChange={setWindowKey}>
          <SelectTrigger size="sm" className="w-48">
            <SelectValue placeholder={t("quotaHistory.filters.window")} />
          </SelectTrigger>
          <SelectContent>
            <SelectItem value="__all__">{t("quotaHistory.filters.allWindows")}</SelectItem>
            {windowOptions.map((option) => (
              <SelectItem key={option.key} value={option.key}>{option.label}</SelectItem>
            ))}
          </SelectContent>
        </Select>

        <Select value={status} onValueChange={setStatus}>
          <SelectTrigger size="sm" className="w-36">
            <SelectValue placeholder={t("quotaHistory.filters.status")} />
          </SelectTrigger>
          <SelectContent>
            <SelectItem value="__all__">{t("quotaHistory.filters.allStatuses")}</SelectItem>
            <SelectItem value="open">{t("quotaHistory.status.open")}</SelectItem>
            <SelectItem value="finalized">{t("quotaHistory.status.finalized")}</SelectItem>
          </SelectContent>
        </Select>

        <div className="flex rounded-md border">
          {(["tokens", "cost", "requests", "percent"] as CycleMetric[]).map((item) => (
            <button
              key={item}
              type="button"
              onClick={() => setMetric(item)}
              className={item === metric
                ? "bg-primary px-2.5 py-1.5 text-xs font-medium text-primary-foreground first:rounded-l-md last:rounded-r-md"
                : "px-2.5 py-1.5 text-xs text-muted-foreground first:rounded-l-md last:rounded-r-md hover:bg-accent hover:text-accent-foreground"}
            >
              {t(`quotaHistory.metric.${item}`)}
            </button>
          ))}
        </div>
      </div>

      {filtered.length === 0 ? (
        <p className="rounded-md border py-8 text-center text-xs text-muted-foreground">{t("quotaHistory.filteredEmpty")}</p>
      ) : (
        <>
          <div className={compact ? "min-w-0 rounded-lg border bg-card p-2" : "min-w-0 rounded-lg border bg-card p-3"}>
            <LegendChips items={chartLegend} className="mb-1 px-1" />
            <div className={compact ? "h-40" : "h-60"}>
              <ResponsiveContainer width="100%" height="100%">
                <BarChart data={chartData} margin={{ top: 8, right: 8, bottom: 0, left: 4 }}>
                  <CartesianGrid vertical={false} stroke="var(--border)" />
                  <XAxis
                    dataKey="at"
                    tickFormatter={(value: number) => new Date(value * 1000).toLocaleDateString(undefined, { month: "short", day: "numeric" })}
                    {...CHART_AXIS}
                  />
                  <YAxis
                    tickFormatter={(value: number) => formatMetric(value, metric)}
                    width={64}
                    {...CHART_AXIS}
                  />
                  <Tooltip
                    cursor={{ fill: "var(--muted)", fillOpacity: 0.5 }}
                    contentStyle={CHART_TOOLTIP_STYLE}
                    labelFormatter={(_, payload) => {
                      const point = payload[0]?.payload as { at?: number; label?: string; credential?: string } | undefined;
                      if (!point?.at) return "";
                      return `${point.credential ?? ""} · ${point.label ?? ""} · ${formatStamp(point.at)}`;
                    }}
                    formatter={(value, _name, item) => {
                      const point = item.payload as { exactValue?: string } | undefined;
                      const formatted = metric === "cost" && point?.exactValue
                        ? formatUsageUsd(point.exactValue)
                        : formatMetric(Number(value), metric);
                      return [formatted, t(`quotaHistory.metric.${metric}`)];
                    }}
                  />
                  <Bar dataKey="value" radius={[4, 4, 0, 0]} maxBarSize={24} isAnimationActive={false}>
                    {chartData.map((point) => (
                      <Cell key={point.id} fill={seriesColor(windowSlots.get(point.windowKey) ?? 0)} />
                    ))}
                  </Bar>
                </BarChart>
              </ResponsiveContainer>
            </div>
          </div>

          <CredentialQuotaCycleList
            cycles={filtered}
            credentialId={credentialId}
            credentialLabel={credentialLabel}
            compact={compact}
            windowSlots={windowSlots}
          />

          {query.hasNextPage && (
            <div className="flex justify-center">
              <Button
                variant="outline"
                size="sm"
                disabled={query.isFetchingNextPage}
                onClick={() => void query.fetchNextPage()}
              >
                {query.isFetchingNextPage ? t("quotaHistory.loadingOlder") : t("quotaHistory.loadOlder")}
              </Button>
            </div>
          )}
        </>
      )}
    </section>
  );
}
