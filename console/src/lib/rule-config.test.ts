import { describe, expect, it } from "vitest";
import { normalizeRuleConfig } from "./rule-config";

describe("normalizeRuleConfig", () => {
  it("materializes every structured editor default into config_json", () => {
    expect(normalizeRuleConfig("cache_breakpoint", {})).toEqual({ target: "system" });
    expect(normalizeRuleConfig("rewrite", {})).toEqual({
      path: "",
      action: "set",
      value_json: null,
    });
    expect(normalizeRuleConfig("header", {})).toEqual({
      name: "",
      value: "",
      mode: "override",
    });
    expect(normalizeRuleConfig("system_text", {})).toEqual({
      text: "",
      position: "prepend",
    });
  });

  it("preserves explicit values while repairing action-specific rewrite values", () => {
    expect(
      normalizeRuleConfig("cache_breakpoint", { target: "message", ttl: "1h" }),
    ).toEqual({ target: "message", ttl: "1h" });
    expect(normalizeRuleConfig("rewrite", { path: "a", action: "merge" })).toEqual({
      path: "a",
      action: "merge",
      value_json: {},
    });
    expect(
      normalizeRuleConfig("rewrite", {
        path: "a",
        action: "delete",
        value_json: "stale",
      }),
    ).toEqual({ path: "a", action: "delete" });
  });
});
