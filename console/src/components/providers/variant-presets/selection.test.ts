import { describe, expect, it } from "vitest"
import { GATEWAY_SOURCE_BY_CHANNEL, variantGroups } from "@/components/providers/variant-presets"
import { inferVariantSelection } from "@/components/providers/variant-presets/selection"

describe("model variant presets", () => {
  it("keeps the complete protocol and gateway preset catalog", () => {
    expect(variantGroups("openai_responses", "openai").map((group) => group.entries.length)).toEqual([5, 6, 3, 4])
    expect(variantGroups("openai_chat", "openai").map((group) => group.entries.length)).toEqual([5, 6, 3])
    expect(variantGroups("claude", "claudeapi").map((group) => group.entries.length)).toEqual([7, 5])
    expect(variantGroups("gemini", "aistudio").map((group) => group.entries.length)).toEqual([4])
    expect(GATEWAY_SOURCE_BY_CHANNEL.openrouter.entries).toHaveLength(15)
    expect(GATEWAY_SOURCE_BY_CHANNEL.vercel.entries).toHaveLength(11)
  })

  it("restores picker controls while retaining custom actions", () => {
    const selection = inferVariantSelection("openrouter", [
      { path: "reasoning", value: { effort: "high" } },
      { path: "service_tier", value: "priority" },
      { path: "provider.only", value: ["anthropic", "google-vertex/us-east5"] },
      { path: "metadata.custom", value: true },
    ])
    expect(selection.protocol).toBe("openai_responses")
    expect(selection.picks).toMatchObject({ thinking: "3", tier: "4" })
    expect(selection.upstream).toBe("anthropic, google-vertex/us-east5")
    expect(selection.preserved).toEqual([{ path: "metadata.custom", value: true }])
  })

  it("writes Gemini thinking beneath generationConfig", () => {
    const minimal = variantGroups("gemini", "aistudio")[0].entries[0]
    expect(minimal.suffix).toBe("-thinking-none")
    expect(minimal.actions).toEqual([{ path: "generationConfig.thinkingConfig", value: { thinkingLevel: "MINIMAL" } }])
  })

  it("offers every Claude adaptive thinking display mode", () => {
    const adaptive = variantGroups("claude", "claudeapi")[0].entries.slice(-3)
    expect(adaptive.map((entry) => entry.actions[0].value)).toEqual([
      { type: "adaptive", display: "omitted" },
      { type: "adaptive", display: "summarized" },
      { type: "adaptive", display: "updates" },
    ])
  })
})
