import { describe, expect, it } from "vitest";

import { normalizeUpdateChannel } from "./global-settings";

describe("global settings helpers", () => {
  it("normalizes update channel to release or staging", () => {
    expect(normalizeUpdateChannel("staging")).toBe("staging");
    expect(normalizeUpdateChannel("release")).toBe("release");
    expect(normalizeUpdateChannel("releases")).toBe("release");
  });
});
