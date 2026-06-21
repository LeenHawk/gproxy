import { useQuery } from "@tanstack/react-query";
import { useTranslation } from "react-i18next";
import { credentialStatusQuery, type CredentialStatus } from "@/api/credentials";
import { Badge } from "@/components/ui/badge";
import { Tooltip, TooltipContent, TooltipTrigger } from "@/components/ui/tooltip";

function fmtTime(unixSecs: number): string {
  return new Date(unixSecs * 1000).toLocaleString();
}

const KIND_STYLE: Record<string, string> = {
  recovered: "bg-emerald-500/15 text-emerald-800 dark:bg-emerald-400/15 dark:text-emerald-200",
  breaker: "bg-destructive/15 text-destructive dark:text-red-300",
  auth_dead: "bg-destructive/15 text-destructive dark:text-red-300",
  rate_limited: "bg-amber-500/15 text-amber-800 dark:bg-amber-400/15 dark:text-amber-200",
};

function latestStatus(rows: CredentialStatus[]): CredentialStatus | undefined {
  return [...rows].sort((a, b) => b.updated_at - a.updated_at)[0];
}

export function HealthBadge({ credentialId }: { credentialId: number }) {
  const { t } = useTranslation("providers");
  const { data } = useQuery(credentialStatusQuery(credentialId));
  const status = data ? latestStatus(data) : undefined;

  if (!status) {
    return <Badge variant="outline" className="text-muted-foreground">{t("health.unknown")}</Badge>;
  }
  const until = status.health_json?.open_until;
  const label = t(`health.${status.health_kind}`, { defaultValue: status.health_kind });
  return (
    <Tooltip>
      <TooltipTrigger asChild>
        <Badge variant="outline" className={KIND_STYLE[status.health_kind] ?? ""}>
          {label}
          {until ? ` · ${t("health.until", { time: fmtTime(until) })}` : ""}
        </Badge>
      </TooltipTrigger>
      <TooltipContent>
        <p>{status.last_error ?? status.health_json?.reason ?? label}</p>
        {status.checked_at && <p>{t("health.asOf", { time: fmtTime(status.checked_at) })}</p>}
      </TooltipContent>
    </Tooltip>
  );
}
