import type { TFunction } from "i18next"

const KNOWN = new Set([
  "five_hour", "seven_day",
  "primary", "secondary",
  "weekly_limit", "monthly_limit", "usage", "enterprise",
  "chat", "completions", "premium_interactions", "agentic_request",
])

const SCOPED: Array<[string, string]> = [
  ["seven_day_", "seven_day_scoped"],
  ["weekly_model:", "weekly_model"],
  ["weekly_surface:", "weekly_surface"],
  ["product:", "product"],
]

/* Window keys are channel wire facts; the known set gets a localized name and
   everything else stays verbatim rather than guessing. */
export function windowName(key: string, t: TFunction): string {
  if (KNOWN.has(key)) return t(`usage.windowNames.${key}`)
  for (const [prefix, i18nKey] of SCOPED) {
    if (key.startsWith(prefix)) {
      const scope = key.slice(prefix.length)
      return t(`usage.windowNames.${i18nKey}`, { scope: scope.charAt(0).toUpperCase() + scope.slice(1) })
    }
  }
  return key
}
