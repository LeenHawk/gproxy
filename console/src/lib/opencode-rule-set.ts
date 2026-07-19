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

export const OPENCODE_PRESET_DESCRIPTION = "gproxy:preset:opencode:v1";

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

function transform(sortOrder: number, phase: "request" | "response", paths: readonly string[], actions: unknown[]): PresetRule {
  return {
    kind: "transform",
    config_json: { phase, locate: { paths: [...paths] }, actions },
    filter_model_pattern: null,
    filter_operation_keys: OPERATIONS,
    sort_order: sortOrder,
    enabled: true,
  };
}

export const OPENCODE_PRESET_RULES: PresetRule[] = [
  transform(0, "request", ["system", "system.*.text"], [
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
  transform(1, "request", OPENCODE_REQUEST_TOOL_PATHS, OPENCODE_TOOL_RENAMES.map(([from, replacement]) => ({
    op: "replace_text",
    from,
    with: replacement,
  }))),
  transform(2, "request", OPENCODE_REQUEST_TOOL_PATHS, [
    { op: "replace_regex", pattern: "^mcp_([^_].*)$", with: "mcp__$1" },
  ]),
  transform(3, "response", RESPONSE_TOOL_PATHS, OPENCODE_TOOL_RENAMES.map(([original, renamed]) => ({
    op: "replace_text",
    from: renamed,
    with: original,
  }))),
  transform(4, "response", RESPONSE_TOOL_PATHS, [
    { op: "replace_regex", pattern: "^mcp__([^_].*)$", with: "mcp_$1" },
  ]),
];

export function findOpenCodePreset(ruleSets: RuleSet[]) {
  return ruleSets.find((ruleSet) => ruleSet.description === OPENCODE_PRESET_DESCRIPTION);
}

export async function applyOpenCodePreset(providerId: number, ruleSets: RuleSet[], attachments: ProviderRuleSet[]) {
  const ruleSet = findOpenCodePreset(ruleSets) ?? await upsertRuleSet({
    name: "OpenCode compatibility",
    description: OPENCODE_PRESET_DESCRIPTION,
    enabled: true,
  });
  const existing = await listRules(ruleSet.id);

  for (const preset of OPENCODE_PRESET_RULES) {
    const current = existing.find((rule) => rule.kind === preset.kind && rule.sort_order === preset.sort_order);
    await upsertRule(ruleSet.id, { ...preset, id: current?.id ?? null, rule_set_id: ruleSet.id });
  }

  if (!attachments.some((attachment) => attachment.rule_set_id === ruleSet.id)) {
    await upsertProviderRuleSet(providerId, {
      provider_id: providerId,
      rule_set_id: ruleSet.id,
      sort_order: attachments.length,
      enabled: true,
    });
  }
  return ruleSet;
}
