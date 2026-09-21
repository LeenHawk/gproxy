import { describe, expect, it } from "vitest"
import { consoleContext, IDENTITY_FAMILIES, SELF_KEYS_WRITE, SELF_LOGS_READ, SELF_PASSWORD_CHANGE, SELF_READ } from "@/capability/capability"
import type { PortalContextDto } from "@/generated/app"

function context(overrides: Partial<PortalContextDto> = {}): PortalContextDto {
  return {
    user: { id: "u1", name: "ada", role: "user", hasPassword: true },
    organizations: [],
    teams: [],
    subscription: null,
    features: { canCreateKeys: true, canChangePassword: true, canSeeLogs: true, canSeeConsole: false },
    ...overrides,
  }
}

describe("the capability adapter", () => {
  it("gives every caller the self-service surface and nothing else by default", () => {
    const derived = consoleContext(context())
    expect(derived.has(SELF_READ)).toBe(true)
    expect(derived.has("identity.users")).toBe(false)
    expect(derived.scopes).toEqual([])
    expect(derived.scope).toBeNull()
  })

  it("gives an instance administrator every identity family", () => {
    const derived = consoleContext(context({
      user: { id: "u1", name: "root", role: "admin", hasPassword: true },
    }))
    for (const family of IDENTITY_FAMILIES) expect(derived.has(`identity.${family}`)).toBe(true)
    expect(derived.scope).toEqual({ kind: "instance" })
  })

  // The seam's whole point: a scope administrator is *recognised* without
  // being shown pages that `/admin/api` would refuse them today.
  it("recognises a scope administrator without granting an identity family", () => {
    const derived = consoleContext(context({
      organizations: [{ id: "o1", name: "Acme", role: "admin" }],
      teams: [{ id: "t1", name: "Platform", organizationId: "o1", role: "member" }],
    }))
    expect(derived.scopes).toEqual([{ kind: "organization", id: "o1", name: "Acme" }])
    expect(derived.has("identity.users")).toBe(false)
    expect(derived.has("identity.teams")).toBe(false)
  })

  it("reads the three self-service facts off the feature flags", () => {
    const derived = consoleContext(context({
      features: { canCreateKeys: false, canChangePassword: false, canSeeLogs: false, canSeeConsole: true },
    }))
    expect(derived.has(SELF_KEYS_WRITE)).toBe(false)
    expect(derived.has(SELF_PASSWORD_CHANGE)).toBe(false)
    expect(derived.has(SELF_LOGS_READ)).toBe(false)
    expect(derived.has(SELF_READ)).toBe(true)
  })

  it("puts the instance scope first, then organizations, then teams", () => {
    const derived = consoleContext(context({
      user: { id: "u1", name: "root", role: "admin", hasPassword: true },
      organizations: [{ id: "o1", name: "Acme", role: "admin" }],
      teams: [{ id: "t1", name: "Platform", organizationId: "o1", role: "admin" }],
    }))
    expect(derived.scopes.map((scope) => scope.kind)).toEqual(["instance", "organization", "team"])
  })
})
