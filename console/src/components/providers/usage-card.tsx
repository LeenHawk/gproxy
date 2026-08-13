import { useCallback, useEffect } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { ChevronsUpDown, RefreshCw, RotateCcw } from "lucide-react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import {
  consumeRateLimitResetCredit,
  credentialUsageQuery,
  type UsageCredits,
  type UsageWindow,
} from "@/api/credentials";
import { ApiError } from "@/api/http";
import { Button } from "@/components/ui/button";
import { Collapsible, CollapsibleContent, CollapsibleTrigger } from "@/components/ui/collapsible";
import { Meter } from "@/components/observability/chart-theme";
import {
  CredentialUsageSummaryCard,
  modelUsageBreakdown,
} from "@/components/providers/credential-usage-summary";
import { CredentialQuotaHistory } from "@/components/observability/credential-quota-history";
import { formatUsageCount, formatUsageUsd } from "@/lib/credential-usage";

const UNSUPPORTED_USAGE_MESSAGE = "channel exposes no usage endpoint";

function windowPercent(w: UsageWindow): number | undefined {
  if (w.used_percent !== undefined) return Math.min(100, Math.max(0, w.used_percent));
  if (w.used !== undefined && w.limit !== undefined && w.limit > 0) {
    return Math.min(100, Math.max(0, (w.used / w.limit) * 100));
  }
  return undefined;
}

function windowReset(w: UsageWindow): string | undefined {
  if (w.resets_at_unix !== undefined) return new Date(w.resets_at_unix * 1000).toLocaleString();
  if (!w.resets_at) return undefined;

  const resetAt = new Date(w.resets_at);
  if (Number.isNaN(resetAt.getTime())) return w.resets_at;
  return resetAt.toLocaleString();
}

function formatCredits(credits: UsageCredits, disabled: string, unlimited: string): string {
  const used = credits.used_credits;
  const limit = credits.monthly_limit;
  if (credits.has_credits === false && used === undefined && limit === undefined && !credits.balance) return disabled;
  if (credits.unlimited) return unlimited;
  if (used === undefined || limit === undefined) {
    if (credits.balance) return credits.balance;
    return JSON.stringify(credits);
  }
  const format = (value: number) => {
    if (!credits.currency) return String(value);
    try {
      return new Intl.NumberFormat(undefined, {
        style: "currency",
        currency: credits.currency,
      }).format(value);
    } catch {
      return `${value} ${credits.currency}`;
    }
  };
  return `${format(used)} / ${format(limit)}`;
}

function formatLocalTotal(totals: { total_tokens: number; cost_usd: string }, tokens: string): string {
  return `${formatUsageCount(totals.total_tokens)} ${tokens} · ${formatUsageUsd(totals.cost_usd)}`;
}

function humanizeWindowName(name: string): string {
  return name
    .replace(/[_:.-]+/g, " ")
    .replace(/\b\w/g, (char) => char.toUpperCase());
}

function windowLabel(w: UsageWindow, t: (key: string, options?: Record<string, unknown>) => string): string {
  if (!w.label) return t(`usage.window.${w.name}`, { defaultValue: humanizeWindowName(w.name) });
  if (w.name.startsWith("weekly_scoped:") || w.name.startsWith("weekly_model:") || w.name.startsWith("weekly_surface:")) {
    return t("usage.window.weekly_scoped", { scope: w.label });
  }
  if (w.name.startsWith("additional_primary:")) return t("usage.window.additional_primary", { scope: w.label });
  if (w.name.startsWith("additional_secondary:")) return t("usage.window.additional_secondary", { scope: w.label });
  return t(`usage.window.${w.name}`, { scope: w.label, defaultValue: w.label });
}

function idempotencyKey(): string {
  if (typeof crypto !== "undefined" && "randomUUID" in crypto) {
    return crypto.randomUUID();
  }
  return `${Date.now()}-${Math.random().toString(36).slice(2)}`;
}

