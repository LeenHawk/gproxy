import { describe, expect, it } from "vitest";

import { parseLiveUsageRows, supportsCredentialUsageChannel } from "./usage";

describe("providers usage helpers", () => {
  it("recognizes channels with credential quota support", () => {
    expect(supportsCredentialUsageChannel("codex")).toBe(true);
    expect(supportsCredentialUsageChannel("claudecode")).toBe(true);
    expect(supportsCredentialUsageChannel("geminicli")).toBe(true);
    expect(supportsCredentialUsageChannel("antigravity")).toBe(true);
    expect(supportsCredentialUsageChannel("kiro")).toBe(true);
    expect(supportsCredentialUsageChannel("openai")).toBe(false);
  });

  it("parses codex quota windows into live rows", () => {
    const rows = parseLiveUsageRows("codex", {
      rate_limit: {
        primary_window: {
          used_percent: 25,
          reset_at: "2026-04-08T12:00:00Z",
        },
        secondary_window: {
          used_percent: 40,
          reset_at: "2026-04-08T18:00:00Z",
        },
      },
    });

    expect(rows).toEqual([
      { name: "primary", percent: 25, resetAt: "2026-04-08T12:00:00Z" },
      { name: "secondary", percent: 40, resetAt: "2026-04-08T18:00:00Z" },
    ]);
  });

  it("parses kiro usage breakdown rows", () => {
    const rows = parseLiveUsageRows("kiro", {
      usageBreakdownList: [
        {
          displayName: "Credit",
          currentUsageWithPrecision: 0.02,
          usageLimitWithPrecision: 50,
          nextDateReset: 1780272000,
        },
      ],
    });

    expect(rows).toEqual([{ name: "Credit", percent: 0.04, resetAt: 1780272000000 }]);
  });
});
