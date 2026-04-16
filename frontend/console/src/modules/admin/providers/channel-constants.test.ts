import { describe, expect, it } from "vitest";

import { parseBetaHeaders } from "./channel-constants";

describe("parseBetaHeaders", () => {
  it("parses JSON array strings", () => {
    expect(parseBetaHeaders('["prompt-caching-2024-07-31","files-api-2025-04-14"]')).toEqual([
      "prompt-caching-2024-07-31",
      "files-api-2025-04-14",
    ]);
  });

  it("rejects non-JSON-array strings", () => {
    expect(parseBetaHeaders("prompt-caching-2024-07-31,files-api-2025-04-14")).toEqual([]);
    expect(parseBetaHeaders("not json")).toEqual([]);
    expect(parseBetaHeaders('{"beta":"prompt-caching-2024-07-31"}')).toEqual([]);
  });
});
