import { useQuery } from "@tanstack/react-query";
import { ChevronsUpDown, RefreshCw } from "lucide-react";
import { useTranslation } from "react-i18next";
import { credentialUsageQuery, type UsageWindow } from "@/api/credentials";
import { ApiError } from "@/api/http";
import { Button } from "@/components/ui/button";
import { Collapsible, CollapsibleContent, CollapsibleTrigger } from "@/components/ui/collapsible";
import { Progress } from "@/components/ui/progress";

function windowPercent(w: UsageWindow): number | undefined {
  if (w.used_percent !== undefined) return Math.min(100, Math.max(0, w.used_percent));
  if (w.used !== undefined && w.limit !== undefined && w.limit > 0) {
    return Math.min(100, Math.max(0, (w.used / w.limit) * 100));
  }
  return undefined;
}

function windowReset(w: UsageWindow): string | undefined {
  if (w.resets_at_unix !== undefined) return new Date(w.resets_at_unix * 1000).toLocaleString();
  return w.resets_at;
}

export function UsageCard({ credentialId }: { credentialId: number }) {
  const { t } = useTranslation("providers");
  const query = useQuery(credentialUsageQuery(credentialId));
  const snapshot = query.data;

  return (
    <div className="grid gap-4">
      <div className="flex items-center justify-between gap-2">
        <p className="text-xs text-muted-foreground">{t("usage.sparingly")}</p>
        <Button size="sm" variant="outline" disabled={query.isFetching} onClick={() => void query.refetch()}>
          <RefreshCw className={query.isFetching ? "size-4 animate-spin" : "size-4"} />
          {snapshot ? t("usage.refresh") : t("usage.fetch")}
        </Button>
      </div>

      {query.isError && (
        <p className="text-sm text-destructive">
          {query.error instanceof ApiError && query.error.status === 400
            ? t("usage.unsupported")
            : query.error instanceof ApiError ? query.error.message : String(query.error)}
        </p>
      )}

      {snapshot && (
        <div className="grid gap-3">
          {snapshot.plan && (
            <p className="text-sm"><span className="text-muted-foreground">{t("usage.plan")}:</span> <span className="font-medium">{snapshot.plan}</span></p>
          )}
          {snapshot.windows.map((w) => {
            const pct = windowPercent(w);
            const reset = windowReset(w);
            return (
              <div key={w.name} className="grid gap-1">
                <div className="flex items-center justify-between text-sm">
                  <span>{t(`usage.window.${w.name}`, { defaultValue: w.name })}</span>
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
