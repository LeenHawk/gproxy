import { describe, expect, it } from "vitest";
import { inferSuffixSelection } from "./selection";

describe("inferSuffixSelection", () => {
  it("restores matching controls and preserves custom actions", () => {
    const selection = inferSuffixSelection("openrouter", [
      { path: "reasoning", value: { effort: "high" } },
      { path: "service_tier", value: "priority" },
      { path: "provider.only", value: ["anthropic", "google-vertex/us-east5"] },
      { path: "metadata.custom", value: true },
    ]);

    expect(selection.protocol).toBe("openai_response");
    expect(selection.picks).toMatchObject({ thinking: "3", tier: "4" });
    expect(selection.upstream).toBe("anthropic, google-vertex/us-east5");
    expect(selection.preservedActions).toEqual([{ path: "metadata.custom", value: true }]);
  });

  it("infers a non-default protocol from its persisted action", () => {
    const selection = inferSuffixSelection("openai", [
      { path: "reasoning_effort", value: "medium" },
    ]);

    expect(selection.protocol).toBe("openai_chat_completions");
    expect(selection.picks.thinking).toBe("2");
    expect(selection.preservedActions).toEqual([]);
  });
});
