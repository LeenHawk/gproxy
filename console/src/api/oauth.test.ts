import { afterEach, describe, expect, it, vi } from "vitest"
import { ApiError } from "./client"
import { authorizeDecide, authorizeDetails } from "./oauth"
afterEach(() => { vi.unstubAllGlobals() })
describe("authorization endpoint", () => {
  it("carries the request's query to the issuer and asks for JSON", async () => {
    const fetch = vi.fn().mockResolvedValue(new Response(JSON.stringify({ client_name: "CLI" }), { status: 200 }))
    vi.stubGlobal("fetch", fetch)
    await authorizeDetails("?client_id=cli&state=xyz")
    expect(fetch.mock.calls[0][0]).toBe("/v1/oauth/authorize?client_id=cli&state=xyz")
    expect(new Headers(fetch.mock.calls[0][1].headers).get("accept")).toBe("application/json")
    expect(fetch.mock.calls[0][1].credentials).toBe("same-origin")
  })
  it("reads RFC 6749's error shape, not the product envelope", async () => {
    vi.stubGlobal("fetch", vi.fn().mockResolvedValue(new Response(
      JSON.stringify({ error: "invalid_request", error_description: "redirect_uri is not registered" }),
      { status: 400 },
    )))
    const failure = await authorizeDecide("?client_id=cli", "approve").catch((error: unknown) => error)
    expect(failure).toBeInstanceOf(ApiError)
    expect((failure as ApiError).code).toBe("invalid_request")
    expect((failure as ApiError).message).toBe("redirect_uri is not registered")
  })
})
