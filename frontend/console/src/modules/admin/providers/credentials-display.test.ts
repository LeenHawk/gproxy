import { describe, expect, it } from "vitest";

import { summarizeCredential } from "./credentials-display";

describe("providers credential display", () => {
  it("uses a short account id for codex-style credentials", () => {
    const summary = summarizeCredential({
      account_id: "fdc791c5-acf2-4760-b8e7-4af508952763",
      user_email: "chatgpt1001@lin.pub",
      access_token: "secret",
    });

    expect(summary.primary).toBe("fdc791c5");
    expect(summary.secondary).toContain("chatgpt1001@lin.pub");
  });

  it("uses project id without field labels for google oauth credentials", () => {
    const summary = summarizeCredential({
      project_id: "demo-project",
      user_email: "user@example.com",
      access_token: "secret",
    });

    expect(summary.primary).toBe("demo-project");
    expect(summary.secondary).toContain("user@example.com");
  });
});
