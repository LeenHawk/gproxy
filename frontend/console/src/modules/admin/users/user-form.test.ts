import { describe, expect, it } from "vitest";

import { buildUserWritePayload } from "./types";

describe("buildUserWritePayload", () => {
  it("preserves blank password for edit submissions", () => {
    expect(
      buildUserWritePayload({
        id: "2",
        name: "alice",
        password: "",
        enabled: true,
        is_admin: false,
      }),
    ).toMatchObject({
      id: 2,
      name: "alice",
      password: "",
      enabled: true,
      is_admin: false,
    });
  });
});
