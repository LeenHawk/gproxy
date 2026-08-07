import {
  listRules,
  upsertProviderRuleSet,
  upsertRule,
  upsertRuleSet,
  type ProviderRuleSet,
  type RuleInput,
  type RuleSet,
} from "@/api/rules";
import { OPENCODE_REQUEST_TOOL_PATHS, OPENCODE_TOOL_RENAMES } from "./transform-templates";

const RESPONSE_TOOL_PATHS = [
  "content.*.name",
  "content.*.tool_name",
  "message.content.*.name",
  "message.content.*.tool_name",
  "content_block.name",
  "content_block.tool_name",
];
const OPERATIONS = ["generate_content", "stream_generate_content"];

type PresetRule = Omit<RuleInput, "id" | "rule_set_id">;

export interface ApplicationRuleSetPreset {
  id: string;
  name: string;
  description: string;
  rules: PresetRule[];
}

function transform(
  sortOrder: number,
  phase: "request" | "response",
  locate: { match: string } | { paths: readonly string[] },
  actions: unknown[],
  clientPattern: string | null = null,
): PresetRule {
  return {
    kind: "transform",
    config_json: {
      phase,
      locate: "paths" in locate ? { paths: [...locate.paths] } : locate,
      actions,
    },
    filter_model_pattern: null,
    filter_operation_keys: OPERATIONS,
    filter_header_pattern: clientPattern,
    sort_order: sortOrder,
    enabled: true,
  };
}

function sanitizeRules(
  entries: readonly (readonly [string, string])[],
  clientPattern: string | null = null,
): PresetRule[] {
  return entries.map(([pattern, replacement], index) =>
    transform(index, "request", { match: pattern }, [{ op: "replace_text", with: replacement }], clientPattern),
  );
}

/// Client scopes (case-insensitive regex over every `name: value` inbound
/// header line), derived from each client's own source:
///
/// - opencode: `user-agent: opencode/<ver> ai-sdk/... runtime/bun/...` (verified
///   against live traffic).
/// - aider: sends nothing of its own — every request goes through litellm,
///   whose default is `User-Agent: litellm/{version}`.
/// - cline: `User-Agent: Cline/<ver>` plus `X-Title` / `HTTP-Referer`, but only
///   on its own billing provider; a plain custom endpoint gets none of them.
/// - continue / cursor: no default self-identifying header at all (Continue only
///   sends one if the user configures `requestOptions.headers`).
/// - pi: identifies itself as `pi (<os>)` on the Codex path, but impersonates
///   Claude Code (`user-agent: claude-cli/<ver>`) on the Anthropic path, so it
///   cannot be told apart there.
///
/// Unverifiable clients keep a best-guess pattern on purpose: an unmatched
/// filter merely leaves the preset inert, whereas an unscoped preset rewrites
/// every other client's body (these rules match on the whole request text).
/// Confirm against a captured request in Logs → Requests and adjust.
export const PRESET_CLIENT_PATTERNS = {
  opencode: "^user-agent: opencode/",
  pi: "^user-agent: pi[ /]",
  aider: "^user-agent: litellm/",
  cline: "^user-agent: cline/|^x-title: cline$|^http-referer: https://cline\\.bot",
  continue: "^user-agent: continue/",
  cursor: "^user-agent: cursor/",
} as const;

const CLIENT = PRESET_CLIENT_PATTERNS;

export const OPENCODE_PRESET_RULES: PresetRule[] = [
  transform(0, "request", { paths: ["system", "system.*.text"] }, [
    {
      op: "replace_regex",
      pattern: "(?s)Here is some useful information about the environment you are running in:\\s*<env>.*?</env>\\n?",
      with: "",
    },
    { op: "replace_regex", pattern: "(?i)https://github\\.com/anomalyco/opencode(?:/[^\\s)]*)?", with: "the project issue tracker" },
    { op: "replace_regex", pattern: "(?i)https://opencode\\.ai/docs(?:/[^\\s)]*)?", with: "the documentation" },
    { op: "replace_regex", pattern: "(?i)(?:~/)?\\.config/opencode/|\\.opencode/", with: "the assistant config directory/" },
    { op: "replace_regex", pattern: "(?i)/tmp/opencode\\b", with: "/tmp/coding-agent" },
    { op: "replace_regex", pattern: "(?i)\\bopencode\\b", with: "the coding assistant" },
    { op: "replace_regex", pattern: "\\bgit repo\\b", with: "git repository" },
  ], CLIENT.opencode),
  transform(1, "request", { paths: OPENCODE_REQUEST_TOOL_PATHS }, OPENCODE_TOOL_RENAMES.map(([from, replacement]) => ({
    op: "replace_text",
    from,
    with: replacement,
  })), CLIENT.opencode),
  transform(2, "request", { paths: OPENCODE_REQUEST_TOOL_PATHS }, [
    { op: "replace_regex", pattern: "^mcp_([^_].*)$", with: "mcp__$1" },
  ], CLIENT.opencode),
  transform(3, "response", { paths: RESPONSE_TOOL_PATHS }, OPENCODE_TOOL_RENAMES.map(([original, renamed]) => ({
    op: "replace_text",
    from: renamed,
    with: original,
  })), CLIENT.opencode),
  transform(4, "response", { paths: RESPONSE_TOOL_PATHS }, [
    { op: "replace_regex", pattern: "^mcp__([^_].*)$", with: "mcp_$1" },
  ], CLIENT.opencode),
];

