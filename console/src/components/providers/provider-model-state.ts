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
  metadata: ModelMetadataDto
}

export const emptyModelMetadata = (): ModelMetadataDto => ({
  description: null,
  instructions: null,
  max_context_window: null,
  input_modalities: null,
  output_modalities: null,
  supported_parameters: null,
  supported_reasoning_levels: null,
  default_reasoning_level: null,
  service_tiers: null,
  default_service_tier: null,
  generation_methods: null,
  supported_actions: null,
  shell_type: null,
  support_verbosity: null,
  default_verbosity: null,
  supports_reasoning_summary_parameter: null,
  default_reasoning_summary: null,
  apply_patch_tool_type: null,
  web_search_tool_type: null,
  truncation_policy: null,
  auto_compact_token_limit: null,
  effective_context_window_percent: null,
  batch_supported: null,
  citations_supported: null,
  code_execution_supported: null,
  context_management_supported: null,
  structured_outputs_supported: null,
  pdf_input_supported: null,
  supports_image_detail_original: null,
  supports_search_tool: null,
})


export type ModelMetadataDto = { description: string | null, instructions: string | null, max_context_window: number | null, input_modalities: Array<string> | null, output_modalities: Array<string> | null, supported_parameters: Array<string> | null, supported_reasoning_levels: Array<ModelReasoningLevelDto> | null, default_reasoning_level: string | null, service_tiers: Array<ModelServiceTierDto> | null, default_service_tier: string | null, generation_methods: Array<string> | null, supported_actions: Array<string> | null, shell_type: string | null, support_verbosity: boolean | null, default_verbosity: string | null, supports_reasoning_summary_parameter: boolean | null, default_reasoning_summary: string | null, apply_patch_tool_type: string | null, web_search_tool_type: string | null, truncation_policy: ModelTruncationPolicyDto | null, auto_compact_token_limit: number | null, effective_context_window_percent: number | null, batch_supported: boolean | null, citations_supported: boolean | null, code_execution_supported: boolean | null, context_management_supported: boolean | null, structured_outputs_supported: boolean | null, pdf_input_supported: boolean | null, supports_image_detail_original: boolean | null, supports_search_tool: boolean | null, };

export type ModelTruncationPolicyDto = { mode: "bytes" | "tokens" | null; limit: number | null };

export type ModelReasoningLevelDto = { effort: string, description: string, };

export type ModelServiceTierDto = { id: string, name: string, description: string, };

export function object(value: unknown): Record<string, unknown> { return value && typeof value === "object" && !Array.isArray(value) ? value as Record<string, unknown> : {} }
export function modelState(metadata: unknown, variants: VariantRuleRow[]): ModelMetadataState {
  const m = object(metadata)
  const messages = object(m.model_messages)
  const levels = Object.hasOwn(m, "supported_reasoning_levels") ? m.supported_reasoning_levels : m.reasoning_levels
  const truncation = Object.hasOwn(m, "truncation_policy") ? m.truncation_policy :
    m.truncation_mode != null || m.truncation_limit != null ? { mode: m.truncation_mode ?? null, limit: m.truncation_limit ?? null } : null
  return { displayName: String(m.display_name ?? ""), contextWindow: String(m.context_window ?? ""), maxOutputTokens: String(m.max_output_tokens ?? ""),
    thinkingSupported: typeof m.thinking_supported === "boolean" ? m.thinking_supported : null,
    thinkingAdaptiveSupported: typeof m.thinking_adaptive_supported === "boolean" ? m.thinking_adaptive_supported : null,
    thinkingEnabledSupported: typeof m.thinking_enabled_supported === "boolean" ? m.thinking_enabled_supported : null,
    exposeBase: m.expose_base !== false, variants, metadata: { ...emptyModelMetadata(), ...m,
      supported_reasoning_levels: levels ?? null,
      truncation_policy: truncation ?? null,
      instructions: typeof messages.instructions_template === "string" ? messages.instructions_template : m.instructions ?? null,
    } as ModelMetadataDto }
}
export function modelMetadata(state: ModelMetadataState, original: unknown) {
  const metadata: Record<string, unknown> = { ...object(original), ...state.metadata, display_name: state.displayName.trim() || null,
    context_window: state.contextWindow ? Number(state.contextWindow) : null, max_output_tokens: state.maxOutputTokens ? Number(state.maxOutputTokens) : null,
    thinking_supported: state.thinkingSupported, thinking_adaptive_supported: state.thinkingAdaptiveSupported, thinking_enabled_supported: state.thinkingEnabledSupported,
    variants: state.variants.map(v => v.name.trim()).filter(Boolean), expose_base: state.exposeBase }
  for (const key of ["reasoning_levels", "truncation_mode", "truncation_limit"]) delete metadata[key]
  if (metadata.model_messages != null) {
    metadata.model_messages = { ...object(metadata.model_messages), instructions_template: state.metadata.instructions }
  }
  return metadata
}
