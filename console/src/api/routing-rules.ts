import { family } from "@/api/admin"
import { api } from "@/api/client"
import type { RuleSetDto, RuleSetWrite, RuleSetPatch, RewriteRuleDto, RewriteRuleWrite, RewriteRulePatch, ProviderRuleSetDto, ProviderRuleSetWrite, ProviderRuleSetPatch, OperationRuleDto, OperationRuleWrite, OperationRulePatch, OperationEndpointDto, OperationEndpointWrite, OperationEndpointPatch, RulePresetDto } from "@/generated/sdk"
export const ruleSets = family<RuleSetDto, Partial<RuleSetWrite>, Partial<RuleSetPatch>>("/rule-sets")
const ruleFamily = family<RewriteRuleDto, Partial<RewriteRuleWrite>, Partial<RewriteRulePatch>>("/rules")
export const rules = { ...ruleFamily, update: (id: string, patch: Partial<RewriteRulePatch>) => ruleFamily.update(id, { ...patch, ...(patch.replacement === null ? { replacement: "" } : {}) }) }
export const bindings = family<ProviderRuleSetDto, Partial<ProviderRuleSetWrite>, Partial<ProviderRuleSetPatch>>("/provider-rule-sets")
export const operationRules = family<OperationRuleDto, Partial<OperationRuleWrite>, Partial<OperationRulePatch>>("/operation-rules")
export const endpoints = family<OperationEndpointDto, Partial<OperationEndpointWrite>, Partial<OperationEndpointPatch>>("/operation-endpoints")
export const rulePresets = () => api<RulePresetDto[]>("/admin/api/rule-presets")
export const applyPreset = (setId: string, preset: string) => api<RewriteRuleDto[]>(`/admin/api/rule-sets/${encodeURIComponent(setId)}/rule-presets/${encodeURIComponent(preset)}`, { method: "POST" })
export async function ruleSetDirectory() {
  const rows: RuleSetDto[] = []
  for (let page = 1; ; page++) {
    const result = await ruleSets.list({ page, pageSize: 500 })
    rows.push(...result.items)
    if (rows.length >= result.total || result.items.length === 0) return rows
  }
}

export const effectiveRouting = (providerId: string) => api<import("@/generated/sdk").OperationRoutingDto[]>(`/admin/api/providers/${encodeURIComponent(providerId)}/routing`)
