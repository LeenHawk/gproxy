import { useEffect } from "react";
import { useMutation, useQuery } from "@tanstack/react-query";
import { ChevronsUpDown, RefreshCw, RotateCcw } from "lucide-react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import { consumeRateLimitResetCredit, credentialUsageQuery, type UsageWindow } from "@/api/credentials";
import { ApiError } from "@/api/http";
import { Button } from "@/components/ui/button";
import { Collapsible, CollapsibleContent, CollapsibleTrigger } from "@/components/ui/collapsible";
import { Progress } from "@/components/ui/progress";

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

function humanizeWindowName(name: string): string {
  return name
    .replace(/[_:.-]+/g, " ")
    .replace(/\b\w/g, (char) => char.toUpperCase());
}

function idempotencyKey(): string {
  if (typeof crypto !== "undefined" && "randomUUID" in crypto) {
    return crypto.randomUUID();
  }
  return `${Date.now()}-${Math.random().toString(36).slice(2)}`;
}

export function UsageCard({ credentialId }: { credentialId: number }) {
  const { t } = useTranslation("providers");
  const query = useQuery(credentialUsageQuery(credentialId));
  const snapshot = query.data;
  const { isFetched, isFetching, refetch } = query;
  const resetCredits = snapshot?.rate_limit_reset_credits;
  const resetMutation = useMutation({
    mutationFn: () => consumeRateLimitResetCredit(credentialId, idempotencyKey()),
    onSuccess: (result) => {
      toast.success(t(`usage.reset.outcome.${result.outcome}`));
      void refetch();
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
    if (isFetched || isFetching) return;
    void refetch();
  }, [credentialId, isFetched, isFetching, refetch]);

  return (
    <div className="grid gap-4">
      <div className="flex items-center justify-between gap-2">
        <p className="text-xs text-muted-foreground">{t("usage.sparingly")}</p>
        <Button size="sm" variant="outline" disabled={!hasResolved || isFetching} onClick={() => void refetch()}>
          <RefreshCw className={isFetching ? "size-4 animate-spin" : "size-4"} />
          {isFetching ? t("usage.fetching") : hasResolved ? t("usage.refresh") : t("usage.fetching")}
        </Button>
      </div>

      {errorText && <p className="text-sm text-destructive">{errorText}</p>}

      {snapshot && (
        <div className="grid gap-3">
          {snapshot.plan && (
            <p className="text-sm"><span className="text-muted-foreground">{t("usage.plan")}:</span> <span className="font-medium">{snapshot.plan}</span></p>
          )}
          {snapshot.windows.map((w) => {
            const pct = windowPercent(w);
            const reset = windowReset(w);
            const label = w.label
              ? w.name.startsWith("weekly_scoped:")
                ? t("usage.window.weekly_scoped", { scope: w.label })
                : t(`usage.window.${w.name}`, { scope: w.label, defaultValue: w.label })
              : t(`usage.window.${w.name}`, { defaultValue: humanizeWindowName(w.name) });
            return (
              <div key={w.name} className="grid gap-1">
                <div className="flex items-center justify-between text-sm">
                  <span>{label}</span>
                  <span className="text-xs text-muted-foreground">
                    {pct !== undefined ? `${pct.toFixed(0)}%` : w.used !== undefined ? `${w.used}${w.limit !== undefined ? ` / ${w.limit}` : ""}` : ""}
                    {reset ? ` · ${t("usage.resets", { time: reset })}` : ""}
                  </span>
                </div>
                {pct !== undefined && <Progress value={pct} />}
              </div>
            );
          })}
          {snapshot.credits && (
            <p className="text-sm">
              <span className="text-muted-foreground">{t("usage.credits")}:</span>{" "}
              {snapshot.credits.unlimited ? "∞"
                : snapshot.credits.balance ?? (snapshot.credits.used_credits !== undefined && snapshot.credits.monthly_limit !== undefined
                  ? `${snapshot.credits.used_credits} / ${snapshot.credits.monthly_limit}`
                : JSON.stringify(snapshot.credits))}
            </p>
          )}
          {resetCredits && (
            <div className="flex items-center justify-between gap-3 rounded-md border bg-muted/30 px-3 py-2">
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
              <Button variant="ghost" size="sm" className="text-muted-foreground">
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
  );
}
