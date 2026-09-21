import { describe, expect, it } from "vitest"
import { BASE, href, routeOf } from "@/lib/router"

// The host serves the bundle under `/console` and falls back to `index.html`
// for any extensionless path below it, so the browser's location carries the
// prefix and the route table does not.
describe("the mount prefix", () => {
  it("strips it from a browser path", () => {
    expect(routeOf(`${BASE}/keys`)).toBe("/keys")
    expect(routeOf(`${BASE}/identity/users`)).toBe("/identity/users")
    expect(routeOf(BASE)).toBe("/")
    expect(routeOf(`${BASE}/`)).toBe("/")
  })

  it("tolerates a path that arrived without it", () => {
    expect(routeOf("/keys")).toBe("/keys")
    expect(routeOf("/")).toBe("/")
  })

  it("puts it back on the way out", () => {
    expect(href("/")).toBe(BASE)
    expect(href("/keys")).toBe(`${BASE}/keys`)
  })
})
