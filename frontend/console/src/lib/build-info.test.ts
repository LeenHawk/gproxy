import { describe, expect, it } from "vitest";

import { normalizeBuildInfo } from "./build-info";

describe("normalizeBuildInfo", () => {
  it("falls back when version and commit are blank", () => {
    expect(normalizeBuildInfo("", "")).toEqual({
      version: "dev",
      commit: "unknown",
    });
  });

  it("preserves build version and short commit hash", () => {
    expect(normalizeBuildInfo("1.0.0", "abc1234")).toEqual({
      version: "1.0.0",
      commit: "abc1234",
    });
  });
});
