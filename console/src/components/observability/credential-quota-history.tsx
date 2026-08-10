import { Fragment, useEffect, useMemo, useState } from "react";
import { useInfiniteQuery, useQuery } from "@tanstack/react-query";
import { ChevronsUpDown, RefreshCw } from "lucide-react";
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
  credentialQuotaCycleDetailQuery,
  fetchCredentialQuotaCycles,
  type CredentialQuotaCycle,
  type CredentialQuotaCycleFilter,
} from "@/api/credentials";
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

function formatCyclePeriod(cycle: CredentialQuotaCycle): string {
  const format = (value: number) => new Date(value * 1000).toLocaleString();
  if (cycle.period_start !== null && cycle.period_end !== null) {
    return `${format(cycle.period_start)} – ${format(cycle.period_end)}`;
  }
  if (cycle.period_start !== null) return `${format(cycle.period_start)} – …`;
  if (cycle.period_end !== null) return `… – ${format(cycle.period_end)}`;
  if (cycle.last_observed_at !== null) return format(cycle.last_observed_at);
  return "—";
}

function formatCycleEstimate(cycle: CredentialQuotaCycle, tokensLabel: string): string {
  const parts = [
    cycle.estimated_tokens !== null
      ? `≈${formatUsageCount(cycle.estimated_tokens)} ${tokensLabel}`
      : undefined,
    cycle.estimated_cost !== null
      ? `≈${formatUsageUsd(cycle.estimated_cost)}`
      : undefined,
  ].filter((part): part is string => part !== undefined);
  return parts.join(" · ") || "—";
}

