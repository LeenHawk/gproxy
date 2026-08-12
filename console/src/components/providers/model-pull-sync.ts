import type {
  ProviderModel,
  ProviderModelInput,
  UpstreamModel,
} from "@/api/provider-models";
import type { PriceRule, PriceRuleInput } from "@/api/price-rules";
import { findDefaultPriceRule } from "@/lib/default-price-rules";

const METADATA_FIELDS = [
  "display_name",
  "context_window",
  "max_input_tokens",
  "max_output_tokens",
  "thinking_supported",
  "thinking_adaptive_supported",
  "thinking_enabled_supported",
] as const;

export function missingMetadataCount(
  existing: ProviderModel | undefined,
  upstream: UpstreamModel,
): number {
  if (!existing) return METADATA_FIELDS.filter((field) => upstream[field] != null).length;
  return METADATA_FIELDS.filter(
    (field) => existing[field] == null && upstream[field] != null,
  ).length;
}

/** Build an additive model write: upstream values only fill empty persisted fields. */
export function modelSyncInput(
  providerId: number,
  upstream: UpstreamModel,
  existing?: ProviderModel,
): ProviderModelInput {
  const input: ProviderModelInput = {
    id: existing?.id ?? null,
    provider_id: providerId,
    model_id: upstream.id,
    display_name: existing?.display_name ?? upstream.display_name,
    context_window: existing?.context_window ?? upstream.context_window,
    max_input_tokens: existing?.max_input_tokens ?? upstream.max_input_tokens,
    max_output_tokens: existing?.max_output_tokens ?? upstream.max_output_tokens,
    thinking_supported: existing?.thinking_supported ?? upstream.thinking_supported,
    thinking_adaptive_supported:
      existing?.thinking_adaptive_supported ?? upstream.thinking_adaptive_supported,
    thinking_enabled_supported:
      existing?.thinking_enabled_supported ?? upstream.thinking_enabled_supported,
    enabled: existing?.enabled ?? true,
  };
  if (existing?.variants_json != null) input.variants_json = existing.variants_json;
  return input;
}

export function hasExactProviderPrice(
  providerId: number,
  modelId: string,
  rules: PriceRule[],
): boolean {
  return rules.some(
    (rule) =>
      rule.provider_id === providerId &&
      rule.match_type === "exact" &&
      rule.model_match === modelId,
  );
}

/** Convert a bundled default into a provider-scoped exact rule. */
export function defaultPriceInput(
  providerId: number,
  modelId: string,
  rules: PriceRule[],
): PriceRuleInput | undefined {
  if (hasExactProviderPrice(providerId, modelId, rules)) return undefined;
  const matched = findDefaultPriceRule(modelId);
  if (!matched) return undefined;
  return {
    ...matched,
    id: null,
    provider_id: providerId,
    match_type: "exact",
    model_match: modelId,
  };
}
