import { describe, expect, it } from "vitest"
import { buildPatch, buildWrite, type FormField } from "@/components/record-form-values"

const FIELDS: ReadonlyArray<FormField> = [
  { name: "name", kind: "text", required: true },
  { name: "organizationId", kind: "text", nullable: true },
  { name: "priority", kind: "number" },
  { name: "enabled", kind: "switch" },
  { name: "redirectUris", kind: "lines" },
  { name: "organizationIdCreateOnly", kind: "text", createOnly: true },
]

describe("a create body", () => {
  it("carries only what was filled in", () => {
    const body = buildWrite(FIELDS, {
      name: "Acme",
      organizationId: "",
      priority: "",
      enabled: true,
      redirectUris: "",
      organizationIdCreateOnly: "",
    })
    expect(body).toEqual({ name: "Acme", enabled: true })
  })

  it("splits a lines field and drops the blanks", () => {
    const body = buildWrite(FIELDS, {
      name: "Acme",
      organizationId: "",
      priority: "",
      enabled: false,
      redirectUris: "http://a\n\n  http://b  \n",
      organizationIdCreateOnly: "",
    })
    expect(body.redirectUris).toEqual(["http://a", "http://b"])
  })
})

describe("a patch body", () => {
  const original = { name: "Acme", organizationId: "o1", priority: 5, enabled: true, redirectUris: ["http://a"] }

  it("is empty when nothing was touched", () => {
    const body = buildPatch(FIELDS, {
      name: "Acme",
      organizationId: "o1",
      priority: "5",
      enabled: true,
      redirectUris: "http://a",
      organizationIdCreateOnly: "",
    }, original)
    expect(body).toEqual({})
  })

  it("carries only the column that changed", () => {
    const body = buildPatch(FIELDS, {
      name: "Acme Ltd",
      organizationId: "o1",
      priority: "5",
      enabled: true,
      redirectUris: "http://a",
      organizationIdCreateOnly: "",
    }, original)
    expect(body).toEqual({ name: "Acme Ltd" })
  })

  // The `Option<Option<T>>` contract: absent leaves the column alone, an
  // explicit null clears it. Emptying a nullable input has to send the null.
  it("clears a nullable column with an explicit null", () => {
    const body = buildPatch(FIELDS, {
      name: "Acme",
      organizationId: "",
      priority: "5",
      enabled: true,
      redirectUris: "http://a",
      organizationIdCreateOnly: "",
    }, original)
    expect(body).toEqual({ organizationId: null })
  })

  it("leaves a non-nullable column alone when its input was emptied", () => {
    const body = buildPatch(FIELDS, {
      name: "",
      organizationId: "o1",
      priority: "5",
      enabled: true,
      redirectUris: "http://a",
      organizationIdCreateOnly: "",
    }, original)
    expect(body).toEqual({})
  })

  it("never sends an immutable column", () => {
    const body = buildPatch(FIELDS, {
      name: "Acme",
      organizationId: "o1",
      priority: "5",
      enabled: true,
      redirectUris: "http://a",
      organizationIdCreateOnly: "changed",
    }, original)
    expect(body).toEqual({})
  })
})
