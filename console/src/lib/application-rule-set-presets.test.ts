import { describe, expect, it } from "vitest";
import { APPLICATION_RULE_SET_PRESETS, OPENCODE_PRESET_RULES } from "./application-rule-set-presets";

describe("application rule-set presets", () => {
  it("offers the supported coding applications", () => {
    expect(APPLICATION_RULE_SET_PRESETS.map((preset) => preset.id)).toEqual([
      "opencode",
      "pi",
      "aider",
      "cline",
      "continue",
      "cursor",
    ]);
  });

  it("keeps Pi replacements ordered from specific phrases to fallbacks", () => {
    const pi = APPLICATION_RULE_SET_PRESETS.find((preset) => preset.id === "pi");
    expect(pi?.rules).toHaveLength(9);
    expect(pi?.rules.map((rule) => rule.sort_order)).toEqual([0, 1, 2, 3, 4, 5, 6, 7, 8]);
    expect(pi?.rules[0].config_json).toMatchObject({ locate: { match: "\\bPi documentation\\b" } });
    expect(pi?.rules[8].config_json).toMatchObject({ locate: { match: "\\bPI\\b" } });
  });

  it("contains paired OpenCode tool and MCP request/response transforms", () => {
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
