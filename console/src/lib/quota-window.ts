import type { TFunction } from "i18next"

const KNOWN = new Set([
  "gemini-5h", "gemini-weekly", "3p-5h", "3p-weekly",
  "five_hour", "seven_day",
  "primary", "secondary",
  "weekly_limit", "monthly_limit", "usage", "enterprise",
  "chat", "completions", "premium_interactions", "agentic_request",
])

const SCOPED: Array<[string, string]> = [
  ["seven_day_", "seven_day_scoped"],
  ["weekly_model:", "weekly_model"],
  ["weekly_surface:", "weekly_surface"],
  ["additional_primary:", "additional_primary"],
  ["additional_secondary:", "additional_secondary"],
  ["product:", "product"],
]

/* Window keys are channel wire facts; the known set gets a localized name,
   an upstream-declared label wins for scoped keys, and everything else stays
   verbatim rather than guessing. */
export function windowName(key: string, t: TFunction, label?: string | null): string {
  if (KNOWN.has(key)) {
    const name = t(`usage.windowNames.${key}`)
    return label === "antigravity_disabled" ? t("usage.windowNames.inactive", { window: name }) : name
  }
  for (const [prefix, i18nKey] of SCOPED) {
    if (key.startsWith(prefix)) {
      const derived = key.slice(prefix.length).replace(/_/g, " ")
      const scope = label ?? derived.charAt(0).toUpperCase() + derived.slice(1)
      return t(`usage.windowNames.${i18nKey}`, { scope })
    }
  }
  return key
}
