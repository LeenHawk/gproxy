import { describe, expect, it } from "vitest";

import { buildOAuthCallbackQuery } from "./oauth";

describe("providers oauth helpers", () => {
  it("wraps a pasted callback url as callback_url query param", () => {
    const query = buildOAuthCallbackQuery(
      "http://localhost:1455/auth/callback?code=abc123&state=xyz",
    );

    expect(query).toBe(
      "?callback_url=http%3A%2F%2Flocalhost%3A1455%2Fauth%2Fcallback%3Fcode%3Dabc123%26state%3Dxyz",
    );
  });

  it("rejects empty callback input", () => {
    expect(() => buildOAuthCallbackQuery("   ")).toThrow(/callback/i);
  });
});
