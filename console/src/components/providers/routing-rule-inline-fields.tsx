import { useTranslation } from "react-i18next";
import type { RoutingRule } from "@/api/rules";
import { EnabledToggle } from "@/components/enabled-toggle";
import { MemberNumberInput } from "@/components/routes/member-number-input";
import { useRoutingRuleUpdate } from "./use-routing-rule-update";

export function RoutingRuleEnabled({
  providerId,
  rule,
}: {
  providerId: number;
  rule: RoutingRule;
}) {
  const update = useRoutingRuleUpdate(providerId);
  return (
    <EnabledToggle
      enabled={rule.enabled}
      pending={update.isPending}
      onToggle={(enabled) => update.mutate({ rule, patch: { enabled } })}
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
