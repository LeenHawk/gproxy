import { useMemo, useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { RefreshCw } from "lucide-react";
import { useTranslation } from "react-i18next";
import {
  CartesianGrid,
  Line,
  LineChart,
  ResponsiveContainer,
  Tooltip,
  XAxis,
  YAxis,
} from "recharts";
import {
  credentialUsageComparisonQuery,
  type CredentialQuotaCycle,
  type CredentialUsageComparison,
  type UsageTokenTotals,
} from "@/api/credentials";
import { providersQuery } from "@/api/providers";
import { CredentialQuotaHistory } from "@/components/observability/credential-quota-history";
import {
  CHART_AXIS,
  CHART_TOOLTIP_STYLE,
  LegendChips,
  Meter,
  seriesColor,
} from "@/components/observability/chart-theme";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { Skeleton } from "@/components/ui/skeleton";
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@/components/ui/table";
import {
  categorizedTotalTokens,
  compareDecimalStrings,
  decimalToChartNumber,
  formatUsageCount,
  formatUsageUsd,
  normalizeLastSevenDays,
  sumUsageTotals,
} from "@/lib/credential-usage";

type ComparisonMetric = "total_tokens" | "cost_usd" | "requests";
type ChartDatum = { day_start: number } & Record<string, number | string>;

function credentialName(row: CredentialUsageComparison): string {
  return row.credential_label?.trim() || `#${row.credential_id}`;
}

function chartMetricValue(totals: UsageTokenTotals, metric: ComparisonMetric): number {
  if (metric === "cost_usd") return decimalToChartNumber(totals.cost_usd);
  return totals[metric];
}

function formatMetric(value: number, metric: ComparisonMetric): string {
  if (metric === "cost_usd") return formatUsageUsd(String(value));
  return formatUsageCount(value);
}

function formatDay(unixSeconds: number): string {
  return new Date(unixSeconds * 1000).toLocaleDateString(undefined, {
    month: "short",
    day: "numeric",
  });
}

function QuotaWindowsCell({ windows }: { windows: CredentialQuotaCycle[] }) {
  if (!windows.length) return <span className="text-muted-foreground">—</span>;
  return (
    <div className="grid min-w-44 gap-1">
      {windows.map((window) => {
        const percent = window.used_percent !== null ? Number(window.used_percent) : undefined;
        const label = window.label || window.name;
        return (
          <div key={window.id} className="flex items-center gap-2 text-xs">
            <span className="w-24 truncate text-muted-foreground" title={label}>{label}</span>
            {percent !== undefined ? (
              <>
                <Meter percent={percent} className="w-16 flex-none" />
                <span className="w-9 text-right tabular-nums">{percent.toFixed(0)}%</span>
              </>
            ) : (
              <span className="tabular-nums text-muted-foreground">
                {formatUsageCount(categorizedTotalTokens(window))}
              </span>
            )}
          </div>
        );
      })}
    </div>
  );
}

export function CredentialUsageComparisonTab() {
  const { t } = useTranslation("observability");
  const comparison = useQuery(credentialUsageComparisonQuery);
  const { data: providers } = useQuery(providersQuery);
  const [providerId, setProviderId] = useState<string>("__all__");
  const [channel, setChannel] = useState<string>("__all__");
  const [credentialId, setCredentialId] = useState<string>("__all__");
  const [metric, setMetric] = useState<ComparisonMetric>("total_tokens");

  const rows = comparison.data ?? [];
  const credentialNameMap = useMemo(
    () => new Map(rows.map((row) => [row.credential_id, credentialName(row)])),
    [rows],
  );
  // Color follows the credential, not its filtered rank: slots are assigned once
  // per load from the lifetime-token ordering, so filters never repaint survivors.
  const colorSlots = useMemo(() => {
    const ordered = [...rows].sort((a, b) => b.lifetime.total_tokens - a.lifetime.total_tokens);
    return new Map(ordered.map((row, index) => [row.credential_id, index]));
  }, [rows]);
  const channels = useMemo(
    () => Array.from(new Set(rows.map((row) => row.channel))).sort(),
    [rows],
  );
  const providerMap = useMemo(
    () => new Map((providers ?? []).map((provider) => [provider.id, provider.label ?? provider.name])),
    [providers],
  );
  const filtered = useMemo(() => rows
    .filter((row) => providerId === "__all__" || row.provider_id === Number(providerId))
    .filter((row) => channel === "__all__" || row.channel === channel)
    .filter((row) => credentialId === "__all__" || row.credential_id === Number(credentialId))
    .sort((a, b) => metric === "cost_usd"
      ? compareDecimalStrings(b.lifetime.cost_usd, a.lifetime.cost_usd)
      : chartMetricValue(b.lifetime, metric) - chartMetricValue(a.lifetime, metric)),
  [channel, credentialId, metric, providerId, rows]);

  const selectableCredentials = useMemo(() => rows
    .filter((row) => providerId === "__all__" || row.provider_id === Number(providerId))
    .filter((row) => channel === "__all__" || row.channel === channel)
    .sort((a, b) => credentialName(a).localeCompare(credentialName(b))),
  [channel, providerId, rows]);

  // Keep multi-line charts legible. A credential filter still allows any row
  // to be viewed independently; the comparison table always includes all rows.
  const chartRows = filtered.slice(0, 5);
  const chartData = useMemo(() => {
    const base = normalizeLastSevenDays([]).map((day) => day.day_start);
    return base.map((dayStart, index) => {
      const result: ChartDatum = { day_start: dayStart };
      for (const row of chartRows) {
        const day = normalizeLastSevenDays(row.last_7_days)[index];
        const key = `credential_${row.credential_id}`;
        result[key] = day ? chartMetricValue(day.totals, metric) : 0;
        if (metric === "cost_usd") result[`${key}_exact`] = day?.totals.cost_usd ?? "0";
      }
      return result;
    });
  }, [chartRows, metric]);
  const chartLegend = useMemo(() => chartRows.map((row) => ({
    label: credentialName(row),
    color: seriesColor(colorSlots.get(row.credential_id) ?? 0),
  })), [chartRows, colorSlots]);

  function changeProvider(value: string) {
    setProviderId(value);
    setCredentialId("__all__");
  }

  function changeChannel(value: string) {
    setChannel(value);
    setCredentialId("__all__");
  }

  if (comparison.isPending) {
    return (
      <div className="grid gap-3" aria-busy="true">
        <Skeleton className="h-8 w-80 max-w-full" />
        <Skeleton className="h-72" />
        <Skeleton className="h-48" />
      </div>
    );
  }

  if (comparison.isError) {
    return (
      <div className="grid justify-items-start gap-2 rounded-md border border-destructive/30 bg-destructive/5 p-4 text-sm text-destructive">
        <p>{t("credentialUsage.loadError")}</p>
        <Button variant="outline" size="sm" onClick={() => void comparison.refetch()}>
          <RefreshCw className="size-4" /> {t("credentialUsage.retry")}
        </Button>
      </div>
    );
  }

  return (
    <section className="grid gap-4" aria-label={t("credentialUsage.title")}>
      <div className="flex flex-wrap items-start justify-between gap-3">
        <div>
          <h2 className="text-base font-medium">{t("credentialUsage.title")}</h2>
          <p className="text-sm text-muted-foreground">{t("credentialUsage.description")}</p>
        </div>
        <Button
          variant="outline"
          size="sm"
          disabled={comparison.isFetching}
          onClick={() => void comparison.refetch()}
        >
          <RefreshCw className={comparison.isFetching ? "size-4 animate-spin" : "size-4"} />
          {t("credentialUsage.refresh")}
        </Button>
      </div>

      <div className="flex flex-wrap items-center gap-2">
        <Select value={providerId} onValueChange={changeProvider}>
          <SelectTrigger size="sm" className="w-44">
            <SelectValue placeholder={t("credentialUsage.filters.provider")} />
          </SelectTrigger>
          <SelectContent>
            <SelectItem value="__all__">{t("credentialUsage.filters.allProviders")}</SelectItem>
            {(providers ?? []).map((provider) => (
              <SelectItem key={provider.id} value={String(provider.id)}>
                {provider.label ?? provider.name}
              </SelectItem>
            ))}
          </SelectContent>
        </Select>

        <Select value={channel} onValueChange={changeChannel}>
          <SelectTrigger size="sm" className="w-44">
            <SelectValue placeholder={t("credentialUsage.filters.channel")} />
          </SelectTrigger>
          <SelectContent>
            <SelectItem value="__all__">{t("credentialUsage.filters.allChannels")}</SelectItem>
            {channels.map((item) => <SelectItem key={item} value={item}>{item}</SelectItem>)}
          </SelectContent>
        </Select>

        <Select value={credentialId} onValueChange={setCredentialId}>
          <SelectTrigger size="sm" className="w-44">
            <SelectValue placeholder={t("credentialUsage.filters.credential")} />
          </SelectTrigger>
          <SelectContent>
            <SelectItem value="__all__">{t("credentialUsage.filters.allCredentials")}</SelectItem>
            {selectableCredentials.map((row) => (
              <SelectItem key={row.credential_id} value={String(row.credential_id)}>
                {credentialName(row)}
              </SelectItem>
            ))}
          </SelectContent>
        </Select>

        <div className="flex rounded-md border">
          {(["total_tokens", "cost_usd", "requests"] as ComparisonMetric[]).map((item) => (
            <button
              key={item}
              type="button"
              onClick={() => setMetric(item)}
              className={item === metric
                ? "px-3 py-1.5 text-xs font-medium first:rounded-l-md last:rounded-r-md bg-primary text-primary-foreground"
                : "px-3 py-1.5 text-xs text-muted-foreground first:rounded-l-md last:rounded-r-md hover:bg-accent hover:text-accent-foreground"}
            >
              {t(`credentialUsage.metric.${item}`)}
            </button>
          ))}
        </div>
      </div>

      {filtered.length === 0 ? (
        <p className="rounded-md border py-12 text-center text-sm text-muted-foreground">
          {t("credentialUsage.empty")}
        </p>
      ) : (
        <>
          <div className="min-w-0 rounded-lg border bg-card p-3">
            <div className="mb-2 flex flex-wrap items-baseline justify-between gap-2">
              <h3 className="text-sm font-medium">{t("credentialUsage.last7DaysTrend")}</h3>
              {filtered.length > chartRows.length && (
                <p className="text-xs text-muted-foreground">
                  {t("credentialUsage.chartLimit", { count: chartRows.length })}
                </p>
              )}
            </div>
            <LegendChips items={chartLegend} className="mb-2" />
            <div className="h-64">
              <ResponsiveContainer width="100%" height="100%">
                <LineChart data={chartData} margin={{ top: 8, right: 12, bottom: 0, left: 4 }}>
                  <CartesianGrid vertical={false} stroke="var(--border)" />
                  <XAxis dataKey="day_start" tickFormatter={formatDay} {...CHART_AXIS} />
                  <YAxis
                    tickFormatter={(value: number) => formatMetric(value, metric)}
                    width={68}
                    {...CHART_AXIS}
                  />
                  <Tooltip
                    contentStyle={CHART_TOOLTIP_STYLE}
                    labelFormatter={(value) => formatDay(Number(value))}
                    formatter={(value, name, item) => {
                      const dataKey = String(item.dataKey ?? "");
                      const point = item.payload as Record<string, unknown> | undefined;
                      const exact = point?.[`${dataKey}_exact`];
                      const formatted = metric === "cost_usd" && typeof exact === "string"
                        ? formatUsageUsd(exact)
                        : formatMetric(Number(value), metric);
                      return [formatted, String(name)];
                    }}
                  />
                  {chartRows.map((row) => (
                    <Line
                      key={row.credential_id}
                      type="monotone"
                      dataKey={`credential_${row.credential_id}`}
                      name={credentialName(row)}
                      stroke={seriesColor(colorSlots.get(row.credential_id) ?? 0)}
                      strokeWidth={2}
                      strokeLinecap="round"
                      dot={false}
                      activeDot={{ r: 4, strokeWidth: 2, stroke: "var(--card)" }}
                      isAnimationActive={false}
                    />
                  ))}
                </LineChart>
              </ResponsiveContainer>
            </div>
          </div>

          <div className="min-w-0 overflow-x-auto rounded-lg border bg-card">
            <Table>
              <TableHeader>
                <TableRow>
                  <TableHead>{t("credentialUsage.columns.credential")}</TableHead>
                  <TableHead>{t("credentialUsage.columns.provider")}</TableHead>
                  <TableHead>{t("credentialUsage.columns.channel")}</TableHead>
                  <TableHead className="text-right">{t("credentialUsage.columns.last7Tokens")}</TableHead>
                  <TableHead className="text-right">{t("credentialUsage.columns.last7Cost")}</TableHead>
                  <TableHead className="text-right">{t("credentialUsage.columns.lifetimeTokens")}</TableHead>
                  <TableHead className="text-right">{t("credentialUsage.columns.lifetimeCost")}</TableHead>
                  <TableHead>{t("credentialUsage.columns.quotaWindows")}</TableHead>
                  <TableHead>{t("credentialUsage.columns.coverage")}</TableHead>
                </TableRow>
              </TableHeader>
              <TableBody>
                {filtered.map((row) => {
                  const recent = sumUsageTotals(normalizeLastSevenDays(row.last_7_days).map((day) => day.totals));
                  const slot = colorSlots.get(row.credential_id);
                  return (
                    <TableRow key={row.credential_id}>
                      <TableCell className="font-medium">
                        <span className="flex items-center gap-1.5">
                          {slot !== undefined && slot < 5 && (
                            <span
                              aria-hidden
                              className="size-2 shrink-0 rounded-full"
                              style={{ background: seriesColor(slot) }}
                            />
                          )}
                          {credentialName(row)}
                        </span>
                      </TableCell>
                      <TableCell>{providerMap.get(row.provider_id) ?? `#${row.provider_id}`}</TableCell>
                      <TableCell>
                        <Badge variant="secondary" className="font-mono text-[11px]">{row.channel}</Badge>
                      </TableCell>
                      <TableCell className="text-right font-mono tabular-nums">{formatUsageCount(recent.total_tokens)}</TableCell>
                      <TableCell className="text-right font-mono tabular-nums">{formatUsageUsd(recent.cost_usd)}</TableCell>
                      <TableCell className="text-right font-mono tabular-nums">{formatUsageCount(row.lifetime.total_tokens)}</TableCell>
                      <TableCell className="text-right font-mono tabular-nums">{formatUsageUsd(row.lifetime.cost_usd)}</TableCell>
                      <TableCell><QuotaWindowsCell windows={row.current_windows} /></TableCell>
                      <TableCell className="text-xs text-muted-foreground">
                        {row.coverage_start !== undefined
                          ? new Date(row.coverage_start * 1000).toLocaleDateString()
                          : t("credentialUsage.coverageUnknown")}
                      </TableCell>
                    </TableRow>
                  );
                })}
              </TableBody>
            </Table>
          </div>

          <CredentialQuotaHistory
            providerId={providerId === "__all__" ? undefined : Number(providerId)}
            channel={channel === "__all__" ? undefined : channel}
            credentialId={credentialId === "__all__" ? undefined : Number(credentialId)}
            credentialLabel={(id) => credentialNameMap.get(id) ?? `#${id}`}
          />
        </>
      )}
    </section>
  );
}
