import { Fragment, useEffect, useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { ChevronsUpDown } from "lucide-react";
import { useTranslation } from "react-i18next";
import {
  credentialQuotaCycleDetailQuery,
  type CredentialQuotaCycle,
} from "@/api/credentials";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Card, CardAction, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { Skeleton } from "@/components/ui/skeleton";
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@/components/ui/table";
import { Meter, seriesColor } from "@/components/observability/chart-theme";
import {
  categorizedTotalTokens,
  formatUsageCount,
  formatUsageUsd,
} from "@/lib/credential-usage";

interface CredentialQuotaCycleListProps {
  cycles: CredentialQuotaCycle[];
  credentialId?: number;
  credentialLabel?: (credentialId: number) => string;
  compact: boolean;
  windowSlots: Map<string, number>;
}

function formatStamp(unixSeconds: number): string {
  return new Date(unixSeconds * 1000).toLocaleString(undefined, {
    month: "short",
    day: "numeric",
    hour: "2-digit",
    minute: "2-digit",
  });
}

function formatPeriod(cycle: CredentialQuotaCycle): string {
  if (cycle.period_start !== null && cycle.period_end !== null) {
    return `${formatStamp(cycle.period_start)} – ${formatStamp(cycle.period_end)}`;
  }
  if (cycle.period_start !== null) return `${formatStamp(cycle.period_start)} – …`;
  if (cycle.period_end !== null) return `… – ${formatStamp(cycle.period_end)}`;
  if (cycle.last_observed_at !== null) return formatStamp(cycle.last_observed_at);
  return "—";
}

function cycleLabel(cycle: CredentialQuotaCycle): string {
  return cycle.label?.trim() || cycle.name || cycle.window_key;
}

function formatEstimate(cycle: CredentialQuotaCycle, tokensLabel: string): string {
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

export function CredentialQuotaCycleList({
  cycles,
  credentialId,
  credentialLabel,
  compact,
  windowSlots,
}: CredentialQuotaCycleListProps) {
  const { t } = useTranslation("observability");
  const [expandedCycle, setExpandedCycle] = useState<number | null>(null);

  useEffect(() => {
    if (expandedCycle !== null && !cycles.some((cycle) => cycle.id === expandedCycle)) {
      setExpandedCycle(null);
    }
  }, [cycles, expandedCycle]);

  const toggleModels = (cycleId: number) => {
    setExpandedCycle((current) => current === cycleId ? null : cycleId);
  };

  return (
    <>
      <div className="grid gap-2 lg:hidden">
        {cycles.map((cycle) => (
          <Card key={cycle.id} size="sm">
            <CardHeader className="border-b">
              <CardTitle className="min-w-0">
                <span className="flex items-center gap-1.5">
                  <span
                    aria-hidden
                    className="size-2 shrink-0 rounded-full"
                    style={{ background: seriesColor(windowSlots.get(cycle.window_key) ?? 0) }}
                  />
                  <span className="truncate">{cycleLabel(cycle)}</span>
                </span>
                <span className="mt-0.5 block truncate pl-3.5 font-mono text-[10px] font-normal text-muted-foreground">
                  {cycle.window_key}
                </span>
              </CardTitle>
              <CardAction>
                <Badge variant={cycle.status === "open" ? "secondary" : "outline"}>
                  {t(`quotaHistory.status.${cycle.status}`, { defaultValue: cycle.status })}
                </Badge>
              </CardAction>
            </CardHeader>
            <CardContent className="grid grid-cols-2 gap-x-4 gap-y-3">
              {!credentialId ? (
                <div className="col-span-2 grid gap-0.5">
                  <span className="text-xs text-muted-foreground">{t("quotaHistory.columns.credential")}</span>
                  <span className="font-medium">{credentialLabel?.(cycle.credential_id) ?? `#${cycle.credential_id}`}</span>
                </div>
              ) : null}
              <div className="col-span-2 grid gap-0.5">
                <span className="text-xs text-muted-foreground">{t("quotaHistory.columns.period")}</span>
                <span className="text-xs tabular-nums">{formatPeriod(cycle)}</span>
              </div>
              <div className="grid gap-1">
                <span className="text-xs text-muted-foreground">{t("quotaHistory.columns.percent")}</span>
                {cycle.used_percent !== null ? (
                  <span className="grid gap-1">
                    <span className="font-medium tabular-nums">{Number(cycle.used_percent).toFixed(1)}%</span>
                    <Meter percent={Number(cycle.used_percent)} />
                  </span>
                ) : "—"}
              </div>
              <div className="grid gap-0.5 text-right">
                <span className="text-xs text-muted-foreground">{t("quotaHistory.columns.status")}</span>
                <span className="text-xs">{t(`quotaHistory.coverage.${cycle.coverage}`, { defaultValue: cycle.coverage })}</span>
              </div>
              <div className="grid gap-0.5">
                <span className="text-xs text-muted-foreground">{t("quotaHistory.columns.tokens")}</span>
                <span className="font-mono tabular-nums">{formatUsageCount(categorizedTotalTokens(cycle))}</span>
              </div>
              <div className="grid gap-0.5 text-right">
                <span className="text-xs text-muted-foreground">{t("quotaHistory.columns.cost")}</span>
                <span className="font-mono tabular-nums">{formatUsageUsd(cycle.cost)}</span>
              </div>
              <Button
                variant="ghost"
                size="sm"
                className="col-span-2 justify-self-start"
                aria-expanded={expandedCycle === cycle.id}
                onClick={() => toggleModels(cycle.id)}
              >
                <ChevronsUpDown data-icon="inline-start" />
                {t("quotaHistory.models")}
              </Button>
              {expandedCycle === cycle.id ? (
                <div className="col-span-2 border-t pt-2">
                  <CycleModels cycleId={cycle.id} />
                </div>
              ) : null}
            </CardContent>
          </Card>
        ))}
      </div>

      <div className="hidden rounded-lg border bg-card lg:block">
        <Table className="table-fixed">
          <TableHeader>
            <TableRow>
              {!credentialId && <TableHead className="w-[14%]">{t("quotaHistory.columns.credential")}</TableHead>}
              <TableHead className={credentialId ? "w-[22%]" : "w-[18%]"}>{t("quotaHistory.columns.window")}</TableHead>
              <TableHead className="w-[24%]">{t("quotaHistory.columns.period")}</TableHead>
              <TableHead className="w-[12%]">{t("quotaHistory.columns.status")}</TableHead>
              <TableHead className="w-[13%] text-right">{t("quotaHistory.columns.percent")}</TableHead>
              <TableHead className="w-[12%] text-right">{t("quotaHistory.columns.tokens")}</TableHead>
              <TableHead className="w-[10%] text-right">{t("quotaHistory.columns.cost")}</TableHead>
              {!compact && <TableHead className="w-[14%] text-right">{t("quotaHistory.columns.estimate")}</TableHead>}
              <TableHead className="w-10" />
            </TableRow>
          </TableHeader>
          <TableBody>
            {cycles.map((cycle) => (
              <Fragment key={cycle.id}>
                <TableRow>
                  {!credentialId && (
                    <TableCell className="truncate font-medium">
                      {credentialLabel?.(cycle.credential_id) ?? `#${cycle.credential_id}`}
                    </TableCell>
                  )}
                  <TableCell className="whitespace-normal">
                    <span className="flex items-center gap-1.5">
                      <span aria-hidden className="size-2 shrink-0 rounded-full" style={{ background: seriesColor(windowSlots.get(cycle.window_key) ?? 0) }} />
                      <span className="truncate font-medium">{cycleLabel(cycle)}</span>
                    </span>
                    <span className="block truncate pl-3.5 font-mono text-[10px] text-muted-foreground">{cycle.window_key}</span>
                  </TableCell>
                  <TableCell className="text-xs tabular-nums text-muted-foreground">{formatPeriod(cycle)}</TableCell>
                  <TableCell>
                    <Badge variant={cycle.status === "open" ? "secondary" : "outline"}>{t(`quotaHistory.status.${cycle.status}`, { defaultValue: cycle.status })}</Badge>
                    <span className="mt-0.5 block truncate text-[10px] text-muted-foreground">{t(`quotaHistory.coverage.${cycle.coverage}`, { defaultValue: cycle.coverage })}</span>
                  </TableCell>
                  <TableCell className="text-right">
                    {cycle.used_percent !== null ? (
                      <span className="inline-flex w-full flex-col items-end gap-1">
                        <span className="font-medium tabular-nums">{Number(cycle.used_percent).toFixed(1)}%</span>
                        <Meter percent={Number(cycle.used_percent)} className="w-full max-w-20" />
                      </span>
                    ) : "—"}
                  </TableCell>
                  <TableCell className="text-right font-mono tabular-nums">{formatUsageCount(categorizedTotalTokens(cycle))}</TableCell>
                  <TableCell className="text-right font-mono tabular-nums">{formatUsageUsd(cycle.cost)}</TableCell>
                  {!compact && (
                    <TableCell className="whitespace-normal text-right text-xs text-muted-foreground">
                      {formatEstimate(cycle, t("quotaHistory.tokens"))}
                    </TableCell>
                  )}
                  <TableCell>
                    <Button variant="ghost" size="icon-sm" aria-label={t("quotaHistory.models")} aria-expanded={expandedCycle === cycle.id} onClick={() => toggleModels(cycle.id)}>
                      <ChevronsUpDown />
                    </Button>
                  </TableCell>
                </TableRow>
                {expandedCycle === cycle.id ? (
                  <TableRow>
                    <TableCell colSpan={(credentialId ? 7 : 8) + (compact ? 0 : 1)} className="whitespace-normal bg-muted/20">
                      <CycleModels cycleId={cycle.id} />
                    </TableCell>
                  </TableRow>
                ) : null}
              </Fragment>
            ))}
          </TableBody>
        </Table>
      </div>
    </>
  );
}
