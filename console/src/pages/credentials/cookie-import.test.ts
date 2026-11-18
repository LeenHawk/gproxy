import { describe, expect, it } from "vitest"
import { parseBulkCredentials } from "./bulk-parse"
import { parsePlaygroundCookie } from "./cookie-import"

describe("Playground cookie import", () => {
  it("imports a Cookie-Editor array as one credential, preserving HttpOnly cookies", () => {
    const cookies = [
      { name: "SAPISID", value: "fixture-signing", domain: ".google.com" },
      { name: "__Secure-1PSID", value: "fixture-session", domain: ".google.com", httpOnly: true },
    ]
    const parsed = parseBulkCredentials(JSON.stringify(cookies), "cookies")
    expect(parsed.errors).toEqual([])
    expect(parsed.items).toEqual([{ label: null, secret: { cookies }, source: 1 }])
    expect(parsePlaygroundCookie("Cookie: SID=fixture; SAPISID=fixture")).toEqual({ cookie: "SID=fixture; SAPISID=fixture" })
  })

  it("keeps the selected Google identity and rejects malformed input without echoing it", () => {
    expect(parsePlaygroundCookie('{"cookie":"SID=x; SAPISID=y","auth_user":"1"}').auth_user).toBe("1")
    expect(() => parsePlaygroundCookie('[{"secret-value"')).toThrow("Invalid cookie JSON")
    expect(parseBulkCredentials('[{"secret-value"', "cookies").errors).toEqual([{ source: 1, code: "invalidSecret" }])
  })
})
