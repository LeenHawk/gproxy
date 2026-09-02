import type { VariantPresetGroup } from "@/components/providers/variant-presets/types"

export const CLAUDE_PRESETS: Array<VariantPresetGroup> = [
  {
    key: "thinking",
    label: "Thinking",
    entries: [
      { suffix: "-thinking-none", label: "thinking: disabled", actions: [{ path: "thinking", value: { type: "disabled" } }] },
      { suffix: "-thinking-low", label: "thinking: low (1024 tokens)", actions: [{ path: "thinking", value: { type: "enabled", budget_tokens: 1024, display: "summarized" } }] },
      { suffix: "-thinking-medium", label: "thinking: medium (10240 tokens)", actions: [{ path: "thinking", value: { type: "enabled", budget_tokens: 10240, display: "summarized" } }] },
      { suffix: "-thinking-high", label: "thinking: high (32768 tokens)", actions: [{ path: "thinking", value: { type: "enabled", budget_tokens: 32768, display: "summarized" } }] },
      { suffix: "-thinking-adaptive-omitted", label: "thinking: adaptive, omitted", actions: [{ path: "thinking", value: { type: "adaptive", display: "omitted" } }] },
      { suffix: "-thinking-adaptive", label: "thinking: adaptive, summarized", actions: [{ path: "thinking", value: { type: "adaptive", display: "summarized" } }] },
      { suffix: "-thinking-adaptive-updates", label: "thinking: adaptive, updates", actions: [{ path: "thinking", value: { type: "adaptive", display: "updates" } }] },
    ],
  },
  {
    key: "effort",
    label: "Effort",
    entries: ["low", "medium", "high", "xhigh", "max"].map((effort) => ({
      suffix: `-effort-${effort}`,
      label: `effort: ${effort}`,
      actions: [{ path: "output_config", value: { effort } }],
    })),
  },
]

export const GEMINI_PRESETS: Array<VariantPresetGroup> = [{
  key: "thinking",
  label: "Thinking",
  entries: [
    ["none", "MINIMAL"],
    ["low", "LOW"],
    ["medium", "MEDIUM"],
    ["high", "HIGH"],
  ].map(([suffix, level]) => ({
    suffix: `-thinking-${suffix}`,
    label: `thinkingLevel: ${level}`,
    actions: [{ path: "generationConfig.thinkingConfig", value: { thinkingLevel: level } }],
  })),
}]
