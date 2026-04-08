import { describe, expect, it } from "vitest";

import { parseApiError } from "./api";

describe("parseApiError", () => {
  it("reads gproxy error JSON", async () => {
    const response = new Response(JSON.stringify({ error: "admin access required" }), {
      status: 403,
      headers: { "content-type": "application/json" },
    });
    await expect(parseApiError(response)).resolves.toMatchObject({
      status: 403,
      message: "admin access required",
    });
  });
});
