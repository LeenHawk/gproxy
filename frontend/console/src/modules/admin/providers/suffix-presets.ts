// Suffix presets mirroring the old sdk/gproxy-provider/src/suffix.rs system.
// Each preset produces rewrite rule templates that can be merged onto a
// per-alias rewrite rule set.
//
// The old suffix system supported some header-modifying suffixes (Claude
// `-fast` / `-1m` / `-200k` / `-non-fast`) which cannot be expressed as
// body-only rewrite rules. Those are marked with `unsupported: true` and
// filtered out of the UI.

export type SuffixProtocol = "claude" | "openai_response" | "openai_chat_completions" | "gemini";

export type SuffixActionSetBody = {
  kind: "set";
  /// JSON path using dot notation (e.g. "thinking", "reasoning.effort").
  path: string;
  /// The JSON value to set.
  value: unknown;
};

export type SuffixEntry = {
  suffix: string;
  /// Short label shown in the dropdown.
  label: string;
  /// Rewrite rule actions this suffix produces (body-only).
  actions: SuffixActionSetBody[];
};

export type SuffixGroup = {
  /// Group id used as a form key in the dialog.
  key: string;
  /// Display label for the group.
  label: string;
  /// Mutually-exclusive entries — the user picks at most one per group.
  entries: SuffixEntry[];
};

// ---------------------------------------------------------------------------
// Claude
// ---------------------------------------------------------------------------

const CLAUDE_GROUPS: SuffixGroup[] = [
  {
    key: "thinking",
    label: "Thinking",
    entries: [
      { suffix: "-thinking-none", label: "thinking: disabled", actions: [{ kind: "set", path: "thinking", value: { type: "disabled" } }] },
      { suffix: "-thinking-low", label: "thinking: low (1024 tokens)", actions: [{ kind: "set", path: "thinking", value: { type: "enabled", budget_tokens: 1024, display: "summarized" } }] },
      { suffix: "-thinking-medium", label: "thinking: medium (10240 tokens)", actions: [{ kind: "set", path: "thinking", value: { type: "enabled", budget_tokens: 10240, display: "summarized" } }] },
      { suffix: "-thinking-high", label: "thinking: high (32768 tokens)", actions: [{ kind: "set", path: "thinking", value: { type: "enabled", budget_tokens: 32768, display: "summarized" } }] },
      { suffix: "-thinking-adaptive", label: "thinking: adaptive", actions: [{ kind: "set", path: "thinking", value: { type: "adaptive", display: "summarized" } }] },
    ],
  },
  {
    key: "effort",
    label: "Effort",
    entries: [
      { suffix: "-effort-low", label: "effort: low", actions: [{ kind: "set", path: "output_config", value: { effort: "low" } }] },
      { suffix: "-effort-medium", label: "effort: medium", actions: [{ kind: "set", path: "output_config", value: { effort: "medium" } }] },
      { suffix: "-effort-high", label: "effort: high", actions: [{ kind: "set", path: "output_config", value: { effort: "high" } }] },
      { suffix: "-effort-xhigh", label: "effort: xhigh", actions: [{ kind: "set", path: "output_config", value: { effort: "xhigh" } }] },
      { suffix: "-effort-max", label: "effort: max", actions: [{ kind: "set", path: "output_config", value: { effort: "max" } }] },
    ],
  },
];

// ---------------------------------------------------------------------------
// OpenAI Response API
// ---------------------------------------------------------------------------

const OPENAI_RESPONSE_GROUPS: SuffixGroup[] = [
  {
    key: "thinking",
    label: "Reasoning",
    entries: [
      { suffix: "-thinking-none", label: "reasoning: none", actions: [{ kind: "set", path: "reasoning", value: { effort: "none" } }] },
      { suffix: "-thinking-low", label: "reasoning: low", actions: [{ kind: "set", path: "reasoning", value: { effort: "low" } }] },
      { suffix: "-thinking-medium", label: "reasoning: medium", actions: [{ kind: "set", path: "reasoning", value: { effort: "medium" } }] },
      { suffix: "-thinking-high", label: "reasoning: high", actions: [{ kind: "set", path: "reasoning", value: { effort: "high" } }] },
      { suffix: "-thinking-xhigh", label: "reasoning: xhigh", actions: [{ kind: "set", path: "reasoning", value: { effort: "xhigh" } }] },
    ],
  },
  {
    key: "tier",
    label: "Service Tier",
    entries: [
      { suffix: "-tier-auto", label: "service_tier: auto", actions: [{ kind: "set", path: "service_tier", value: "auto" }] },
      { suffix: "-tier-default", label: "service_tier: default", actions: [{ kind: "set", path: "service_tier", value: "default" }] },
      { suffix: "-tier-flex", label: "service_tier: flex", actions: [{ kind: "set", path: "service_tier", value: "flex" }] },
      { suffix: "-tier-scale", label: "service_tier: scale", actions: [{ kind: "set", path: "service_tier", value: "scale" }] },
      { suffix: "-tier-priority", label: "service_tier: priority", actions: [{ kind: "set", path: "service_tier", value: "priority" }] },
      { suffix: "-fast", label: "fast (= priority)", actions: [{ kind: "set", path: "service_tier", value: "priority" }] },
    ],
  },
  {
    key: "verbosity",
    label: "Verbosity",
    entries: [
      { suffix: "-effort-low", label: "verbosity: low", actions: [{ kind: "set", path: "text", value: { verbosity: "low" } }] },
      { suffix: "-effort-medium", label: "verbosity: medium", actions: [{ kind: "set", path: "text", value: { verbosity: "medium" } }] },
      { suffix: "-effort-high", label: "verbosity: high", actions: [{ kind: "set", path: "text", value: { verbosity: "high" } }] },
    ],
  },
];

