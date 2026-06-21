import { useQuery } from "@tanstack/react-query";
import { useTranslation } from "react-i18next";
import { credentialStatusesQuery, type CredentialStatus } from "@/api/usage";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { Skeleton } from "@/components/ui/skeleton";

type HealthKind = "recovered" | "breaker" | "rate_limited" | "auth_dead";

const KIND_CLASS: Record<string, string> = {
  recovered:
    "bg-emerald-500/15 text-emerald-800 ring-emerald-500/20 dark:bg-emerald-400/15 dark:text-emerald-200",
  breaker:
    "bg-destructive/15 text-destructive ring-destructive/20 dark:text-red-300",
  auth_dead:
    "bg-destructive/15 text-destructive ring-destructive/20 dark:text-red-300",
  rate_limited:
    "bg-amber-500/15 text-amber-800 ring-amber-500/20 dark:bg-amber-400/15 dark:text-amber-200",
};

const ALL_KINDS: HealthKind[] = ["recovered", "breaker", "auth_dead", "rate_limited"];

function countByKind(rows: CredentialStatus[]): Record<string, number> {
  const counts: Record<string, number> = {};
  for (const r of rows) {
    counts[r.health_kind] = (counts[r.health_kind] ?? 0) + 1;
  }
  return counts;
}

function fmtTime(unixSecs: number): string {
  return new Date(unixSecs * 1000).toLocaleString();
}

export function HealthPanel() {
  const { t } = useTranslation("observability");
  const { data, isPending, isError } = useQuery(credentialStatusesQuery);

  if (isPending) {
    return (
      <div aria-busy="true" className="grid grid-cols-2 gap-3 sm:grid-cols-4">
        {ALL_KINDS.map((k) => (
          <Skeleton key={k} className="h-20 rounded-xl" />
        ))}
      </div>
    );
  }

  if (isError) {
    return (
      <p className="text-sm text-destructive">{t("health.loadError")}</p>
    );
  }

  const rows = data ?? [];
  const counts = countByKind(rows);
  const unhealthy = rows.filter((r) => r.health_kind !== "recovered");
  const noEvents = rows.length === 0;
  const allHealthy = unhealthy.length === 0;

  return (
    <div className="space-y-4">
      {/* Stat cards */}
      <div className="grid grid-cols-2 gap-3 sm:grid-cols-4">
        {ALL_KINDS.map((kind) => {
          const count = counts[kind] ?? 0;
          const cls = KIND_CLASS[kind] ?? "";
          return (
            <div
              key={kind}
              className={`flex flex-col gap-1 rounded-xl px-4 py-3 ring-1 ${cls}`}
            >
              <span className="text-2xl font-semibold tabular-nums">{count}</span>
              <span className="text-xs font-medium">{t(`health.${kind}`)}</span>
            </div>
          );
        })}
      </div>

      {/* Summary / unhealthy list */}
      {allHealthy ? (
        <p className="text-sm text-muted-foreground">
          {noEvents ? t("health.noEvents") : t("health.allHealthy")}
        </p>
      ) : (
        <Card size="sm">
          <CardHeader>
            <CardTitle>{t("health.title")}</CardTitle>
          </CardHeader>
          <CardContent>
            <ul className="divide-y divide-border text-sm">
              {unhealthy.map((s) => (
                <li key={s.id} className="flex flex-col gap-0.5 py-2">
                  <div className="flex items-center justify-between gap-4">
                    <span className="font-mono text-xs text-muted-foreground">
                      credential:{s.credential_id} / {s.channel}
                    </span>
                    <span
                      className={`rounded-full px-2 py-0.5 text-xs font-medium ring-1 ${KIND_CLASS[s.health_kind] ?? ""}`}
                    >
                      {t(`health.${s.health_kind}`, { defaultValue: s.health_kind })}
                    </span>
                  </div>
                  {s.health_json?.reason && (
                    <span className="text-xs text-muted-foreground">{s.health_json.reason}</span>
                  )}
                  {s.last_error && (
                    <span className="text-xs text-muted-foreground">
                      {t("health.lastError")}: {s.last_error}
                    </span>
                  )}
                  {s.checked_at && (
                    <span className="text-xs text-muted-foreground/60">
                      {t("health.checkedAt", { time: fmtTime(s.checked_at) })}
                    </span>
                  )}
                </li>
              ))}
            </ul>
          </CardContent>
        </Card>
      )}
    </div>
  );
}
