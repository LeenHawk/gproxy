import { expect, test } from "vitest"
import { oauthReturnUrl, validateCallbackUrl } from "./oauth-callback"

test("OAuth return URLs stay on the gateway after browser URL normalization", () => {
  const origin = "https://gateway.example"
  expect(oauthReturnUrl("/codex/oauth/authorize?state=example", origin))
    .toBe(`${origin}/codex/oauth/authorize?state=example`)
  expect(oauthReturnUrl("/codex/activate", origin)).toBe(`${origin}/codex/activate`)
  for (const value of [null, "", "//attacker.example", "/\\attacker.example", "/\n/attacker.example", "/\t/attacker.example", "https://attacker.example", "javascript:alert(1)", "//user@gateway.example"]) {
    expect(oauthReturnUrl(value, origin), String(value)).toBeNull()
  }
})

test("pasted OAuth callbacks need a code and the current authorization state", () => {
  const authorization = "https://accounts.example/authorize?state=current"
  expect(validateCallbackUrl("http://localhost:1455/auth/callback?code=example&state=current", authorization)).toBe(true)
  expect(validateCallbackUrl("http://localhost:1455/auth/callback?code=example&state=stale", authorization)).toBe(false)
  expect(validateCallbackUrl("http://localhost:1455/auth/callback?state=current", authorization)).toBe(false)
})
