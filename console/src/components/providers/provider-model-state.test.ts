import { describe, expect, it } from "vitest"
import { providerModelRequest, providerModelState } from "@/components/providers/provider-model-state"
import type { ProviderModelDto } from "@/generated/ProviderModelDto"

describe("provider model metadata state", () => {
  it("preserves unknown and explicitly empty structured collections", () => {
    const model = {
      id: 1,
      provider_id: 2,
      model_id: "gpt-test",
      display_name: null,
      variants: null,
      context_window: null,
      max_output_tokens: null,
      thinking_supported: null,
      thinking_adaptive_supported: null,
      thinking_enabled_supported: null,
      metadata: {
        ...providerModelState(null).metadata,
        input_modalities: [],
        output_modalities: null,
        reasoning_levels: [{ effort: "high", description: "Deep reasoning" }],
      },
      enabled: true,
    } satisfies ProviderModelDto

    const request = providerModelRequest(providerModelState(model))
    expect(request.metadata.input_modalities).toEqual([])
    expect(request.metadata.output_modalities).toBeNull()
    expect(request.metadata.reasoning_levels?.[0].effort).toBe("high")
  })
})
