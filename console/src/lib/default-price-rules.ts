import type { PriceRuleInput } from "@/api/price-rules";
import openrouterPriceRules from "@/data/openrouter-price-rules.json";

export type PriceRuleDraft = Omit<PriceRuleInput, "id">;

export interface PriceRuleBundle {
  schema_version: number;
  source: {
    catalog: string;
    total_models: number;
    supported_output_models: number;
    dynamic_price_models: number;
    included_models: number;
    embedding_models: number;
    image_output_priced_models: number;
  };
  price_rules: PriceRuleDraft[];
}

const DEFAULT_BUNDLE = openrouterPriceRules as PriceRuleBundle;

export const DEFAULT_RULES: PriceRuleDraft[] = DEFAULT_BUNDLE.price_rules.map((rule) => ({
  ...rule,
  provider_id: null,
  match_type: "contains",
}));

export function findDefaultPriceRule(model: string): PriceRuleDraft | undefined {
  const normalizedModel = model.trim().toLowerCase();
  if (!normalizedModel) return undefined;

  let bestMatch: PriceRuleDraft | undefined;
  for (const rule of DEFAULT_RULES) {
    if (
      normalizedModel.includes(rule.model_match.toLowerCase()) &&
      (!bestMatch || rule.model_match.length > bestMatch.model_match.length)
    ) {
      bestMatch = rule;
    }
  }
  return bestMatch;
}
