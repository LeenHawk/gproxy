import { queryOptions } from "@tanstack/react-query";
import { api } from "./http";

export interface PriceRule {
  id: number;
  provider_id: number | null;
  match_type: "exact" | "contains";
  model_match: string;
  input_price: string;
  output_price: string;
  cache_read_price: string;
  cache_creation_5m_price: string;
  cache_creation_1h_price: string;
  image_price: string;
  enabled: boolean;
  created_at: number;
  updated_at: number;
}

export interface PriceRuleInput {
  id?: number | null;
  provider_id?: number | null;
  match_type: "exact" | "contains";
  model_match: string;
  input_price: string;
  output_price: string;
  cache_read_price: string;
  cache_creation_5m_price: string;
  cache_creation_1h_price: string;
  image_price: string;
  enabled: boolean;
}

export const priceRulesQuery = queryOptions({
  queryKey: ["price-rules"],
  queryFn: () => api<PriceRule[]>("/admin/price-rules"),
});

export function upsertPriceRule(input: PriceRuleInput): Promise<PriceRule> {
  return api<PriceRule>("/admin/price-rules", {
    method: "POST",
    body: JSON.stringify(input),
  });
}

export function deletePriceRule(id: number): Promise<void> {
  return api<void>(`/admin/price-rules/${id}`, { method: "DELETE" });
}
