import { describe, expect, it } from "vitest"
import { consoleContext } from "@/capability/capability"
import { mayEnter, sectionsFor } from "@/capability/navigation"
import { adminContext, portalContext, orgScope } from "@/test/context"
import type { PortalContextDto } from "@/generated/app"

function derived(role: string, canSeeLogs = true) {
  const context: PortalContextDto = {
    user: { id: "u1", name: "ada", role, hasPassword: true },
    organizations: [],
    teams: [],
    features: { canCreateKeys: true, canChangePassword: true, canSeeLogs, canSeeConsole: false },
  }
  return consoleContext({ ...context, admin: role === "admin" ? adminContext(["users", "organizations", "teams", "api-keys", "permissions", "rate-limits", "oauth-clients", "sessions", "audit", "providers", "credentials", "quotas", "routes", "price-rules", "transfer", "settings", "connection-profiles"]) : null })
}

describe("the navigation", () => {
  it("gives scoped administrators object-context credential and budget entry points only", () => {
    const scoped = consoleContext({ ...portalContext(), admin: adminContext(["credentials", "quotas"], orgScope) })
    const paths = sectionsFor(scoped).flatMap(section => section.items.map(item => item.route))
    expect(paths).toContain("/providers")
    expect(paths).toContain("/identity/organizations")
    expect(paths).toContain("/identity/teams")
    expect(paths).not.toContain("/identity/users")
    expect(paths).not.toContain("/credentials")
    expect(paths).not.toContain("/quotas")
    expect(mayEnter(scoped, "/providers/p1/credentials")).toBe(true)
    expect(mayEnter(scoped, "/providers/p1/settings")).toBe(false)
    expect(mayEnter(scoped, "/providers/p1/models")).toBe(false)
    const team = consoleContext({ ...portalContext(), admin: adminContext(["credentials", "quotas"], { ...orgScope, kind: "team", id: "t1", selector: "team:t1" }) })
    expect(mayEnter(team, "/identity/teams")).toBe(true)
    expect(mayEnter(team, "/identity/organizations")).toBe(false)
  })

  // `canSeeLogs` is an instance setting, not a role: the item disappears
  // rather than rendering a page that answers with nothing.
  it("drops the recent-requests item when the instance has the log off", () => {
    const items = sectionsFor(derived("user", false))[0].items.map((item) => item.id)
    expect(items).not.toContain("requests")
    expect(items).toContain("overview")
    expect(items).not.toContain("quota")
  })

  it("refuses a route the caller has no capability for", () => {
    expect(mayEnter(derived("user"), "/identity/users")).toBe(false)
    expect(mayEnter(derived("admin"), "/identity/users")).toBe(true)
    expect(mayEnter(derived("user"), "/keys")).toBe(true)
    expect(mayEnter(derived("user"), "/settings")).toBe(false)
    expect(mayEnter(derived("user"), "/tokenizer")).toBe(false)
    expect(mayEnter(derived("admin"), "/tokenizer")).toBe(true)
    expect(mayEnter(derived("user"), "/providers/p1/models")).toBe(false)
    expect(mayEnter(derived("admin"), "/providers/p1/models")).toBe(true)
  })

  it("leaves an undeclared route to the not-found page rather than forbidding it", () => {
    expect(mayEnter(derived("user"), "/nowhere")).toBe(true)
  })
})
