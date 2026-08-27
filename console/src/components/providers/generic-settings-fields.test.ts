import { describe, expect, it } from "vitest"
import type { ChannelFieldDto } from "@/generated/ChannelFieldDto"
import { channelFieldText, updateSetting } from "./settings-values"
import { resolveTheme, storedTheme, THEME_STORAGE_KEY } from "@/lib/theme-state"

describe("declared channel settings", () => {
  it("serializes an unseen channel field set without channel-specific frontend code", () => {
    const fields: Array<ChannelFieldDto> = [
      { key: "region", i18n_key: "region", control: "text", required: true, advanced: false, default_value: null, options: [] },
      { key: "strict_mode", i18n_key: "strict_mode", control: "boolean", required: false, advanced: true, default_value: null, options: [] },
      { key: "fallbacks", i18n_key: "fallbacks", control: "string_list", required: false, advanced: true, default_value: null, options: [] },
      { key: "fallback_mode", i18n_key: "fallback_mode", control: "select", required: true, advanced: true, default_value: "off", options: ["off", "models"] },
    ]
    const inputs: Array<string | boolean> = ["eu-west-3", true, "a, b", "models"]
    const values = fields.reduce(
      (current, field, index) => updateSetting(current, field, inputs[index]),
      { untouched: 7 } as Record<string, unknown>,
    )
    expect(values).toEqual({ untouched: 7, region: "eu-west-3", strict_mode: true, fallbacks: ["a", "b"], fallback_mode: "models" })
    expect(channelFieldText(fields[0], (key) => ({
      "providers.channelFields.region.label": "Cloud region",
      "providers.channelFields.region.description": "Selects the upstream regional endpoint.",
    })[key] ?? key)).toEqual({
      label: "Cloud region",
      description: "Selects the upstream regional endpoint.",
    })
  })

  it("resolves system, explicit, and persisted theme choices", () => {
    expect(resolveTheme("system", true)).toBe("dark")
    expect(resolveTheme("system", false)).toBe("light")
    expect(resolveTheme("light", true)).toBe("light")
    expect(storedTheme({ getItem: (key) => key === THEME_STORAGE_KEY ? "dark" : null })).toBe("dark")
    expect(storedTheme({ getItem: () => "unknown" })).toBe("system")
  })
})
