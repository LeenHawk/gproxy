import { useTranslation } from "react-i18next";
import type { CredentialModelStatus, CredentialStatus } from "@/api/usage";
import { Tooltip, TooltipContent, TooltipTrigger } from "@/components/ui/tooltip";
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

export function ProviderHealthDot({ level }: { level: HealthLevel }) {
  const { t } = useTranslation("providers");
  const label = level === "healthy"
    ? t("summary.healthy")
    : level === "warning" ? t("summary.degraded") : t("summary.unhealthy");
  return (
    <Tooltip>
      <TooltipTrigger asChild>
        <span
          role="img"
          aria-label={label}
          className={cn(
            "inline-block size-2 shrink-0 rounded-full",
            level === "healthy" && "bg-emerald-500",
            level === "warning" && "bg-amber-500",
            level === "danger" && "bg-red-500",
          )}
        />
      </TooltipTrigger>
      <TooltipContent>{label}</TooltipContent>
    </Tooltip>
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
      <ProviderHealthDot level={level} />
    </span>
  );
}
