import type { VariantPresetGroup } from "@/components/providers/variant-presets/types"

const thinking = (path: "reasoning" | "reasoning_effort"): VariantPresetGroup => ({
  key: "thinking",
  label: "Reasoning",
  entries: ["none", "low", "medium", "high", "xhigh"].map((effort) => ({
    suffix: `-thinking-${effort}`,
    label: `${path}: ${effort}`,
    actions: [{ path, value: path === "reasoning" ? { effort } : effort }],
  })),
})

const tier: VariantPresetGroup = {
  key: "tier",
  label: "Service Tier",
  entries: [
    ...["auto", "default", "flex", "scale", "priority"].map((value) => ({
      suffix: `-tier-${value}`,
      label: `service_tier: ${value}`,
      actions: [{ path: "service_tier", value }],
    })),
    { suffix: "-fast", label: "fast (= priority)", actions: [{ path: "service_tier", value: "priority" }] },
  ],
}

const verbosity = (path: "text" | "verbosity"): VariantPresetGroup => ({
  key: "verbosity",
  label: "Verbosity",
  entries: ["low", "medium", "high"].map((value) => ({
    suffix: `-effort-${value}`,
    label: `verbosity: ${value}`,
    actions: [{ path, value: path === "text" ? { verbosity: value } : value }],
  })),
})

const tools: VariantPresetGroup = {
  key: "tool",
  label: "Forced Tool",
  entries: [
    {
      suffix: "-image-generate",
      label: "force image_generation (generate)",
      actions: [
        { path: "tools", value: [{ type: "image_generation", action: "generate" }] },
        { path: "tool_choice", value: { type: "image_generation" } },
      ],
    },
    {
      suffix: "-image-edit",
      label: "force image_generation (edit)",
      actions: [
        { path: "tools", value: [{ type: "image_generation", action: "edit" }] },
        { path: "tool_choice", value: { type: "image_generation" } },
      ],
    },
    { suffix: "-search", label: "force web_search_preview", actions: [{ path: "tools", value: [{ type: "web_search_preview" }] }, { path: "tool_choice", value: { type: "web_search_preview" } }] },
    { suffix: "-deep-research", label: "force deep_research", actions: [{ path: "tools", value: [{ type: "deep_research" }] }, { path: "tool_choice", value: { type: "deep_research" } }] },
  ],
}

export const OPENAI_RESPONSES_PRESETS = [thinking("reasoning"), tier, verbosity("text"), tools]
export const OPENAI_CHAT_PRESETS = [thinking("reasoning_effort"), tier, verbosity("verbosity")]
