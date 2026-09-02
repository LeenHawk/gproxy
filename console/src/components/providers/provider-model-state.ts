import type { ProviderModelDto } from "@/generated/ProviderModelDto"
import type { ProviderModelWriteRequest } from "@/generated/ProviderModelWriteRequest"
import type { VariantAction } from "@/components/providers/variant-presets/types"
import type { VariantRuleRow } from "@/components/providers/provider-model-variant-rules"

export type ModelMetadataState = {
  displayName: string
  variants: Array<VariantRuleRow>
  exposeBase: boolean
  contextWindow: string
  maxOutputTokens: string
  thinkingSupported: boolean | null
  thinkingAdaptiveSupported: boolean | null
  thinkingEnabledSupported: boolean | null
}

export function providerModelState(value: ProviderModelDto | null, actions: Map<string, Array<VariantAction>> = new Map()): ModelMetadataState {
  const variants = readVariants(value?.variants)
  return {
    displayName: value?.display_name ?? "",
    variants: variants.names.map((name) => ({ name, actions: actions.get(name) ?? [], touched: false })),
    exposeBase: variants.exposeBase,
    contextWindow: value?.context_window == null ? "" : String(value.context_window),
    maxOutputTokens: value?.max_output_tokens == null ? "" : String(value.max_output_tokens),
    thinkingSupported: value?.thinking_supported ?? null,
    thinkingAdaptiveSupported: value?.thinking_adaptive_supported ?? null,
    thinkingEnabledSupported: value?.thinking_enabled_supported ?? null,
  }
}

export function providerModelRequest(value: ModelMetadataState): Omit<ProviderModelWriteRequest, "provider_id" | "model_id" | "enabled"> {
  const optionalNumber = (input: string) => input ? Number(input) : null
  const variants = value.variants.map((row) => row.name.trim()).filter(Boolean)
  return {
    display_name: value.displayName.trim() || null,
    variants: value.exposeBase ? (variants.length > 0 ? variants : null) : { expose_base: false, variants },
    context_window: optionalNumber(value.contextWindow),
    max_output_tokens: optionalNumber(value.maxOutputTokens),
    thinking_supported: value.thinkingSupported,
    thinking_adaptive_supported: value.thinkingAdaptiveSupported,
    thinking_enabled_supported: value.thinkingEnabledSupported,
  }
}

export function readVariants(value: unknown): { names: Array<string>; exposeBase: boolean } {
  if (Array.isArray(value)) return { names: value.map(String), exposeBase: true }
  if (value && typeof value === "object") {
    const object = value as { expose_base?: unknown; variants?: unknown }
    return {
      names: Array.isArray(object.variants) ? object.variants.map(String) : [],
      exposeBase: object.expose_base !== false,
    }
  }
  return { names: [], exposeBase: true }
}
