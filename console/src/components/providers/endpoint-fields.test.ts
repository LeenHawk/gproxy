import { describe, expect, it } from "vitest";
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
    const state = initSettingsState(base, "custom");

    expect(state.endpoints).toEqual([
      { kind: "openai_responses", url: " https://api.example/v1/responses " },
    ]);
    expect(state.baseUrl).toBe("https://fallback.example");
    expect(assembleSettings(base, state, "custom")).toEqual({
      base_url: "https://fallback.example",
      untouched: { enabled: true },
      endpoints: { openai_responses: "https://api.example/v1/responses" },
    });
    expect(initSettingsState({ endpoints: [] }, "custom").endpoints).toEqual([]);
  });

  it("accepts HTTP path placeholders and rejects non-absolute URLs", () => {
    expect(isValidEndpointUrl("https://api.example/v1/{organization}/models/{model}")).toBe(true);
    expect(isValidEndpointUrl("http://localhost:8080/v1/models")).toBe(true);
    expect(isValidEndpointUrl("/v1/models")).toBe(false);
    expect(isValidEndpointUrl("ftp://api.example/v1/models")).toBe(false);
    expect(isValidEndpointUrl("https://api.example/v1/models?model={model}")).toBe(false);
  });
});
