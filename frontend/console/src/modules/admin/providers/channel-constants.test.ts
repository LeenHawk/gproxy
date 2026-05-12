import { describe, expect, it } from "vitest";

import { parseBetaHeaders, parseCacheBreakpoints } from "./channel-constants";

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

describe("parseCacheBreakpoints", () => {
  it("normalizes API TTL tags back to console TTL values", () => {
    expect(
      parseCacheBreakpoints([
        { target: "messages", position: "last_nth", index: 1, ttl: "ttl5m" },
        { target: "system", position: "last_nth", index: 1, ttl: "ttl1h" },
      ]),
    ).toEqual([
      { target: "messages", position: "last_nth", index: 1, ttl: "5m" },
      { target: "system", position: "last_nth", index: 1, ttl: "1h" },
    ]);
  });
});
