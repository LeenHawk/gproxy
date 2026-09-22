import { describe, expect, it } from "vitest"
import type { SettingsDto } from "@/generated/sdk"
import { settingsPatch } from "@/pages/settings/schema"

const settings = {
  instance: { instanceName: "gateway", configRevision: 1, corsOrigins: ["https://old.example"], hasTokenizerAuthToken: true, retentionDays: 7 },
  logging: { logLevel: "info", logFormat: "text" },
} as SettingsDto

describe("global settings patches", () => {
  it("sends no untouched values or saved secret", () => {
    expect(settingsPatch(settings, settings, undefined)).toEqual({})
  })
  it("distinguishes clearing lists, nullable values and credentials", () => {
    const draft = { ...settings, instance: { ...settings.instance, corsOrigins: [], retentionDays: null } }
    expect(settingsPatch(settings, draft, null)).toEqual({ instance: { corsOrigins: [], retentionDays: null, tokenizerAuthToken: null } })
  })
  it("sends only the edited logging field", () => {
    expect(settingsPatch(settings, { ...settings, logging: { ...settings.logging, logFormat: "json" } }, undefined)).toEqual({ logging: { logFormat: "json" } })
  })
})
