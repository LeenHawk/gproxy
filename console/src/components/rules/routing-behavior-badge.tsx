import { useTranslation } from "react-i18next"
import type { RoutingRuleDto } from "@/generated/RoutingRuleDto"
import { Badge } from "@/components/ui/badge"

export function RoutingBehaviorBadge({ rule }: { rule: RoutingRuleDto }) {
  const { t } = useTranslation()
  const label = t(`rules.values.${rule.implementation}`)
  if (rule.implementation === "transform_to") {
    return (
      <Badge variant="info" className="h-auto max-w-full whitespace-normal py-1 text-left [overflow-wrap:anywhere]">
        {label} → {t(`rules.operations.${rule.dest_operation ?? rule.operation}`, { defaultValue: rule.dest_operation ?? rule.operation })} · {t(`rules.wires.${rule.dest_kind ?? rule.kind}`, { defaultValue: rule.dest_kind ?? rule.kind })}
      </Badge>
    )
  }
  if (rule.implementation === "passthrough") return <Badge variant="success">{label}</Badge>
  if (rule.implementation === "local") return <Badge variant="warning">{label}</Badge>
  return <Badge variant="destructive">{label}</Badge>
}
