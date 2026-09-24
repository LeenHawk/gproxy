import { describe, expect, it } from "vitest"
import { consoleContext } from "@/capability/capability"
import { mayEnter, sectionsFor } from "@/capability/navigation"
import { adminContext } from "@/test/context"
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
  it("shows an ordinary account only its own section", () => {
    const sections = sectionsFor(derived("user"))
    expect(sections.map((section) => section.id)).toEqual(["self"])
  })

  it("groups operator navigation by task", () => {
    const sections = sectionsFor(derived("admin"))
    expect(sections.map((section) => section.id)).toEqual(["self", "providers", "model-catalog", "rules", "management", "people", "access", "system"])
  })

  it("keeps pricing on model pages rather than a duplicate navigation entry", () => {
    const routes = sectionsFor(derived("admin")).flatMap(section => section.items.map(item => item.route))
    expect(routes).toContain("/model-catalog")
    expect(routes).toContain("/providers")
    expect(routes).not.toContain("/price-rules")
  })

  // `canSeeLogs` is an instance setting, not a role: the item disappears
  // rather than rendering a page that answers with nothing.
  it("drops the recent-requests item when the instance has the log off", () => {
    const items = sectionsFor(derived("user", false))[0].items.map((item) => item.id)
    expect(items).not.toContain("requests")
    expect(items).toContain("overview")
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
