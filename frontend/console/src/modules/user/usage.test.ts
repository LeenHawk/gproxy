import { describe, expect, it } from "vitest";

import { buildMyUsageQuery, summarizeUsageRows } from "./usage";

describe("my usage helpers", () => {
  it("builds a fully scoped usage query so the backend can deserialize it", () => {
    expect(buildMyUsageQuery(100)).toEqual({
      provider_id: "All",
      credential_id: "All",
      channel: "All",
      model: "All",
      user_id: "All",
      user_key_id: "All",
      offset: 100,
      limit: 50,
    });
  });

  it("summarizes displayed rows and token totals", () => {
    expect(
      summarizeUsageRows([
        {
          trace_id: 11,
          at: "2026-04-08T00:00:00Z",
          operation: "chat",
          protocol: "responses",
          input_tokens: 120,
          output_tokens: 80,
          cache_read_input_tokens: 20,
          cache_creation_input_tokens: 5,
          cache_creation_input_tokens_5min: 6,
          cache_creation_input_tokens_1h: 7,
        },
        {
          trace_id: 12,
          at: "2026-04-08T00:01:00Z",
          operation: "chat",
          protocol: "responses",
          input_tokens: 40,
          output_tokens: 60,
          cache_read_input_tokens: 3,
          cache_creation_input_tokens: 0,
          cache_creation_input_tokens_5min: 0,
          cache_creation_input_tokens_1h: 10,
        },
      ]),
    ).toEqual({
      displayed: 2,
      inputTokens: 160,
      outputTokens: 140,
      cachedTokens: 51,
    });
  });
});