export const APPLICATION_RULE_SET_PRESETS: ApplicationRuleSetPreset[] = [
  {
    id: "opencode",
    name: "OpenCode",
    description: "gproxy:preset:opencode:v1",
    rules: OPENCODE_PRESET_RULES,
  },
  {
    id: "pi",
    name: "pi-mono",
    description: "gproxy:preset:pi:v1",
    rules: sanitizeRules([
      ["\\bPi documentation\\b", "Harness documentation"],
      ["\\binside pi, a coding\\b", "inside the coding"],
      ["\\bpi packages\\b", "harness packages"],
      ["\\bpi topics\\b", "harness topics"],
      ["\\bpi \\.md files\\b", "the harness .md files"],
      ["\\bpi itself\\b", "the harness itself"],
      ["\\bpi\\b", "the agent"],
      ["\\bPi\\b", "The agent"],
      ["\\bPI\\b", "AGENT"],
    ], CLIENT.pi),
  },
  {
    id: "aider",
    name: "Aider",
    description: "gproxy:preset:aider:v1",
    rules: sanitizeRules([
      ["\\bAider\\b", "The assistant"],
      ["\\baider\\b", "the assistant"],
    ], CLIENT.aider),
  },
  {
    id: "cline",
    name: "Cline",
    description: "gproxy:preset:cline:v1",
    rules: sanitizeRules([["\\bCline\\b", "Assistant"]], CLIENT.cline),
  },
  {
    id: "continue",
    name: "Continue",
    description: "gproxy:preset:continue:v1",
    rules: sanitizeRules([["\\bContinue\\b", "Assistant"]], CLIENT.continue),
  },
  {
    id: "cursor",
    name: "Cursor",
    description: "gproxy:preset:cursor:v1",
    rules: sanitizeRules([["\\bCursor\\b", "Assistant"]], CLIENT.cursor),
  },
];

export function findApplicationPreset(ruleSets: RuleSet[], preset: ApplicationRuleSetPreset) {
  return ruleSets.find((ruleSet) => ruleSet.description === preset.description);
}

export async function applyApplicationPreset(
  providerId: number,
  preset: ApplicationRuleSetPreset,
  ruleSets: RuleSet[],
  attachments: ProviderRuleSet[],
) {
  let ruleSet = findApplicationPreset(ruleSets, preset);
  if (!ruleSet) {
    ruleSet = await upsertRuleSet({
      name: `${preset.name} compatibility`,
      description: preset.description,
      enabled: true,
    });
  } else if (!ruleSet.enabled) {
    ruleSet = await upsertRuleSet({
      id: ruleSet.id,
      name: ruleSet.name,
      description: ruleSet.description,
      enabled: true,
    });
  }

  const existing = await listRules(ruleSet.id);
  for (const expected of preset.rules) {
    const current = existing.find((rule) => rule.kind === expected.kind && rule.sort_order === expected.sort_order);
    await upsertRule(ruleSet.id, { ...expected, id: current?.id ?? null, rule_set_id: ruleSet.id });
  }

  const attachment = attachments.find((item) => item.rule_set_id === ruleSet.id);
  if (!attachment || !attachment.enabled) {
    await upsertProviderRuleSet(providerId, {
      id: attachment?.id ?? null,
      provider_id: providerId,
      rule_set_id: ruleSet.id,
      sort_order: attachment?.sort_order ?? attachments.length,
      enabled: true,
    });
  }
  return ruleSet;
}
