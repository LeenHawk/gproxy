import type { ProviderModelDto } from "@/generated/ProviderModelDto"
import type { ProviderModelWriteRequest } from "@/generated/ProviderModelWriteRequest"

export type TriState = "unset" | "true" | "false"

export type ModelMetadataState = {
  displayName: string
  variants: string
  contextWindow: string
  maxOutputTokens: string
  thinkingSupported: TriState
  thinkingAdaptiveSupported: TriState
  thinkingEnabledSupported: TriState
}

export function providerModelState(value: ProviderModelDto | null): ModelMetadataState {
  const tri = (flag: boolean | null): TriState => flag == null ? "unset" : String(flag) as TriState
  return {
    displayName: value?.display_name ?? "",
    variants: value?.variants == null ? "" : JSON.stringify(value.variants, null, 2),
    contextWindow: value?.context_window == null ? "" : String(value.context_window),
    maxOutputTokens: value?.max_output_tokens == null ? "" : String(value.max_output_tokens),
    thinkingSupported: tri(value?.thinking_supported ?? null),
    thinkingAdaptiveSupported: tri(value?.thinking_adaptive_supported ?? null),
    thinkingEnabledSupported: tri(value?.thinking_enabled_supported ?? null),
  }
}

export function providerModelRequest(value: ModelMetadataState): Omit<ProviderModelWriteRequest, "provider_id" | "model_id" | "enabled"> {
  const optionalNumber = (input: string) => input ? Number(input) : null
  const tri = (input: TriState) => input === "unset" ? null : input === "true"
  return {
    display_name: value.displayName.trim() || null,
    variants: value.variants.trim() ? JSON.parse(value.variants) as unknown : null,
    context_window: optionalNumber(value.contextWindow),
    max_output_tokens: optionalNumber(value.maxOutputTokens),
    thinking_supported: tri(value.thinkingSupported),
    thinking_adaptive_supported: tri(value.thinkingAdaptiveSupported),
    thinking_enabled_supported: tri(value.thinkingEnabledSupported),
  }
}
