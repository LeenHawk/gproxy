import type { PriceRateDto, PriceRuleDto } from "@/generated/sdk"

export function priceRuleFor(rules: readonly PriceRuleDto[], operation = "generate_content") {
  return rules.find(rule => rule.enabled && (rule.operation === null || rule.operation === operation))
}

export function baseRateFor(rates: readonly PriceRateDto[], ruleId: string, metric: string) {
  return rates.filter(rate => rate.priceRuleId === ruleId && rate.metric === metric && rate.conditions === null)
    .sort((a, b) => a.priority - b.priority || (a.id < b.id ? -1 : a.id > b.id ? 1 : 0))[0]
}
