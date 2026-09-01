import type { DefaultPriceCatalogDto } from "@/generated/DefaultPriceCatalogDto"
import type { DefaultPriceRuleDto } from "@/generated/DefaultPriceRuleDto"
import type { PriceRuleDto } from "@/generated/PriceRuleDto"

export function findDefaultPrice(
  catalog: DefaultPriceCatalogDto | undefined,
  model: string,
): DefaultPriceRuleDto | undefined {
  if (!catalog) return undefined
  const normalized = model.trim().toLowerCase()
  if (!normalized) return undefined
  let best: DefaultPriceRuleDto | undefined
  let bestLength = -1
  for (const rule of catalog.price_rules) {
    const needle = rule.model_pattern.slice(1, -1).toLowerCase()
    if (needle.length > bestLength && normalized.includes(needle)) {
      best = rule
      bestLength = needle.length
    }
  }
  return best
}

export function exactProviderPrices(providerId: number, rules: Array<PriceRuleDto>) {
  return new Set(rules
    .filter((rule) => rule.provider_id === providerId && !rule.model_pattern.includes("*"))
    .map((rule) => rule.model_pattern))
}
