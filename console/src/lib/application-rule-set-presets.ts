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
    sort_order: sortOrder,
    enabled: true,
  };
}

function sanitizeRules(entries: readonly (readonly [string, string])[]): PresetRule[] {
  return entries.map(([pattern, replacement], index) =>
    transform(index, "request", { match: pattern }, [{ op: "replace_text", with: replacement }]),
  );
}

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
  ]),
  transform(1, "request", { paths: OPENCODE_REQUEST_TOOL_PATHS }, OPENCODE_TOOL_RENAMES.map(([from, replacement]) => ({
    op: "replace_text",
    from,
    with: replacement,
  }))),
  transform(2, "request", { paths: OPENCODE_REQUEST_TOOL_PATHS }, [
    { op: "replace_regex", pattern: "^mcp_([^_].*)$", with: "mcp__$1" },
  ]),
  transform(3, "response", { paths: RESPONSE_TOOL_PATHS }, OPENCODE_TOOL_RENAMES.map(([original, renamed]) => ({
    op: "replace_text",
    from: renamed,
    with: original,
  }))),
  transform(4, "response", { paths: RESPONSE_TOOL_PATHS }, [
    { op: "replace_regex", pattern: "^mcp__([^_].*)$", with: "mcp_$1" },
  ]),
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
    ]),
  },
  {
    id: "aider",
    name: "Aider",
    description: "gproxy:preset:aider:v1",
    rules: sanitizeRules([
      ["\\bAider\\b", "The assistant"],
      ["\\baider\\b", "the assistant"],
    ]),
  },
  {
    id: "cline",
    name: "Cline",
    description: "gproxy:preset:cline:v1",
    rules: sanitizeRules([["\\bCline\\b", "Assistant"]]),
  },
  {
    id: "continue",
    name: "Continue",
    description: "gproxy:preset:continue:v1",
    rules: sanitizeRules([["\\bContinue\\b", "Assistant"]]),
  },
  {
    id: "cursor",
    name: "Cursor",
    description: "gproxy:preset:cursor:v1",
    rules: sanitizeRules([["\\bCursor\\b", "Assistant"]]),
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
