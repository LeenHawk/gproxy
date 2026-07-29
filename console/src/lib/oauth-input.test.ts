import { describe, expect, it } from "vitest";
import { normalizeCookieInput, validateCallbackUrl } from "./oauth-input";

const AUTH = "https://claude.ai/oauth/authorize?client_id=x&state=abc&code_challenge=y";

describe("validateCallbackUrl", () => {
  it("accepts a real callback with code+state", () => {
    expect(validateCallbackUrl("https://platform.claude.com/oauth/code/callback?code=c1&state=abc", AUTH)).toBe(true);
  });
  it("rejects the authorize URL itself", () => {
    expect(validateCallbackUrl(AUTH + "&code=zzz", AUTH)).toBe(false);
  });
  it("rejects a callback from a different authorization session", () => {
    expect(validateCallbackUrl("https://platform.claude.com/oauth/code/callback?code=c1&state=old", AUTH)).toBe(false);
  });
  it("rejects missing code or state, garbage, and empty", () => {
    expect(validateCallbackUrl("https://x.test/cb?code=c1", AUTH)).toBe(false);
    expect(validateCallbackUrl("https://x.test/cb?state=s1", AUTH)).toBe(false);
    expect(validateCallbackUrl("not a url", AUTH)).toBe(false);
    expect(validateCallbackUrl("", AUTH)).toBe(false);
    expect(validateCallbackUrl("https://x.test/cb?code=c1&state=abc", "not a url")).toBe(false);
  });
});

describe("normalizeCookieInput", () => {
  const claude = { id: "claudeweb", source: "builtin" } as const;

  it("preserves a complete Claude Cookie header", () => {
    expect(normalizeCookieInput(
      "Cookie: cf_clearance=clear; sessionKey=sk-ant-sid01-AAA; __cf_bm=bm",
      claude,
    )).toBe("cf_clearance=clear; sessionKey=sk-ant-sid01-AAA; __cf_bm=bm");
  });
  it("accepts sessionKey directly and normalizes a bare Claude session key", () => {
    expect(normalizeCookieInput("sessionKey=sk-ant-sid01-BBB", claude)).toBe("sessionKey=sk-ant-sid01-BBB");
    expect(normalizeCookieInput("sk-ant-sid01-CCC", claude)).toBe("sessionKey=sk-ant-sid01-CCC");
  });
  it("rejects a Claude input without a sessionKey", () => {
    expect(normalizeCookieInput("foo=1; bar=2", claude)).toBeNull();
    expect(normalizeCookieInput("", claude)).toBeNull();
  });
  it("accepts an external channel's nonempty cookie as opaque text", () => {
    const external = { id: "acme", source: "external" } as const;
    expect(normalizeCookieInput("opaque signed cookie", external)).toBe("opaque signed cookie");
    expect(normalizeCookieInput("Cookie: signed=value", external)).toBe("Cookie: signed=value");
  });
});
