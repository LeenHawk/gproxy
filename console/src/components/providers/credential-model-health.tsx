import type { CredentialHealthDto } from "@/generated/CredentialHealthDto"
import type { CredentialModelHealthDto } from "@/generated/CredentialModelHealthDto"
import { useMutation, useQueryClient } from "@tanstack/react-query"
import { useTranslation } from "react-i18next"
import { toast } from "sonner"
import { ApiError } from "@/api/client"
import { resetCredentialHealth } from "@/api/control"
import { StatusBadge } from "@/components/status-badge"
import { Tooltip, TooltipContent, TooltipTrigger } from "@/components/ui/tooltip"
import { formatInstant } from "@/lib/format"

const SHOWN = 8

/* One badge carries credential health. v2 idiom: healthy models never
   surface; abnormal ones ride the status badge's tooltip. Clicking the badge
   clears the recorded rows — the manual escape hatch for a stale state,
   since health otherwise only refreshes when the same model is hit again. */
export function CredentialHealthBadge({ credentialId, health, models, observedAt }: {
  credentialId: number
  health: CredentialHealthDto
  models: Array<CredentialModelHealthDto>
  observedAt?: number | null
}) {
  const { t, i18n } = useTranslation()
  const client = useQueryClient()
  const reset = useMutation({
    mutationFn: () => resetCredentialHealth(credentialId),
    onSuccess: async () => {
      await client.invalidateQueries({ queryKey: ["credentials"] })
      toast.success(t("providers.credentials.healthReset.success"))
    },
    onError: (error) => toast.error(error instanceof ApiError ? error.message : t("providers.credentials.healthReset.error")),
  })
  const issues = models
    .filter((value) => value.health === "degraded" || value.health === "dead")
    .sort((left, right) => left.model.localeCompare(right.model))
  const observed = formatInstant(observedAt ?? null, i18n.language)
  return (
    <Tooltip>
      <TooltipTrigger asChild>
        <button
          type="button"
          className="inline-flex disabled:opacity-50"
          disabled={reset.isPending}
          aria-label={t("providers.credentials.healthReset.action")}
          onClick={(event) => {
            event.stopPropagation()
            reset.mutate()
          }}
        >
          <StatusBadge status={health} />
        </button>
      </TooltipTrigger>
      <TooltipContent className="max-w-sm flex-col items-start gap-1">
        {observed ? <span className="opacity-70">{t("providers.credentials.healthObserved", { time: observed })}</span> : null}
        {issues.length ? (
          <span className="font-medium">{t("providers.credentials.modelHealth.issues", { count: issues.length })}</span>
        ) : null}
        {issues.slice(0, SHOWN).map((issue) => (
          <span key={issue.model} className="flex flex-col">
            <span>
              <span className="font-mono">{issue.model || "*"}</span>
              {" · "}
              {t(`common.status.${issue.health}`)}
              {" · "}
              {formatInstant(issue.observed_at, i18n.language)}
            </span>
            {issue.detail ? <span className="font-mono opacity-70">{issue.response_status != null ? `${issue.response_status} · ` : ""}{issue.detail}</span> : null}
          </span>
        ))}
        {issues.length > SHOWN ? <span>{t("providers.credentials.modelHealth.more", { count: issues.length - SHOWN })}</span> : null}
        <span className="opacity-70">{t("providers.credentials.healthReset.hint")}</span>
      </TooltipContent>
    </Tooltip>
  )
}
