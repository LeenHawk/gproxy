import { describe, expect, it } from "vitest";
import { OPENCODE_PRESET_RULES } from "./opencode-rule-set";

describe("OpenCode rule-set preset", () => {
  it("contains paired tool and MCP request/response transforms", () => {
    expect(OPENCODE_PRESET_RULES).toHaveLength(5);
    expect(OPENCODE_PRESET_RULES.map((rule) => rule.sort_order)).toEqual([0, 1, 2, 3, 4]);

    const configs = OPENCODE_PRESET_RULES.map((rule) => rule.config_json as {
      phase: string;
      actions: { op: string; pattern?: string; from?: string; with: string }[];
    });
    expect(configs.map((config) => config.phase)).toEqual([
      "request",
      "request",
      "request",
      "response",
      "response",
    ]);
    expect(configs[1].actions).toContainEqual({ op: "replace_text", from: "todowrite", with: "TodoWrite" });
    expect(configs[3].actions).toContainEqual({ op: "replace_text", from: "TodoWrite", with: "todowrite" });
    expect(configs[2].actions[0]).toMatchObject({ pattern: "^mcp_([^_].*)$", with: "mcp__$1" });
    expect(configs[4].actions[0]).toMatchObject({ pattern: "^mcp__([^_].*)$", with: "mcp_$1" });
  });
});