// ---------------------------------------------------------------------------
// OpenAI Chat Completions
// ---------------------------------------------------------------------------

const OPENAI_CHAT_GROUPS: SuffixGroup[] = [
  {
    key: "thinking",
    label: "Reasoning",
    entries: [
      { suffix: "-thinking-none", label: "reasoning_effort: none", actions: [{ kind: "set", path: "reasoning_effort", value: "none" }] },
      { suffix: "-thinking-low", label: "reasoning_effort: low", actions: [{ kind: "set", path: "reasoning_effort", value: "low" }] },
      { suffix: "-thinking-medium", label: "reasoning_effort: medium", actions: [{ kind: "set", path: "reasoning_effort", value: "medium" }] },
      { suffix: "-thinking-high", label: "reasoning_effort: high", actions: [{ kind: "set", path: "reasoning_effort", value: "high" }] },
      { suffix: "-thinking-xhigh", label: "reasoning_effort: xhigh", actions: [{ kind: "set", path: "reasoning_effort", value: "xhigh" }] },
    ],
  },
  {
    key: "tier",
    label: "Service Tier",
    entries: [
      { suffix: "-tier-auto", label: "service_tier: auto", actions: [{ kind: "set", path: "service_tier", value: "auto" }] },
      { suffix: "-tier-default", label: "service_tier: default", actions: [{ kind: "set", path: "service_tier", value: "default" }] },
      { suffix: "-tier-flex", label: "service_tier: flex", actions: [{ kind: "set", path: "service_tier", value: "flex" }] },
      { suffix: "-tier-scale", label: "service_tier: scale", actions: [{ kind: "set", path: "service_tier", value: "scale" }] },
      { suffix: "-tier-priority", label: "service_tier: priority", actions: [{ kind: "set", path: "service_tier", value: "priority" }] },
      { suffix: "-fast", label: "fast (= priority)", actions: [{ kind: "set", path: "service_tier", value: "priority" }] },
    ],
  },
  {
    key: "verbosity",
    label: "Verbosity",
    entries: [
      { suffix: "-effort-low", label: "verbosity: low", actions: [{ kind: "set", path: "verbosity", value: "low" }] },
      { suffix: "-effort-medium", label: "verbosity: medium", actions: [{ kind: "set", path: "verbosity", value: "medium" }] },
      { suffix: "-effort-high", label: "verbosity: high", actions: [{ kind: "set", path: "verbosity", value: "high" }] },
    ],
  },
];

// ---------------------------------------------------------------------------
// Gemini
// ---------------------------------------------------------------------------

const GEMINI_GROUPS: SuffixGroup[] = [
  {
    key: "thinking",
    label: "Thinking",
    entries: [
      { suffix: "-thinking-none", label: "thinkingLevel: MINIMAL", actions: [{ kind: "set", path: "thinkingConfig", value: { thinkingLevel: "MINIMAL" } }] },
      { suffix: "-thinking-low", label: "thinkingLevel: LOW", actions: [{ kind: "set", path: "thinkingConfig", value: { thinkingLevel: "LOW" } }] },
      { suffix: "-thinking-medium", label: "thinkingLevel: MEDIUM", actions: [{ kind: "set", path: "thinkingConfig", value: { thinkingLevel: "MEDIUM" } }] },
      { suffix: "-thinking-high", label: "thinkingLevel: HIGH", actions: [{ kind: "set", path: "thinkingConfig", value: { thinkingLevel: "HIGH" } }] },
    ],
  },
];

export const SUFFIX_GROUPS_BY_PROTOCOL: Record<SuffixProtocol, SuffixGroup[]> = {
  claude: CLAUDE_GROUPS,
  openai_response: OPENAI_RESPONSE_GROUPS,
  openai_chat_completions: OPENAI_CHAT_GROUPS,
  gemini: GEMINI_GROUPS,
};

export const SUFFIX_PROTOCOL_LABELS: Record<SuffixProtocol, string> = {
  claude: "Claude (Anthropic)",
  openai_response: "OpenAI Responses API",
  openai_chat_completions: "OpenAI Chat Completions",
  gemini: "Gemini",
};

/// Guess the default protocol from a channel name. Falls back to openai_response.
export function suffixProtocolForChannel(channel: string | undefined): SuffixProtocol {
  switch (channel) {
    case "anthropic":
    case "claudecode":
      return "claude";
    case "aistudio":
    case "vertex":
    case "vertexexpress":
    case "geminicli":
    case "antigravity":
      return "gemini";
    default:
      return "openai_response";
  }
}
