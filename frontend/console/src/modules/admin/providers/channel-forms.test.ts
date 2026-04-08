import { describe, expect, it } from "vitest";

import { buildChannelSettingsJson } from "./channel-forms";

describe("buildChannelSettingsJson", () => {
  it("builds openai settings from structured form values", () => {
    const result = buildChannelSettingsJson("openai", {
      base_url: "https://api.openai.com",
      user_agent: "",
    });
    expect(result).toEqual({ base_url: "https://api.openai.com", user_agent: "" });
  });
});
