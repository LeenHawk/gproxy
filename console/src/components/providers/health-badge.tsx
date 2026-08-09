import { useQuery } from "@tanstack/react-query";
import { useTranslation } from "react-i18next";
import {
  credentialModelStatusesQuery,
  credentialStatusQuery,
  type CredentialModelStatus,
  type CredentialStatus,
} from "@/api/credentials";
import { useCredentialHealthReset } from "@/components/providers/use-credential-health-reset";
import { Badge } from "@/components/ui/badge";
import { Tooltip, TooltipContent, TooltipTrigger } from "@/components/ui/tooltip";
import {
  currentUnhealthyModels,
  latestCurrentCredentialStatus,
  unixNow,
} from "@/lib/credential-health";

/** Longer lists are truncated — a tooltip taller than the row is unreadable. */
const MAX_TOOLTIP_MODELS = 8;

function fmtTime(unixSecs: number): string {
  return new Date(unixSecs * 1000).toLocaleString();
}

const KIND_STYLE: Record<string, string> = {
  recovered: "bg-emerald-500/15 text-emerald-800 dark:bg-emerald-400/15 dark:text-emerald-200",
  breaker: "bg-destructive/15 text-destructive dark:text-red-300",
  auth_dead: "bg-destructive/15 text-destructive dark:text-red-300",
  rate_limited: "bg-amber-500/15 text-amber-800 dark:bg-amber-400/15 dark:text-amber-200",
};

const TOOLTIP_CLASS = "max-w-sm flex-col items-start gap-1";

function ModelIssue({ row }: { row: CredentialModelStatus }) {
  const { t } = useTranslation("providers");
  const until = row.health_json?.open_until;
  const detail = row.health_json?.reason ?? row.last_error;
  return (
    <span className="flex flex-col">
      <span>
        <span className="font-mono">{row.model_id}</span>
        {" · "}
        {t(`health.${row.health_kind}`, { defaultValue: row.health_kind })}
        {until ? ` · ${t("health.until", { time: fmtTime(until) })}` : ""}
      </span>
      {detail && <span className="opacity-70">{detail}</span>}
    </span>
  );
}

/** Health is per-instance soft state, so clearing is safe: worst case the next
 *  attempt fails and re-trips the breaker. No confirmation step. */
export function HealthBadge({ credentialId }: { credentialId: number }) {
  const { t } = useTranslation("providers");
  const { data } = useQuery(credentialStatusQuery(credentialId));
  const { data: modelData } = useQuery(credentialModelStatusesQuery(credentialId));
  const reset = useCredentialHealthReset(credentialId);
  const now = unixNow();
  const status = data ? latestCurrentCredentialStatus<CredentialStatus>(data, now) : undefined;
  const modelIssues = currentUnhealthyModels(modelData ?? [], now);
  const until = status?.health_json?.open_until;
  const label = status
    ? t(`health.${status.health_kind}`, { defaultValue: status.health_kind })
    : t("health.unknown");
  const text = `${label}${until ? ` · ${t("health.until", { time: fmtTime(until) })}` : ""}`;
  const shown = modelIssues.slice(0, MAX_TOOLTIP_MODELS);

  return (
    <span className="flex flex-wrap items-center gap-1">
      <Tooltip>
        <TooltipTrigger asChild>
          <Badge
            asChild={status !== undefined}
            variant="outline"
            className={status ? KIND_STYLE[status.health_kind] ?? "" : "text-muted-foreground"}
          >
            {status ? (
              <button
                type="button"
                aria-label={t("health.clearAria")}
                disabled={reset.credential.isPending}
                className="cursor-pointer disabled:opacity-60"
                onClick={(e) => {
                  e.stopPropagation();
                  reset.credential.mutate();
                }}
              >
                {text}
              </button>
            ) : (
              text
            )}
          </Badge>
        </TooltipTrigger>
        <TooltipContent className={TOOLTIP_CLASS}>
          <p>{status?.last_error ?? status?.health_json?.reason ?? label}</p>
          {status?.checked_at && <p>{t("health.asOf", { time: fmtTime(status.checked_at) })}</p>}
          {status && <p className="opacity-70">{t("health.clearHint")}</p>}
        </TooltipContent>
      </Tooltip>
      {modelIssues.length > 0 && (
        <Tooltip>
          <TooltipTrigger asChild>
            <Badge
              asChild
              variant="outline"
              className="bg-amber-500/15 text-amber-800 dark:text-amber-200"
            >
              <button
                type="button"
                aria-label={t("health.modelIssuesClearAria")}
                disabled={reset.models.isPending}
                className="cursor-pointer disabled:opacity-60"
                onClick={(e) => {
                  e.stopPropagation();
                  reset.models.mutate();
                }}
              >
                {t("health.modelIssues", { count: modelIssues.length })}
              </button>
            </Badge>
          </TooltipTrigger>
          <TooltipContent className={TOOLTIP_CLASS}>
            {shown.map((row) => (
              <ModelIssue key={row.id} row={row} />
            ))}
            {modelIssues.length > shown.length && (
              <span>{t("health.more", { count: modelIssues.length - shown.length })}</span>
            )}
            <span className="opacity-70">{t("health.modelIssuesHint")}</span>
          </TooltipContent>
        </Tooltip>
      )}
    </span>
  );
}
