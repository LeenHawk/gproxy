import { describe, expect, it } from "vitest";
import {
  groupRulesByKind, groupRulesByKindStable, sortRulesForPipeline, computeRuleSetUsage,
} from "./rule-usage";
import type { Rule, ProviderRuleSet } from "@/api/rules";

function rule(o: Partial<Rule>): Rule {
  return {
    id: 1, rule_set_id: 1, kind: "rewrite", config_json: {},
    filter_model_pattern: null, filter_operation_keys: null,
    sort_order: 0, enabled: true, created_at: 0, updated_at: 0, ...o,
  };
}
function attach(o: Partial<ProviderRuleSet>): ProviderRuleSet {
  return { id: 1, provider_id: 1, rule_set_id: 1, sort_order: 0, enabled: true, created_at: 0, updated_at: 0, ...o };
}

describe("sortRulesForPipeline", () => {
  it("orders by kind rank before sort_order", () => {
    const out = sortRulesForPipeline([
      rule({ id: 1, kind: "header", sort_order: 0 }),
      rule({ id: 2, kind: "system_text", sort_order: 99 }),
      rule({ id: 3, kind: "rewrite", sort_order: 1 }),
    ]);
    expect(out.map((r) => r.kind)).toEqual(["system_text", "rewrite", "header"]);
  });
});

describe("groupRulesByKind", () => {
  it("groups in execution order and drops empty kinds", () => {
    const groups = groupRulesByKind([
      rule({ id: 1, kind: "sanitize" }),
      rule({ id: 2, kind: "system_text" }),
    ]);
    expect(groups.map((g) => g.kind)).toEqual(["system_text", "sanitize"]);
  });
});

describe("groupRulesByKindStable", () => {
  it("preserves attachment order within a kind, not global sort_order", () => {
    // Set-A is attachment 0, Set-B is attachment 1 (attachment order: A then B).
    // Set-B's rewrite has sort_order=1, Set-A's rewrite has sort_order=5.
    // Flattened in attachment order: setA_rewrite (sort_order=5) then setB_rewrite (sort_order=1).
    // groupRulesByKindStable must keep setA before setB within "rewrite".
    // It must also place "rewrite" before "header" (kind-rank order).
    const setA_rewrite = rule({ id: 10, kind: "rewrite", sort_order: 5 });
    const setB_rewrite = rule({ id: 20, kind: "rewrite", sort_order: 1 });
    const setA_header = rule({ id: 30, kind: "header", sort_order: 0 });

    // Input order = attachment-A rules (sorted by sort_order), then attachment-B rules.
    // Within set-A: header(0) then rewrite(5); within set-B: rewrite(1).
    const flat = [setA_header, setA_rewrite, setB_rewrite];

    const groups = groupRulesByKindStable(flat);
    expect(groups.map((g) => g.kind)).toEqual(["rewrite", "header"]);
    const rewrites = groups.find((g) => g.kind === "rewrite")!.rules;
    expect(rewrites.map((r) => r.id)).toEqual([10, 20]); // setA before setB
  });
});

describe("computeRuleSetUsage", () => {
  it("classifies private / shared / unused", () => {
    const atts = [
      attach({ id: 1, rule_set_id: 7, provider_id: 1 }),
      attach({ id: 2, rule_set_id: 7, provider_id: 2 }),
      attach({ id: 3, rule_set_id: 8, provider_id: 1 }),
    ];
    expect(computeRuleSetUsage(7, atts).scope).toBe("shared");
    expect(computeRuleSetUsage(7, atts).providerIds).toEqual([1, 2]);
    expect(computeRuleSetUsage(8, atts).scope).toBe("private");
    expect(computeRuleSetUsage(9, atts).scope).toBe("unused");
  });
});