function CycleModels({ cycleId }: { cycleId: number }) {
  const { t } = useTranslation("observability");
  const detail = useQuery(credentialQuotaCycleDetailQuery(cycleId));

  if (detail.isPending) return <Skeleton className="h-16 w-full" />;
  if (detail.isError) return <p className="text-xs text-destructive">{t("quotaHistory.modelLoadError")}</p>;
  if (!detail.data.by_model.length) {
    return <p className="text-xs text-muted-foreground">{t("quotaHistory.noModels")}</p>;
  }

  return (
    <div className="grid gap-1">
      {detail.data.by_model.map((model) => (
        <div key={model.id} className="flex flex-wrap items-baseline justify-between gap-2 rounded-md bg-muted/40 px-2 py-1.5 text-xs">
          <span className="break-all font-mono font-medium">{model.model}</span>
          <span className="tabular-nums text-muted-foreground">
            {formatUsageCount(categorizedTotalTokens(model))} {t("quotaHistory.tokens")} · {formatUsageUsd(model.cost)} · {formatUsageCount(model.requests)} {t("quotaHistory.requests")}
          </span>
        </div>
      ))}
    </div>
  );
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
  const [expandedCycle, setExpandedCycle] = useState<number | null>(null);

  useEffect(() => {
    setWindowKey("__all__");
    setExpandedCycle(null);
  }, [channel, credentialId, providerId]);

  const rows = useMemo(() => query.data?.pages.flat() ?? [], [query.data?.pages]);
  const windowOptions = useMemo(() => {
    const labels = new Map<string, string>();
    for (const cycle of rows) labels.set(cycle.window_key, cycleLabel(cycle));
    return Array.from(labels, ([key, label]) => ({ key, label }))
      .sort((left, right) => left.label.localeCompare(right.label));
  }, [rows]);
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
      credential: credentialLabel?.(cycle.credential_id) ?? `#${cycle.credential_id}`,
    })),
  [credentialLabel, filtered, metric]);

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
    <section className="grid gap-3" aria-label={t("quotaHistory.title")}>
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
          <div className={compact ? "h-44 rounded-md border bg-muted/20 p-2" : "h-64 rounded-md border bg-card p-2"}>
            <ResponsiveContainer width="100%" height="100%">
              <BarChart data={chartData} margin={{ top: 4, right: 8, bottom: 0, left: 8 }}>
                <CartesianGrid vertical={false} strokeDasharray="3 3" stroke="var(--border)" />
                <XAxis
                  dataKey="at"
                  tickFormatter={(value: number) => new Date(value * 1000).toLocaleDateString()}
                  tick={{ fontSize: 10 }}
                  stroke="var(--muted-foreground)"
                />
                <YAxis
                  tickFormatter={(value: number) => formatMetric(value, metric)}
                  tick={{ fontSize: 10 }}
                  stroke="var(--muted-foreground)"
                  width={64}
                />
                <Tooltip
                  contentStyle={{
                    background: "var(--popover)",
                    border: "1px solid var(--border)",
                    borderRadius: "0.5rem",
                    fontSize: 12,
                  }}
                  labelFormatter={(_, payload) => {
                    const point = payload[0]?.payload as { at?: number; label?: string; credential?: string } | undefined;
                    if (!point?.at) return "";
                    return `${point.credential ?? ""} · ${point.label ?? ""} · ${new Date(point.at * 1000).toLocaleString()}`;
                  }}
                  formatter={(value, _name, item) => {
                    const point = item.payload as { exactValue?: string } | undefined;
                    const formatted = metric === "cost" && point?.exactValue
                      ? formatUsageUsd(point.exactValue)
                      : formatMetric(Number(value), metric);
                    return [formatted, t(`quotaHistory.metric.${metric}`)];
                  }}
                />
                <Bar dataKey="value" fill="var(--chart-2)" radius={[3, 3, 0, 0]} isAnimationActive={false} />
              </BarChart>
            </ResponsiveContainer>
          </div>

          <div className="rounded-md border bg-card">
            <Table>
              <TableHeader>
                <TableRow>
                  {!credentialId && <TableHead>{t("quotaHistory.columns.credential")}</TableHead>}
                  <TableHead>{t("quotaHistory.columns.window")}</TableHead>
                  <TableHead>{t("quotaHistory.columns.period")}</TableHead>
                  <TableHead>{t("quotaHistory.columns.status")}</TableHead>
                  <TableHead className="text-right">{t("quotaHistory.columns.percent")}</TableHead>
                  <TableHead className="text-right">{t("quotaHistory.columns.tokens")}</TableHead>
                  <TableHead className="text-right">{t("quotaHistory.columns.cost")}</TableHead>
                  <TableHead className="text-right">{t("quotaHistory.columns.estimate")}</TableHead>
                  <TableHead className="w-10" />
                </TableRow>
              </TableHeader>
              <TableBody>
                {filtered.map((cycle) => (
                  <Fragment key={cycle.id}>
                    <TableRow>
                      {!credentialId && (
                        <TableCell className="font-medium">
                          {credentialLabel?.(cycle.credential_id) ?? `#${cycle.credential_id}`}
                        </TableCell>
                      )}
                      <TableCell>
                        <p>{cycleLabel(cycle)}</p>
                        <p className="font-mono text-[10px] text-muted-foreground">{cycle.window_key}</p>
                      </TableCell>
                      <TableCell className="text-xs text-muted-foreground">{formatCyclePeriod(cycle)}</TableCell>
                      <TableCell className="text-xs">
                        {t(`quotaHistory.status.${cycle.status}`, { defaultValue: cycle.status })}
                        <span className="block text-[10px] text-muted-foreground">
                          {t(`quotaHistory.coverage.${cycle.coverage}`, { defaultValue: cycle.coverage })}
                        </span>
                      </TableCell>
                      <TableCell className="text-right font-mono tabular-nums">
                        {cycle.used_percent !== null ? `${Number(cycle.used_percent).toFixed(1)}%` : "—"}
                      </TableCell>
                      <TableCell className="text-right font-mono tabular-nums">{formatUsageCount(categorizedTotalTokens(cycle))}</TableCell>
                      <TableCell className="text-right font-mono tabular-nums">{formatUsageUsd(cycle.cost)}</TableCell>
                      <TableCell className="text-right text-xs text-muted-foreground">
                        {formatCycleEstimate(cycle, t("quotaHistory.tokens"))}
                      </TableCell>
                      <TableCell>
                        <Button
                          variant="ghost"
                          size="icon-sm"
                          aria-label={t("quotaHistory.models")}
                          aria-expanded={expandedCycle === cycle.id}
                          onClick={() => setExpandedCycle(expandedCycle === cycle.id ? null : cycle.id)}
                        >
                          <ChevronsUpDown className="size-3" />
                        </Button>
                      </TableCell>
                    </TableRow>
                    {expandedCycle === cycle.id && (
                      <TableRow>
                        <TableCell colSpan={credentialId ? 8 : 9} className="whitespace-normal bg-muted/20">
                          <CycleModels cycleId={cycle.id} />
                        </TableCell>
                      </TableRow>
                    )}
                  </Fragment>
                ))}
              </TableBody>
            </Table>
          </div>

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
