import { describe, expect, it } from "vitest";

import { buildDownstreamRequestQuery } from "./requests-filter";

describe("buildDownstreamRequestQuery", () => {
  it("builds the downstream request filter payload", () => {
    expect(
      buildDownstreamRequestQuery({
        request_path_contains: "/v1/responses",
        limit: "50",
        include_body: true,
      }),
    ).toMatchObject({
      request_path_contains: "/v1/responses",
      limit: 50,
      include_body: true,
    });
  });
});
