import type { PriceRuleDto } from "@/generated/PriceRuleDto"

export function exactProviderPrices(providerId: number, rules: Array<PriceRuleDto>) {
  return new Set(rules
    .filter((rule) => rule.provider_id === providerId && !rule.model_pattern.includes("*"))
    .map((rule) => rule.model_pattern))
}
