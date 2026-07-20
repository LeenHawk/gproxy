import { useQuery } from "@tanstack/react-query";
import { useTranslation } from "react-i18next";
import {
  credentialModelStatusesQuery,
  credentialStatusQuery,
  type CredentialStatus,
} from "@/api/credentials";
import { Badge } from "@/components/ui/badge";
import { Tooltip, TooltipContent, TooltipTrigger } from "@/components/ui/tooltip";
import {
  countCurrentUnhealthyModels,
  latestCurrentCredentialStatus,
  unixNow,
} from "@/lib/credential-health";

function fmtTime(unixSecs: number): string {
  return new Date(unixSecs * 1000).toLocaleString();
}

const KIND_STYLE: Record<string, string> = {
  recovered: "bg-emerald-500/15 text-emerald-800 dark:bg-emerald-400/15 dark:text-emerald-200",
  breaker: "bg-destructive/15 text-destructive dark:text-red-300",
  auth_dead: "bg-destructive/15 text-destructive dark:text-red-300",
  rate_limited: "bg-amber-500/15 text-amber-800 dark:bg-amber-400/15 dark:text-amber-200",
};

export function HealthBadge({ credentialId }: { credentialId: number }) {
  const { t } = useTranslation("providers");
  const { data } = useQuery(credentialStatusQuery(credentialId));
  const { data: modelData } = useQuery(credentialModelStatusesQuery(credentialId));
  const now = unixNow();
  const status = data ? latestCurrentCredentialStatus<CredentialStatus>(data, now) : undefined;
  const modelIssueCount = countCurrentUnhealthyModels(modelData ?? [], now);
  const until = status?.health_json?.open_until;
  const label = status
    ? t(`health.${status.health_kind}`, { defaultValue: status.health_kind })
    : t("health.unknown");
  return (
    <span className="flex flex-wrap items-center gap-1">
      <Tooltip>
        <TooltipTrigger asChild>
          <Badge
            variant="outline"
            className={status ? KIND_STYLE[status.health_kind] ?? "" : "text-muted-foreground"}
          >
            {label}
            {until ? ` · ${t("health.until", { time: fmtTime(until) })}` : ""}
          </Badge>
        </TooltipTrigger>
        <TooltipContent>
          <p>{status?.last_error ?? status?.health_json?.reason ?? label}</p>
          {status?.checked_at && <p>{t("health.asOf", { time: fmtTime(status.checked_at) })}</p>}
        </TooltipContent>
      </Tooltip>
      {modelIssueCount > 0 && (
        <Badge variant="outline" className="bg-amber-500/15 text-amber-800 dark:text-amber-200">
          {t("health.modelIssues", { count: modelIssueCount })}
        </Badge>
      )}
    </span>
  );
}
