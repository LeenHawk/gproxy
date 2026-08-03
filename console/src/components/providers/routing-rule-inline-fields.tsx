import { useTranslation } from "react-i18next";
import type { RoutingRule } from "@/api/rules";
import { MemberNumberInput } from "@/components/routes/member-number-input";
import { Switch } from "@/components/ui/switch";
import { useRoutingRuleUpdate } from "./use-routing-rule-update";

export function RoutingRuleEnabled({
  providerId,
  rule,
}: {
  providerId: number;
  rule: RoutingRule;
}) {
  const { t } = useTranslation("rules");
  const update = useRoutingRuleUpdate(providerId);
  return (
    <Switch
      size="sm"
      checked={rule.enabled}
      disabled={update.isPending}
      aria-label={t("rule.enabled")}
      onClick={(event) => event.stopPropagation()}
      onCheckedChange={(enabled) => update.mutate({ rule, patch: { enabled } })}
    />
  );
}

export function RoutingRuleSortOrder({
  providerId,
  rule,
}: {
  providerId: number;
  rule: RoutingRule;
}) {
  const { t } = useTranslation("rules");
  const update = useRoutingRuleUpdate(providerId);
  return (
    <MemberNumberInput
      value={rule.sort_order}
      label={t("rule.sortOrder")}
      disabled={update.isPending}
      onCommit={(sort_order) => update.mutate({ rule, patch: { sort_order } })}
    />
  );
}
