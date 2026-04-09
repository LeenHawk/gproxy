import { describe, expect, it } from "vitest";

import { buildQuotaIncrementPayload } from "./quota";

describe("buildQuotaIncrementPayload", () => {
  it("adds a fixed increment on top of the current quota", () => {
    expect(
      buildQuotaIncrementPayload(
        {
          user_id: 7,
          quota: 100,
          cost_used: 6.5,
        },
        100,
      ),
    ).toEqual({
      user_id: 7,
      quota: 200,
      cost_used: 6.5,
    });
  });

  it("supports arbitrary decimal increments", () => {
    expect(
      buildQuotaIncrementPayload(
        {
          user_id: 9,
          quota: 12.75,
          cost_used: 2.5,
        },
        "7.25",
      ),
    ).toEqual({
      user_id: 9,
      quota: 20,
      cost_used: 2.5,
    });
  });
});
