import type { RoutingRule } from "@/api/rules";
import { EnabledToggle } from "@/components/enabled-toggle";
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
