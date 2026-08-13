import { describe, expect, it } from "vitest";
import { channelMeta } from "@/lib/channel-meta";
import { isValidEndpointUrl } from "./endpoint-fields";
import { assembleSettings, initSettingsState } from "./settings-fields";

describe("endpoint settings", () => {
  it("defensively parses and serializes trimmed endpoint objects", () => {
    const base = {
      base_url: "https://fallback.example",
      untouched: { enabled: true },
      endpoints: {
        openai_responses: " https://api.example/v1/responses ",
        usage: 42,
        unknown_kind: "https://api.example/unknown",
      },
    };
    const meta = channelMeta("custom");
    const state = initSettingsState(base, meta);

    expect(state.endpoints).toEqual([
      { kind: "openai_responses", url: " https://api.example/v1/responses " },
    ]);
    expect(state.baseUrl).toBe("https://fallback.example");
    expect(assembleSettings(base, state, "custom", meta)).toEqual({
      base_url: "https://fallback.example",
      untouched: { enabled: true },
      endpoints: { openai_responses: "https://api.example/v1/responses" },
    });
    expect(initSettingsState({ endpoints: [] }, meta).endpoints).toEqual([]);
  });

  it("accepts HTTP path placeholders and rejects non-absolute URLs", () => {
    expect(isValidEndpointUrl("https://api.example/v1/{organization}/models/{model}")).toBe(true);
    expect(isValidEndpointUrl("http://localhost:8080/v1/models")).toBe(true);
    expect(isValidEndpointUrl("/v1/models")).toBe(false);
    expect(isValidEndpointUrl("ftp://api.example/v1/models")).toBe(false);
    expect(isValidEndpointUrl("https://api.example/v1/models?model={model}")).toBe(false);
  });

  it("stores custom magic cache switches independently", () => {
    const meta = channelMeta("custom");
    const state = initSettingsState({ enable_magic_cache: true }, meta);
    state.enableOpenAiMagicCache = true;
    state.enableClaudeMagicCache = false;

    expect(assembleSettings({ enable_magic_cache: true }, state, "custom", meta)).toEqual({
      enable_openai_magic_cache: true,
    });
  });

  it("stores default or ordered Claude fallback routing without a legacy switch", () => {
    const meta = channelMeta("claudeapi");
    const state = initSettingsState({}, meta);
    state.enableClaudeFableFallback = true;
    expect(assembleSettings({}, state, "claudeapi", meta)).toEqual({
      claude_fable_fallbacks: "default",
    });

    state.claudeFableFallbackModels = ["claude-opus-5", "claude-opus-4-8", "claude-opus-5"];
    expect(assembleSettings({}, state, "claudeapi", meta)).toEqual({
      claude_fable_fallbacks: ["claude-opus-5", "claude-opus-4-8"],
    });
  });

  it("stores the DeepSeek beta switch only for the DeepSeek channel", () => {
    const meta = channelMeta("deepseek");
    const state = initSettingsState({ enable_beta: true }, meta);
    expect(state.enableDeepSeekBeta).toBe(true);
    expect(assembleSettings({}, state, "deepseek", meta)).toEqual({ enable_beta: true });

    expect(assembleSettings({}, state, "openai", channelMeta("openai"))).toEqual({});
  });
});
