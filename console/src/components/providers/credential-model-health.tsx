import type { CredentialModelHealthDto } from "@/generated/CredentialModelHealthDto"
import { useTranslation } from "react-i18next"
import { Badge } from "@/components/ui/badge"
import { Tooltip, TooltipContent, TooltipTrigger } from "@/components/ui/tooltip"
import { formatInstant } from "@/lib/format"

const SHOWN = 8

/* v2 idiom: healthy models never surface; only abnormal ones earn an amber
   count badge, with per-model detail in its tooltip. */
export function CredentialModelHealth({ values }: { values: Array<CredentialModelHealthDto> }) {
  const { t, i18n } = useTranslation()
  const issues = values
    .filter((value) => value.health === "degraded" || value.health === "dead")
    .sort((left, right) => left.model.localeCompare(right.model))
  if (!issues.length) return null
  return (
    <Tooltip>
      <TooltipTrigger asChild>
        <Badge variant="outline" className="border-state-warning/40 bg-state-warning/10 text-state-warning">
          {t("providers.credentials.modelHealth.issues", { count: issues.length })}
        </Badge>
      </TooltipTrigger>
      <TooltipContent className="max-w-sm flex-col items-start gap-1">
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
      </TooltipContent>
    </Tooltip>
  )
}