function QuotaWindowCard({ window: w }: { window: UsageWindow }) {
  const { t } = useTranslation("providers");
  const pct = windowPercent(w);
  const reset = windowReset(w);
  const localUsage = w.local_usage;
  const estimated = localUsage?.estimated_capacity;
  const estimatedParts = estimated
    ? [
        estimated.tokens !== undefined
          ? `≈${formatUsageCount(estimated.tokens)} ${t("usage.local.tokens")}`
          : undefined,
        estimated.cost_usd !== undefined ? `≈${formatUsageUsd(estimated.cost_usd)}` : undefined,
      ].filter((part): part is string => part !== undefined)
    : [];

  return (
    <div className="grid min-w-0 gap-1.5 rounded-lg border bg-card px-3 py-2.5">
      <div className="flex flex-wrap items-baseline justify-between gap-x-3 gap-y-0.5">
        <span className="text-sm font-medium">{windowLabel(w, t)}</span>
        <span className="text-sm font-semibold tabular-nums">
          {pct !== undefined
            ? `${pct.toFixed(0)}%`
            : w.used !== undefined
              ? `${w.used}${w.limit !== undefined ? ` / ${w.limit}` : ""}`
              : "—"}
        </span>
      </div>
      {pct !== undefined && <Meter percent={pct} />}
      {reset && (
        <p className="text-xs text-muted-foreground">{t("usage.resets", { time: reset })}</p>
      )}

      {localUsage && (
        <div className="mt-0.5 grid gap-1 border-t pt-1.5 text-xs">
          <div className="flex flex-wrap items-baseline justify-between gap-x-3 gap-y-0.5">
            <span className="text-muted-foreground">
              {t("usage.local.recorded")} · {t(`usage.local.coverage.${localUsage.coverage}`)}
            </span>
            <span className="font-medium tabular-nums">
              {formatLocalTotal(localUsage.totals, t("usage.local.tokens"))}
            </span>
          </div>
          {estimatedParts.length > 0 && (
            <div className="flex flex-wrap items-baseline justify-between gap-x-3 gap-y-0.5 text-muted-foreground">
              <span>{t("usage.local.estimatedCurrentMix")}</span>
              <span className="tabular-nums">
                {estimatedParts.join(" · ")} · {t("usage.local.perWindow")}
              </span>
            </div>
          )}
          {localUsage.by_model.length > 0 && (
            <Collapsible>
              <CollapsibleTrigger asChild>
                <Button variant="ghost" size="sm" className="h-7 px-1 text-xs text-muted-foreground">
                  <ChevronsUpDown className="size-3" />
                  {t("usage.local.byModel", { count: localUsage.by_model.length })}
                </Button>
              </CollapsibleTrigger>
              <CollapsibleContent className="grid gap-1 border-t pt-1.5">
                {localUsage.by_model.map((model, index) => (
                  <div key={`${model.model}-${index}`} className="grid gap-0.5 py-1 text-xs">
                    <div className="flex flex-wrap items-baseline justify-between gap-x-3 gap-y-1">
                      <span className="break-all font-mono font-medium">{model.model}</span>
                      <span className="tabular-nums">{formatLocalTotal(model, t("usage.local.tokens"))}</span>
                    </div>
                    <span className="text-muted-foreground">{modelUsageBreakdown(model, t)}</span>
                  </div>
                ))}
              </CollapsibleContent>
            </Collapsible>
          )}
        </div>
      )}
    </div>
  );
}

