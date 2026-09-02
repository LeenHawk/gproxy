import { describe, expect, it } from "vitest"
import type { ChannelFieldDto } from "@/generated/ChannelFieldDto"
import {
  buildSecret,
  defaultCredentialKind,
  fieldsForCredentialKind,
} from "./credential-secret"

const field = (key: string): ChannelFieldDto => ({
  key,
  i18n_key: key,
  control: "secret",
  required: false,
  advanced: false,
  default_value: null,
  options: [],
})

describe("credential secret shape", () => {
  it("uses the bare API key field for mixed API key and OAuth channels", () => {
    const declared = [field("api_key"), field("access_token"), field("refresh_token")]
    const fields = fieldsForCredentialKind(declared, "api_key")

    expect(defaultCredentialKind(declared)).toBe("api_key")
    expect(fields.map((item) => item.key)).toEqual(["api_key"])
    expect(buildSecret(fields, "upstream-key")).toEqual({ api_key: "upstream-key" })
  })

  it("models a cookie credential as one opaque cookie even for OAuth channels", () => {
    const fields = fieldsForCredentialKind([field("access_token"), field("refresh_token")], "cookie")

    expect(fields.map((item) => item.key)).toEqual(["cookie"])
    expect(buildSecret(fields, "sessionKey=sk-ant-sid-example")).toEqual({
      cookie: "sessionKey=sk-ant-sid-example",
    })
  })

  it("defaults cookie-only channel forms to the cookie kind", () => {
    expect(defaultCredentialKind([field("cookie")])).toBe("cookie")
  })
})
