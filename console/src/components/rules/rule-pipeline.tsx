import { useQueries, useQuery } from "@tanstack/react-query";
import { useTranslation } from "react-i18next";
import { providerRuleSetsQuery, ruleSetsQuery, rulesQuery, type Rule } from "@/api/rules";
import { groupRulesByKindStable } from "@/lib/rule-usage";
import { RULE_KIND_META, summarizeRuleConfig } from "./rule-kind-meta";
import { Badge } from "@/components/ui/badge";

export function RulePipeline({ providerId }: { providerId: number }) {
  const { t } = useTranslation("rules");
  const { data: attachments = [] } = useQuery(providerRuleSetsQuery(providerId));
  const { data: ruleSets = [] } = useQuery(ruleSetsQuery);
  const enabledSets = attachments
    .filter((a) => a.enabled)
    .sort((x, y) => x.sort_order - y.sort_order);
  const ruleQueries = useQueries({ queries: enabledSets.map((a) => rulesQuery(a.rule_set_id)) });

  const nameOf = new Map(ruleSets.map((rs) => [rs.id, rs.name]));
  const setEnabled = new Map(ruleSets.map((rs) => [rs.id, rs.enabled]));
  // Only rules from enabled sets whose rule_set is itself enabled.
  // Sort each set's rules by (sort_order, id) BEFORE flattening so the flattened
  // order matches the backend: attachment order, then within-set sort_order.
  const effective: { rule: Rule; setName: string }[] = enabledSets.flatMap((a, i) => {
    if (!setEnabled.get(a.rule_set_id)) return [];
    const rs = (ruleQueries[i].data ?? [])
      .filter((r) => r.enabled)
      .sort((a, b) => a.sort_order - b.sort_order || a.id - b.id);
    return rs.map((rule) => ({ rule, setName: nameOf.get(a.rule_set_id) ?? `#${a.rule_set_id}` }));
  });
  const groups = groupRulesByKindStable(effective.map((e) => e.rule));
  const setNameByRuleId = new Map(effective.map((e) => [e.rule.id, e.setName]));

  return (
    <section className="grid gap-2 rounded-md border p-3">
      <div className="grid gap-0.5">
        <h3 className="text-sm font-medium">{t("pipeline.title")}</h3>
        <p className="text-xs text-muted-foreground">{t("pipeline.caption")}</p>
      </div>
      {groups.length === 0 ? (
        <p className="text-sm text-muted-foreground">{t("pipeline.empty")}</p>
      ) : (
        groups.map((g) => {
          const Icon = RULE_KIND_META[g.kind]?.icon;
          return (
            <div key={g.kind} className="grid gap-1">
              <div className="flex items-center gap-2 text-xs font-medium">
                {Icon && <Icon className="size-3.5" />} {t(`kind.${g.kind}`)}
              </div>
              {g.rules.map((r) => (
                <div key={r.id} className="flex items-center justify-between gap-2 pl-5 text-xs">
                  <span className="truncate font-mono">{summarizeRuleConfig(r.kind, r.config_json)}</span>
                  <span className="flex shrink-0 items-center gap-1.5">
                    <Badge variant="outline" className="text-[10px]">
                      {r.filter_model_pattern ?? t("pipeline.allRequests")}
                    </Badge>
                    <span className="text-muted-foreground">
                      {t("pipeline.fromSet", { name: setNameByRuleId.get(r.id) })}
                    </span>
                  </span>
                </div>
              ))}
            </div>
          );
        })
      )}
    </section>
  );
}
