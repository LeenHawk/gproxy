import type { Rule, ProviderRuleSet } from "@/api/rules";

// Backend executes rules in this fixed kind order (process/compile.rs RuleConfig::rank).
// sort_order only breaks ties WITHIN a kind / orders sets — it never reorders across kinds.
export const RULE_KIND_ORDER = [
  "system_text", "cache_breakpoint", "rewrite", "sanitize", "header",
] as const;

export function kindRank(kind: string): number {
  const i = (RULE_KIND_ORDER as readonly string[]).indexOf(kind);
  return i === -1 ? RULE_KIND_ORDER.length : i;
}

/** Sort by kind rank, then sort_order, then id — mirrors backend effective order. */
export function sortRulesForPipeline(rules: Rule[]): Rule[] {
  return [...rules].sort(
    (a, b) =>
      kindRank(a.kind) - kindRank(b.kind) ||
      a.sort_order - b.sort_order ||
      a.id - b.id,
  );
}

/** Group rules by kind in execution order; only kinds that have rules appear.
 *  Rules inside a group are ordered by sort_order then id. */
export function groupRulesByKind(rules: Rule[]): { kind: string; rules: Rule[] }[] {
  return RULE_KIND_ORDER
    .map((kind) => ({
      kind,
      rules: rules
        .filter((r) => r.kind === kind)
        .sort((a, b) => a.sort_order - b.sort_order || a.id - b.id),
    }))
    .filter((g) => g.rules.length > 0);
}

/** Stable-partition rules into kind groups, PRESERVING the input order within each kind.
 *  Use this when the caller has already arranged the flat list in execution order
 *  (e.g. flattened from attachment-order → within-set sort_order) and a re-sort
 *  within a kind would destroy that order. */
export function groupRulesByKindStable(rules: Rule[]): { kind: string; rules: Rule[] }[] {
  const buckets = new Map<string, Rule[]>(RULE_KIND_ORDER.map((k) => [k, []]));
  for (const r of rules) {
    buckets.get(r.kind)?.push(r);
  }
  return RULE_KIND_ORDER
    .map((kind) => ({ kind, rules: buckets.get(kind) ?? [] }))
    .filter((g) => g.rules.length > 0);
}

export type RuleSetScope = "private" | "shared" | "unused";

/** Derived privacy: a set attached to exactly one provider is "private",
 *  to two or more is "shared", to none is "unused". */
export function computeRuleSetUsage(
  ruleSetId: number,
  attachments: ProviderRuleSet[],
): { scope: RuleSetScope; providerIds: number[] } {
  const providerIds = Array.from(
    new Set(attachments.filter((a) => a.rule_set_id === ruleSetId).map((a) => a.provider_id)),
  );
  const scope: RuleSetScope =
    providerIds.length === 0 ? "unused" : providerIds.length === 1 ? "private" : "shared";
  return { scope, providerIds };
}
