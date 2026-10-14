import { describe, expect, it } from "vitest"
import { configPlaceholder, setConfigValue } from "@/pages/providers/config-schema"
import type { ConfigKey } from "@/generated/sdk"

describe("provider configuration controls", () => {
  it("updates one field while preserving unedited configuration", () => {
    const original = {
      headers: { "x-request": "keep" },
      custom_extension: { enabled: true },
      region: "us-east-1",
    }
    expect(setConfigValue(original, "region", "eu-west-1")).toEqual({ ...original, region: "eu-west-1" })
    expect(setConfigValue(original, "region", undefined)).toEqual({
      headers: original.headers,
      custom_extension: original.custom_extension,
    })
    expect(original.region).toBe("us-east-1")
  })
})

describe("channel address placeholders", () => {
  const field = (name: string, placeholder: string | null): ConfigKey => ({ name, placeholder, kind: "string", required: false, description: "" })
  it("uses channel defaults, regional settings and inherited origins without writing config", () => {
    const fields = [field("region", "us-east-1"), field("base_url", "https://runtime.{region}.kiro.dev")]
    const config = { region: "eu-west-1" }
    expect(configPlaceholder(fields[1], fields, config, "")).toBe("https://runtime.eu-west-1.kiro.dev")
    expect(configPlaceholder(fields[1], fields, {}, "")).toBe("https://runtime.us-east-1.kiro.dev")
    expect(config).toEqual({ region: "eu-west-1" })
    expect(configPlaceholder(field("usage_base_url", "{base_url}"), [field("base_url", "https://default.example")], {}, "https://custom.example")).toBe("https://custom.example")
    expect(configPlaceholder(field("usage_base_url", "{base_url}"), [field("base_url", "https://default.example")], {}, "")).toBe("https://default.example")
    expect(configPlaceholder(field("base_url", "https://{location}-aiplatform.googleapis.com"), [], { location: "global" }, "")).toBe("https://aiplatform.googleapis.com")
    expect(configPlaceholder(field("base_url", "https://{resource}.openai.azure.com"), [], {}, "")).toBe("https://{resource}.openai.azure.com")
    expect(configPlaceholder(field("base_url", null), [], {}, "")).toBeUndefined()
  })
})
