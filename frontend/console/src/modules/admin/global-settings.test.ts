import { describe, expect, it } from "vitest";

import {
  normalizeUpdateChannel,
  normalizeUpdateSourceMode,
  resolveUpdateSourceValue,
  resolveUpdateTag,
} from "./global-settings";

describe("global settings helpers", () => {
  it("normalizes built-in and custom update sources with our current conventions", () => {
    expect(normalizeUpdateSourceMode("github")).toBe("github");
    expect(normalizeUpdateSourceMode("web")).toBe("web");
    expect(normalizeUpdateSourceMode("https://updates.example.com")).toBe("custom");
  });

  it("normalizes update channel to release or staging", () => {
    expect(normalizeUpdateChannel("staging")).toBe("staging");
    expect(normalizeUpdateChannel("release")).toBe("release");
    expect(normalizeUpdateChannel("releases")).toBe("release");
  });

  it("resolves saved update source and update tag", () => {
    expect(resolveUpdateSourceValue("github", "")).toBe("github");
    expect(resolveUpdateSourceValue("custom", "https://updates.example.com")).toBe(
      "https://updates.example.com",
    );
    expect(resolveUpdateTag("release")).toBeNull();
    expect(resolveUpdateTag("staging")).toBe("staging");
  });
});
