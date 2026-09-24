import { afterEach, describe, expect, it, vi } from "vitest"
import { api, setAdminScope } from "./client"
afterEach(() => { setAdminScope(null); vi.unstubAllGlobals() })
describe("management request scope", () => {
  it("sends the selected scope only to management routes", async () => {
    const fetch = vi.fn().mockResolvedValue(new Response("{}", { status: 200 }))
    vi.stubGlobal("fetch", fetch)
    setAdminScope("organization:o1")
    await api("/admin/api/credentials")
    expect(new Headers(fetch.mock.calls[0][1].headers).get("x-gproxy-admin-scope")).toBe("organization:o1")
    fetch.mockResolvedValue(new Response("{}", { status: 200 }))
    await api("/portal/api/context")
    expect(new Headers(fetch.mock.calls[1][1].headers).has("x-gproxy-admin-scope")).toBe(false)
  })
  it("aborts outstanding management requests when the scope changes", async () => {
    let signal: AbortSignal | undefined
    vi.stubGlobal("fetch", vi.fn((_path, init) => { signal = init.signal; return new Promise((_resolve, reject) => signal?.addEventListener("abort", () => reject(new DOMException("aborted", "AbortError")))) }))
    setAdminScope("organization:o1")
    const pending = api("/admin/api/credentials")
    const assertion = expect(pending).rejects.toThrow("aborted")
    setAdminScope("team:t1")
    await assertion
    expect(signal?.aborted).toBe(true)
  })
})
