import { useTranslation } from "react-i18next";
import type { CredentialModelStatus, CredentialStatus } from "@/api/usage";
import { isCurrentCredentialStatus, unixNow } from "@/lib/credential-health";
import { cn } from "@/lib/utils";

export type HealthLevel = "healthy" | "warning" | "danger";

function statusLevel(status: CredentialStatus, now: number): HealthLevel {
  if (!isCurrentCredentialStatus(status, now) || status.health_kind === "recovered") {
    return "healthy";
  }
  return status.health_kind === "rate_limited" ? "warning" : "danger";
}

export function providerHealthLevels(
  statuses: CredentialStatus[],
  modelStatuses: CredentialModelStatus[],
  now = unixNow(),
): Map<number, HealthLevel> {
  const levels = new Map<number, HealthLevel>();
  for (const status of [...statuses, ...modelStatuses]) {
    if (status.provider_id === null) continue;
    const next = statusLevel(status, now);
    const current = levels.get(status.provider_id) ?? "healthy";
    if (current !== "danger" && next !== "healthy") levels.set(status.provider_id, next);
  }
  return levels;
}

export function ProviderHealthBadge({ level }: { level: HealthLevel }) {
  const { t } = useTranslation("providers");
  const label = level === "healthy"
    ? t("summary.healthy")
    : level === "warning" ? t("summary.degraded") : t("summary.unhealthy");
  return (
    <span
      className={cn(
        "shrink-0 rounded-sm border px-1.5 py-0 text-[10px] font-medium leading-4",
        level === "healthy" && "border-emerald-500/30 bg-emerald-500/10 text-emerald-700 dark:text-emerald-300",
        level === "warning" && "border-amber-500/30 bg-amber-500/10 text-amber-700 dark:text-amber-300",
        level === "danger" && "border-red-500/30 bg-red-500/10 text-red-700 dark:text-red-300",
      )}
    >
      {label}
    </span>
  );
}

export function ProviderSummary({
  channel,
  credentialCount,
  level,
}: {
  channel: string;
  credentialCount: number;
  level: HealthLevel;
}) {
  const { t } = useTranslation("providers");
  return (
    <span className="flex min-w-0 items-center gap-1.5 text-xs text-muted-foreground">
      <span className="truncate font-mono">{channel}</span>
      <span aria-hidden>·</span>
      <span className="shrink-0">{t("summary.credentialCount", { count: credentialCount })}</span>
      <span aria-hidden>·</span>
      <ProviderHealthBadge level={level} />
    </span>
  );
}
