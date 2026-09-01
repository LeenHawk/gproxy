import type { ProviderModelDto } from "@/generated/ProviderModelDto"
import type { ProviderModelWriteRequest } from "@/generated/ProviderModelWriteRequest"

export type ModelMetadataState = {
  displayName: string
  variants: string
  contextWindow: string
  maxOutputTokens: string
  thinkingSupported: boolean | null
  thinkingAdaptiveSupported: boolean | null
  thinkingEnabledSupported: boolean | null
}

export function providerModelState(value: ProviderModelDto | null): ModelMetadataState {
  return {
    displayName: value?.display_name ?? "",
    variants: value?.variants == null ? "" : JSON.stringify(value.variants, null, 2),
    contextWindow: value?.context_window == null ? "" : String(value.context_window),
    maxOutputTokens: value?.max_output_tokens == null ? "" : String(value.max_output_tokens),
    thinkingSupported: value?.thinking_supported ?? null,
    thinkingAdaptiveSupported: value?.thinking_adaptive_supported ?? null,
    thinkingEnabledSupported: value?.thinking_enabled_supported ?? null,
  }
}

export function providerModelRequest(value: ModelMetadataState): Omit<ProviderModelWriteRequest, "provider_id" | "model_id" | "enabled"> {
  const optionalNumber = (input: string) => input ? Number(input) : null
  return {
    display_name: value.displayName.trim() || null,
    variants: value.variants.trim() ? JSON.parse(value.variants) as unknown : null,
    context_window: optionalNumber(value.contextWindow),
    max_output_tokens: optionalNumber(value.maxOutputTokens),
    thinking_supported: value.thinkingSupported,
    thinking_adaptive_supported: value.thinkingAdaptiveSupported,
    thinking_enabled_supported: value.thinkingEnabledSupported,
  }
}
