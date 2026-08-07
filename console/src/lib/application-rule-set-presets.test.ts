import { describe, expect, it } from "vitest";
import {
  APPLICATION_RULE_SET_PRESETS,
  OPENCODE_PRESET_RULES,
  PRESET_CLIENT_PATTERNS,
} from "./application-rule-set-presets";

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

/** Real inbound header lines, as the backend renders them for the client filter
 *  ("name: value", one per header). Taken from captured gproxy traffic and from
 *  each client's own header-building code. */
const CLIENTS: Record<string, string[]> = {
  opencode: ["user-agent: opencode/1.18.10 ai-sdk/provider-utils/4.0.27 runtime/bun/1.3.14"],
  claudeCode: ["user-agent: claude-cli/2.1.223 (external, cli)", "x-app: cli"],
  claudeCodeSdk: ["user-agent: claude-cli/2.1.223 (external, sdk-cli)"],
  codex: ["user-agent: codex-tui/0.146.0 (Windows 10.0.26200; x86_64)", "originator: codex-tui"],
  aider: ["user-agent: litellm/1.82.4"],
  cline: ["user-agent: Cline/3.0.38", "x-title: Cline", "http-referer: https://cline.bot"],
  openaiPython: ["user-agent: OpenAI/Python 2.46.0"],
  curl: ["user-agent: curl/8.14.1"],
};

/** Preset → the sample client it scopes; `""` = that client ships no
 *  self-identifying header, so its preset stays inert until the user points it
 *  at whatever their own setup sends. */
const OWN_CLIENT: Record<keyof typeof PRESET_CLIENT_PATTERNS, string> = {
  opencode: "opencode",
  aider: "aider",
  cline: "cline",
  pi: "",
  continue: "",
  cursor: "",
};

function matchesClient(pattern: string, lines: string[]): boolean {
  const re = new RegExp(pattern, "i");
  return lines.some((line) => re.test(line));
}

describe("preset client scopes", () => {
  it("matches the client it scopes", () => {
    for (const [preset, client] of Object.entries(OWN_CLIENT)) {
      if (!client) continue;
      const pattern = PRESET_CLIENT_PATTERNS[preset as keyof typeof PRESET_CLIENT_PATTERNS];
      expect(matchesClient(pattern, CLIENTS[client]), `${preset} must match ${client}`).toBe(true);
    }
  });

  // The bug the filter exists for: OpenCode's response-side tool renames must
  // never touch another client sharing the provider (Claude Code's `Read` was
  // being rewritten to `read`, so it rejected its own tool calls).
  it("never matches another client", () => {
    for (const [preset, pattern] of Object.entries(PRESET_CLIENT_PATTERNS)) {
      for (const [client, lines] of Object.entries(CLIENTS)) {
        if (client === OWN_CLIENT[preset as keyof typeof PRESET_CLIENT_PATTERNS]) continue;
        expect(matchesClient(pattern, lines), `${preset} must not match ${client}`).toBe(false);
      }
    }
  });

  it("ships a compilable scope on every preset rule", () => {
    for (const preset of APPLICATION_RULE_SET_PRESETS) {
      for (const rule of preset.rules) {
        expect(rule.filter_header_pattern, `${preset.id} rule is unscoped`).toBeTruthy();
        expect(() => new RegExp(rule.filter_header_pattern as string, "i")).not.toThrow();
      }
    }
  });
});
