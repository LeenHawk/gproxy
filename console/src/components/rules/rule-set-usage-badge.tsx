import { useTranslation } from "react-i18next";
import type { computeRuleSetUsage } from "@/lib/rule-usage";
import { Badge } from "@/components/ui/badge";

interface RuleSetUsageBadgeProps {
  usage: ReturnType<typeof computeRuleSetUsage>;
  providerNames: Map<number, string>;
}

export function RuleSetUsageBadge({ usage, providerNames }: RuleSetUsageBadgeProps) {
  const { t } = useTranslation("rules");
  const variant = usage.scope === "unused" ? "outline" : usage.scope === "shared" ? "default" : "secondary";
  const label = usage.scope === "shared"
    ? t("usage.shared", { count: usage.providerIds.length })
    : t(`usage.${usage.scope}`);
  const title = usage.scope === "unused"
    ? undefined
    : `${t("usage.usedBy")}: ${usage.providerIds.map((id) => providerNames.get(id) ?? String(id)).join(", ")}`;

  return <Badge variant={variant} title={title} className="shrink-0 px-1 py-0 text-[10px]">{label}</Badge>;
}