export function UsageCard({
  credentialId,
  supportsUpstreamUsage = false,
}: {
  credentialId: number;
  supportsUpstreamUsage?: boolean;
}) {
  const { t } = useTranslation("providers");
  const queryClient = useQueryClient();
  const query = useQuery(credentialUsageQuery(credentialId));
  const snapshot = query.data;
  const { isFetched, isFetching, refetch } = query;
  const resetCredits = snapshot?.rate_limit_reset_credits;
  const refreshUpstream = useCallback(async () => {
    const result = await refetch();
    if (result.isSuccess) {
      await queryClient.invalidateQueries({ queryKey: ["credential-quota-cycles"] });
    }
  }, [queryClient, refetch]);
  const resetMutation = useMutation({
    mutationFn: () => consumeRateLimitResetCredit(credentialId, idempotencyKey()),
    onSuccess: (result) => {
      toast.success(t(`usage.reset.outcome.${result.outcome}`));
      void refreshUpstream();
    },
    onError: (error) => {
      toast.error(error instanceof ApiError ? error.message : String(error));
    },
  });
  const hasResolved = isFetched || query.isError;
  const errorText = query.isError
    ? query.error instanceof ApiError
      ? query.error.message === UNSUPPORTED_USAGE_MESSAGE
        ? t("usage.unsupported")
        : query.error.message
      : String(query.error)
    : undefined;

  useEffect(() => {
    if (!supportsUpstreamUsage || isFetched || isFetching) return;
    void refreshUpstream();
  }, [credentialId, isFetched, isFetching, refreshUpstream, supportsUpstreamUsage]);

  return (
    <div className="grid min-w-0 gap-4">
      {supportsUpstreamUsage && (
        <div className="grid min-w-0 gap-3">
          <div className="flex items-center justify-between gap-2">
            <div>
              <p className="text-sm font-medium">{t("usage.upstreamTitle")}</p>
              <p className="text-xs text-muted-foreground">{t("usage.sparingly")}</p>
            </div>
            <Button size="sm" variant="outline" disabled={!hasResolved || isFetching} onClick={() => void refreshUpstream()}>
              <RefreshCw className={isFetching ? "size-4 animate-spin" : "size-4"} />
              {isFetching ? t("usage.fetching") : hasResolved ? t("usage.refresh") : t("usage.fetching")}
            </Button>
          </div>

          {errorText && <p className="text-sm text-destructive">{errorText}</p>}

          {snapshot && (
            <div className="grid min-w-0 gap-2">
              {snapshot.plan && (
                <p className="text-sm">
                  <span className="text-muted-foreground">{t("usage.plan")}:</span>{" "}
                  <span className="font-medium">{snapshot.plan}</span>
                </p>
              )}
              {snapshot.windows.map((w) => <QuotaWindowCard key={w.name} window={w} />)}
              {snapshot.credits && (
                <p className="text-sm">
                  <span className="text-muted-foreground">{t("usage.credits")}:</span>{" "}
                  {formatCredits(snapshot.credits, t("usage.creditsDisabled"), t("usage.unlimited"))}
                </p>
              )}
              {resetCredits && (
                <div className="flex items-center justify-between gap-3 rounded-lg border bg-card px-3 py-2">
                  <p className="text-sm">
                    <span className="text-muted-foreground">{t("usage.reset.available")}:</span>{" "}
                    <span className="font-medium">{resetCredits.available_count}</span>
                  </p>
                  <Button
                    size="sm"
                    variant="outline"
                    disabled={resetMutation.isPending || resetCredits.available_count <= 0}
                    onClick={() => resetMutation.mutate()}
                  >
                    <RotateCcw className={resetMutation.isPending ? "size-4 animate-spin" : "size-4"} />
                    {resetMutation.isPending ? t("usage.reset.consuming") : t("usage.reset.consume")}
                  </Button>
                </div>
              )}
              <Collapsible>
                <CollapsibleTrigger asChild>
                  <Button variant="ghost" size="sm" className="justify-self-start text-muted-foreground">
                    <ChevronsUpDown className="size-3" />
                    {t("usage.raw")}
                  </Button>
                </CollapsibleTrigger>
                <CollapsibleContent>
                  <pre className="max-h-64 overflow-auto rounded-md bg-muted p-3 text-xs">
                    {JSON.stringify(snapshot.raw, null, 2)}
                  </pre>
                </CollapsibleContent>
              </Collapsible>
            </div>
          )}
        </div>
      )}

      <CredentialUsageSummaryCard credentialId={credentialId} />

      <CredentialQuotaHistory
        credentialId={credentialId}
        compact
        hideWhenEmpty={!supportsUpstreamUsage}
      />
    </div>
  );
}
