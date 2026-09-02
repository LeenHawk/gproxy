import { describe, expect, it } from "vitest"
import type { LoginParamDto } from "@/generated/LoginParamDto"
import { loginParamApplies } from "./login-param-values"

const idcField: LoginParamDto = {
  name: "region",
  kind: "text",
  required: true,
  default_value: null,
  options: [],
  modes: ["authcode"],
  condition: { param: "auth_method", equals: "idc" },
}

describe("login parameter conditions", () => {
  it("shows required Kiro IdC fields only for the IdC authcode branch", () => {
    expect(loginParamApplies(idcField, "authcode", { auth_method: "idc" })).toBe(true)
    expect(loginParamApplies(idcField, "authcode", { auth_method: "builder_id" })).toBe(false)
    expect(loginParamApplies(idcField, "device", { auth_method: "idc" })).toBe(false)
  })
})
