/// <reference types="node" />
import { readFileSync, readdirSync } from "node:fs"
import path from "node:path"
import { describe, expect, it } from "vitest"
import { controlFor, setConfigValue } from "@/pages/providers/config-schema"
import type { ConfigKey } from "@/generated/sdk"

function sources(directory: string): Array<string> {
  return readdirSync(directory, { withFileTypes: true }).flatMap((entry) => {
    const file = path.join(directory, entry.name)
    return entry.isDirectory() ? sources(file) : file.endsWith(".rs") ? [readFileSync(file, "utf8")] : []
  })
}

describe("provider configuration controls", () => {
  it("covers every config key declared by the channel implementations", () => {
    const rust = sources(path.resolve("../crates/gproxy-channel/src")).join("\n")
    const fields = [
      ...rust.matchAll(/ConfigKey::(?:optional|required)\(\s*"([^"]+)"\s*,\s*ConfigKeyKind::(\w+)/g),
    ]
    expect(fields.length).toBeGreaterThan(100)
    for (const [, name, kind] of fields) {
      const wire = kind === "HeaderList" ? "header_list" : kind.toLowerCase()
      expect(
        () => controlFor({ name, kind: wire, required: false, description: "" } as ConfigKey),
        name,
      ).not.toThrow()
    }
  })

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
