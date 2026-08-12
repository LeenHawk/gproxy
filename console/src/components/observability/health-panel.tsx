import { useQuery } from "@tanstack/react-query";
import { useTranslation } from "react-i18next";
import {
  credentialStatusesQuery,
  type CredentialStatus,
} from "@/api/usage";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { Skeleton } from "@/components/ui/skeleton";
import { currentCredentialStatuses, unixNow } from "@/lib/credential-health";

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
  for (const row of rows) counts[row.health_kind] = (counts[row.health_kind] ?? 0) + 1;
  return counts;
}

function fmtTime(unixSecs: number): string {
  return new Date(unixSecs * 1000).toLocaleString();
}

function GlobalHealth({ rawRows, now }: { rawRows: CredentialStatus[]; now: number }) {
  const { t } = useTranslation("observability");
  const rows = currentCredentialStatuses(rawRows, now);
  const counts = countByKind(rows);
  const unhealthy = rows.filter((row) => row.health_kind !== "recovered");

  return (
    <section className="flex flex-col gap-3" aria-labelledby="credential-health-global">
      <div>
        <h3 id="credential-health-global" className="text-sm font-semibold">
          {t("health.globalScope")}
        </h3>
        <p className="text-xs text-muted-foreground">{t("health.globalDescription")}</p>
      </div>

      <div className="grid grid-cols-2 gap-3 sm:grid-cols-4">
        {ALL_KINDS.map((kind) => (
          <div
            key={kind}
            className={`flex flex-col gap-1 rounded-xl px-4 py-3 ring-1 ${KIND_CLASS[kind]}`}
          >
            <span className="text-2xl font-semibold tabular-nums">{counts[kind] ?? 0}</span>
            <span className="text-xs font-medium">{t(`health.${kind}`)}</span>
          </div>
        ))}
      </div>

      {unhealthy.length === 0 ? (
        <p className="text-sm text-muted-foreground">
          {t(rawRows.length === 0 ? "health.globalNoEvents" : "health.globalAllHealthy")}
        </p>
      ) : (
        <Card size="sm">
          <CardHeader>
            <CardTitle>{t("health.globalIssues")}</CardTitle>
          </CardHeader>
          <CardContent>
            <ul className="divide-y divide-border text-sm">
              {unhealthy.map((status) => (
                <li key={status.id} className="flex flex-col gap-0.5 py-2">
                  <div className="flex items-center justify-between gap-4">
                    <span className="font-mono text-xs text-muted-foreground">
                      credential:{status.credential_id} / {status.channel}
                    </span>
                    <span
                      className={`rounded-full px-2 py-0.5 text-xs font-medium ring-1 ${KIND_CLASS[status.health_kind] ?? ""}`}
                    >
                      {t(`health.${status.health_kind}`, { defaultValue: status.health_kind })}
                    </span>
                  </div>
                  {status.health_json?.reason && (
                    <span className="text-xs text-muted-foreground">
                      {status.health_json.reason}
                    </span>
                  )}
                  {status.last_error && (
                    <span className="text-xs text-muted-foreground">
                      {t("health.lastError")}: {status.last_error}
                    </span>
                  )}
                  {status.checked_at && (
                    <span className="text-xs text-muted-foreground/60">
                      {t("health.checkedAt", { time: fmtTime(status.checked_at) })}
                    </span>
                  )}
                </li>
              ))}
            </ul>
          </CardContent>
        </Card>
      )}
    </section>
  );
}

export function HealthPanel() {
  const { t } = useTranslation("observability");
  const global = useQuery(credentialStatusesQuery);

  if (global.isPending) {
    return (
      <div aria-busy="true" className="grid grid-cols-2 gap-3 sm:grid-cols-4">
        {ALL_KINDS.map((kind) => <Skeleton key={kind} className="h-20 rounded-xl" />)}
      </div>
    );
  }

  if (global.isError) {
    return <p className="text-sm text-destructive">{t("health.loadError")}</p>;
  }

  return <GlobalHealth rawRows={global.data ?? []} now={unixNow()} />;
}
