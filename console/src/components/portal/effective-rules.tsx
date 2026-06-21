import { useTranslation } from "react-i18next";
import { Badge } from "@/components/ui/badge";
import { Progress } from "@/components/ui/progress";
import { Skeleton } from "@/components/ui/skeleton";
import type { EffQuota, EffRateLimit, EffPermission } from "@/api/portal";

// Source badge: color + text (dual-channel a11y)
const SOURCE_CLASS: Record<string, string> = {
  user: "border-blue-500/50 text-blue-700 dark:text-blue-400",
  team: "border-purple-500/50 text-purple-700 dark:text-purple-400",
  org: "border-amber-500/50 text-amber-700 dark:text-amber-400",
};

function SourceBadge({ source, label }: { source: string; label: string }) {
  return (
    <Badge variant="outline" className={SOURCE_CLASS[source] ?? ""}>
      {label}
    </Badge>
  );
}

// ── Quota ──────────────────────────────────────────────────────────────
interface QuotaSectionProps { data: EffQuota[] | undefined; isPending: boolean; }

export function EffectiveQuotaSection({ data, isPending }: QuotaSectionProps) {
  const { t } = useTranslation("portal");

  if (isPending) {
    return (
      <section aria-busy="true" className="grid gap-3">
        <Skeleton className="h-4 w-32" />
        <Skeleton className="h-8 w-full" />
      </section>
    );
  }

  const rows = data ?? [];
  if (rows.length === 0) {
    return <p className="text-sm text-muted-foreground">{t("limits.noQuota")}</p>;
  }

  return (
    <ul className="grid gap-3">
      {rows.map((q) => {
        const total = parseFloat(q.quota_total);
        const used = parseFloat(q.cost_used);
        const pct = total > 0 ? Math.min(1, Math.max(0, used / total)) * 100 : 0;
        return (
          <li key={`${q.source}-${q.id}`} className="grid gap-1.5">
            <div className="flex items-center justify-between text-sm">
              <span>
                <span className="text-muted-foreground">{t("limits.costUsed")}: </span>
                <span className="font-mono">{q.cost_used}</span>
                {" / "}
                <span className="font-mono">{q.quota_total}</span>
              </span>
              <SourceBadge source={q.source} label={t(`limits.source.${q.source}`)} />
            </div>
            <Progress value={total > 0 ? pct : 0} aria-label={`${pct.toFixed(0)}%`} />
          </li>
        );
      })}
    </ul>
  );
}

// ── Rate Limits ────────────────────────────────────────────────────────
interface RateLimitsSectionProps { data: EffRateLimit[] | undefined; isPending: boolean; }

export function EffectiveRateLimitsSection({ data, isPending }: RateLimitsSectionProps) {
  const { t } = useTranslation("portal");

  if (isPending) {
    return (
      <section aria-busy="true" className="grid gap-2">
        <Skeleton className="h-4 w-40" />
        <Skeleton className="h-8 w-full" />
      </section>
    );
  }

  const rows = data ?? [];
  if (rows.length === 0) {
    return <p className="text-sm text-muted-foreground">{t("limits.noRateLimits")}</p>;
  }

  return (
    <ul className="grid gap-1">
      {rows.map((r) => (
        <li key={`${r.source}-${r.id}`}
          className="flex flex-wrap items-center justify-between gap-2 rounded-md border px-3 py-1.5 text-sm"
        >
          <span className="font-mono">{r.route_pattern}</span>
          <span className="flex flex-wrap items-center gap-3 text-muted-foreground">
            <span>{t("limits.rpm")} {r.rpm ?? "—"}</span>
            <span>{t("limits.rpd")} {r.rpd ?? "—"}</span>
            <span>{t("limits.totalTokens")} {r.total_tokens ?? "—"}</span>
            <SourceBadge source={r.source} label={t(`limits.source.${r.source}`)} />
          </span>
        </li>
      ))}
    </ul>
  );
}

// ── Permissions ────────────────────────────────────────────────────────
interface PermissionsSectionProps { data: EffPermission[] | undefined; isPending: boolean; }

export function EffectivePermissionsSection({ data, isPending }: PermissionsSectionProps) {
  const { t } = useTranslation("portal");

  if (isPending) {
    return (
      <section aria-busy="true" className="grid gap-2">
        <Skeleton className="h-4 w-36" />
        <Skeleton className="h-8 w-full" />
      </section>
    );
  }

  const rows = data ?? [];
  if (rows.length === 0) {
    return <p className="text-sm text-muted-foreground">{t("limits.noPermissions")}</p>;
  }

  return (
    <ul className="grid gap-1">
      {rows.map((p) => (
        <li key={`${p.source}-${p.id}`}
          className="flex items-center justify-between rounded-md border px-3 py-1.5"
        >
          <span className="font-mono text-sm">{p.route_pattern}</span>
          <SourceBadge source={p.source} label={t(`limits.source.${p.source}`)} />
        </li>
      ))}
    </ul>
  );
}
