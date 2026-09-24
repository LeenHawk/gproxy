import { describe, expect, it } from "vitest"
import { consoleContext, SELF_READ, SELF_KEYS_WRITE, SELF_PASSWORD_CHANGE, SELF_LOGS_READ } from "./capability"
import { adminContext, portalContext, orgScope } from "@/test/context"
describe("server-derived management capabilities", () => {
  it("does not infer gateway access from a role or membership", () => {
    const derived = consoleContext(portalContext({ user: { id: "u1", name: "root", role: "admin", hasPassword: true }, organizations: [{ id: "o1", name: "Acme", role: "admin" }] }))
    expect(derived.has(SELF_READ)).toBe(true)
    expect(derived.has("configuration.providers")).toBe(false)
    expect(derived.scopes).toEqual([])
  })
  it("uses only the sections granted in the selected scope", () => {
    const derived = consoleContext({ ...portalContext(), admin: adminContext(["credentials", "quotas"], orgScope) })
    expect(derived.has("configuration.credentials")).toBe(true)
    expect(derived.has("configuration.quotas")).toBe(true)
    expect(derived.has("configuration.providers")).toBe(false)
    expect(derived.has("identity.users")).toBe(false)
    expect(derived.scope?.selector).toBe("organization:o1")
  })
  it("keeps a required scope choice unresolved", () => {
    const admin = adminContext([], orgScope)
    admin.scope = null
    const derived = consoleContext({ ...portalContext(), admin })
    expect(derived.scope).toBeNull()
    expect(derived.scopes).toEqual([orgScope])
    expect(derived.has("configuration.credentials")).toBe(false)
  })
  it("preserves portal feature gates independently of admin sections", () => {
    const derived = consoleContext({ ...portalContext({ features: { canCreateKeys: false, canChangePassword: false, canSeeLogs: false, canSeeConsole: true } }), admin: adminContext(["users"]) })
    expect(derived.has("identity.users")).toBe(true)
    for (const capability of [SELF_KEYS_WRITE, SELF_PASSWORD_CHANGE, SELF_LOGS_READ]) expect(derived.has(capability)).toBe(false)
  })
})
