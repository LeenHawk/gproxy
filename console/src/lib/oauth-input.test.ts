import { describe, expect, it } from "vitest";
import { extractSessionKey, validateCallbackUrl } from "./oauth-input";

const AUTH = "https://claude.ai/oauth/authorize?client_id=x&state=abc&code_challenge=y";

describe("validateCallbackUrl", () => {
  it("accepts a real callback with code+state", () => {
    expect(validateCallbackUrl("https://platform.claude.com/oauth/code/callback?code=c1&state=abc", AUTH)).toBe(true);
  });
  it("rejects the authorize URL itself (wontfix guard)", () => {
    expect(validateCallbackUrl(AUTH + "&code=zzz", AUTH)).toBe(false);
  });
  it("rejects missing code or state, garbage, and empty", () => {
    expect(validateCallbackUrl("https://x.test/cb?code=c1", AUTH)).toBe(false);
    expect(validateCallbackUrl("https://x.test/cb?state=s1", AUTH)).toBe(false);
    expect(validateCallbackUrl("not a url", AUTH)).toBe(false);
    expect(validateCallbackUrl("", AUTH)).toBe(false);
  });
});

describe("extractSessionKey", () => {
  it("extracts from a full Cookie header dump", () => {
    expect(extractSessionKey("foo=1; sessionKey=sk-ant-sid01-AAA; bar=2")).toBe("sessionKey=sk-ant-sid01-AAA");
  });
  it("accepts sessionKey=… directly and bare sk-ant values", () => {
    expect(extractSessionKey("sessionKey=sk-ant-sid01-BBB")).toBe("sessionKey=sk-ant-sid01-BBB");
    expect(extractSessionKey("sk-ant-sid01-CCC")).toBe("sessionKey=sk-ant-sid01-CCC");
  });
  it("rejects input without a sessionKey", () => {
    expect(extractSessionKey("foo=1; bar=2")).toBeNull();
    expect(extractSessionKey("")).toBeNull();
  });
});
