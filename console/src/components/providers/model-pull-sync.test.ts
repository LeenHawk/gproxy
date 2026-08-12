import { describe, expect, it } from "vitest";
import type { ProviderModel, UpstreamModel } from "@/api/provider-models";
import { defaultPriceInput, missingMetadataCount, modelSyncInput } from "./model-pull-sync";

const upstream: UpstreamModel = {
  id: "openai/gpt-4.1",
  display_name: "GPT 4.1 upstream",
  context_window: 128_000,
  max_input_tokens: null,
  max_output_tokens: 16_384,
  thinking_supported: true,
  thinking_adaptive_supported: null,
  thinking_enabled_supported: false,
};

const existing: ProviderModel = {
  id: 7,
  provider_id: 3,
  model_id: upstream.id,
  display_name: "My GPT",
  variants_json: ["gpt-fast"],
  context_window: null,
  max_input_tokens: 100_000,
  max_output_tokens: null,
  thinking_supported: false,
  thinking_adaptive_supported: null,
  thinking_enabled_supported: null,
  enabled: false,
  created_at: 1,
  updated_at: 2,
};

describe("model pull sync", () => {
  it("fills only missing metadata and preserves local policy fields", () => {
    expect(missingMetadataCount(existing, upstream)).toBe(3);
    expect(modelSyncInput(3, upstream, existing)).toEqual({
      id: 7,
      provider_id: 3,
      model_id: upstream.id,
      display_name: "My GPT",
      variants_json: ["gpt-fast"],
      context_window: 128_000,
      max_input_tokens: 100_000,
      max_output_tokens: 16_384,
      thinking_supported: false,
      thinking_adaptive_supported: null,
      thinking_enabled_supported: false,
      enabled: false,
    });
  });

  it("creates an exact provider price without overwriting an existing exact rule", () => {
    const input = defaultPriceInput(3, upstream.id, []);
    expect(input).toMatchObject({
      id: null,
      provider_id: 3,
      match_type: "exact",
      model_match: upstream.id,
    });
    expect(defaultPriceInput(3, upstream.id, [{
      ...input!,
      id: 9,
      provider_id: 3,
      created_at: 1,
      updated_at: 1,
    }])).toBeUndefined();
  });
});
