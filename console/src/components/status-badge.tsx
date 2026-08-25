import type { CredentialHealthDto } from "@/generated/CredentialHealthDto"
import { useTranslation } from "react-i18next"
import { Badge } from "@/components/ui/badge"
import { cn } from "@/lib/utils"

const styles: Record<CredentialHealthDto, string> = {
  healthy: "border-state-healthy/35 text-state-healthy",
  degraded: "border-state-warning/40 text-state-warning",
  dead: "border-state-critical/40 text-state-critical",
  disabled: "border-state-disabled/40 text-state-disabled",
  unknown: "border-state-info/35 text-state-info",
}

export function StatusBadge({ status }: { status: CredentialHealthDto }) {
  const { t } = useTranslation()
  return (
    <Badge variant="outline" className={cn("gap-1.5", styles[status])}>
      <span className="size-1.5 rounded-full bg-current" aria-hidden="true" />
      {t(`common.status.${status}`)}
    </Badge>
  )
}
