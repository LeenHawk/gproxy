import { describe, expect, it } from "vitest";

import { suffixGroupsForChannel, suffixProtocolForChannel } from "./suffix-presets";

describe("suffix presets", () => {
  it("adds Vercel gateway providerOptions source aliases only for Vercel", () => {
    const vercelGroups = suffixGroupsForChannel("openai_response", "vercel");
    const sourceGroup = vercelGroups.find((group) => group.key === "vercel_gateway_source");

    expect(suffixProtocolForChannel("vercel")).toBe("openai_response");
    expect(sourceGroup).toBeDefined();
    expect(sourceGroup?.entries[0]).toMatchObject({
      suffix: "-via-openai",
      actions: [
        {
          kind: "set",
          path: "providerOptions.gateway.only",
          value: ["openai"],
        },
      ],
    });

    const openAiGroups = suffixGroupsForChannel("openai_response", "openai");
    expect(openAiGroups.some((group) => group.key === "vercel_gateway_source")).toBe(false);
  });
});
