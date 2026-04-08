import { describe, expect, it } from "vitest";

import { buildUserQuotaFormState, buildUserQuotaWritePayload } from "./quota";

describe("user quota helpers", () => {
  it("builds form state from an existing quota row", () => {
    expect(
      buildUserQuotaFormState({
        quota: 25.5,
        cost_used: 6.25,
      }),
    ).toEqual({
      quota: "25.5",
      cost_used: "6.25",
    });
  });

  it("treats blank fields as zero when building an upsert payload", () => {
    expect(
      buildUserQuotaWritePayload(7, {
        quota: "",
        cost_used: "",
      }),
    ).toEqual({
      user_id: 7,
      quota: 0,
      cost_used: 0,
    });
  });

  it("parses decimal quota fields for the save payload", () => {
    expect(
      buildUserQuotaWritePayload(9, {
        quota: "12.75",
        cost_used: "2.5",
      }),
    ).toEqual({
      user_id: 9,
      quota: 12.75,
      cost_used: 2.5,
    });
  });
});
