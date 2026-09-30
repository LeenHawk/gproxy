import { configFamily as family } from "@/api/config-family"
import { api, json } from "@/api/client"
import type { RuleSetDto, RuleSetWrite, RuleSetPatch, RewriteRuleDto, RewriteRuleWrite, RewriteRulePatch, ProviderRuleSetDto, ProviderRuleSetWrite, ProviderRuleSetPatch, OperationRuleDto, OperationRuleWrite, OperationRulePatch, OperationEndpointDto, OperationEndpointWrite, OperationEndpointPatch, RulePresetDto } from "@/generated/sdk"
export const ruleSets = family<RuleSetDto, Partial<RuleSetWrite>, Partial<RuleSetPatch>>("/rule-sets")
const ruleFamily = family<RewriteRuleDto, Partial<RewriteRuleWrite>, Partial<RewriteRulePatch>>("/rules")
export const rules = { ...ruleFamily, update: (id: string, patch: Partial<RewriteRulePatch>) => ruleFamily.update(id, { ...patch, ...(patch.replacement === null ? { replacement: "" } : {}) }) }
export const bindings = family<ProviderRuleSetDto, Partial<ProviderRuleSetWrite>, Partial<ProviderRuleSetPatch>>("/provider-rule-sets")
export const operationRules = family<OperationRuleDto, Partial<OperationRuleWrite>, Partial<OperationRulePatch>>("/operation-rules")
export const endpoints = family<OperationEndpointDto, Partial<OperationEndpointWrite>, Partial<OperationEndpointPatch>>("/operation-endpoints")
export const rulePresets = () => api<RulePresetDto[]>("/admin/api/rule-presets")
export const applyPreset = (setId: string, preset: string) => api<RewriteRuleDto[]>(`/admin/api/rule-sets/${encodeURIComponent(setId)}/rule-presets/${encodeURIComponent(preset)}`, { method: "POST" })
export const effectiveRouting = (providerId: string) => api<import("@/generated/sdk").OperationRoutingDto[]>(`/admin/api/providers/${encodeURIComponent(providerId)}/routing`)

const mappingPath = (providerId: string, operation: string, dialect: string) => `/admin/api/providers/${encodeURIComponent(providerId)}/routing/${encodeURIComponent(operation)}/${encodeURIComponent(dialect)}`
export const saveRoutingMapping = (providerId: string, operation: string, dialect: string, write: import("@/generated/sdk").RoutingMappingWrite) => api<import("@/generated/sdk").OperationRoutingDto[]>(mappingPath(providerId, operation, dialect), json("PUT", write))
export const resetRoutingMapping = (providerId: string, operation: string, dialect: string) => api<import("@/generated/sdk").OperationRoutingDto[]>(mappingPath(providerId, operation, dialect), { method: "DELETE" })

export const applyDefaultRouting = (providerId: string) => api<{ cleared: number }>(`/admin/api/providers/${encodeURIComponent(providerId)}/routing-defaults/reset`, { method: "POST" })

export const providerDefaultSetId = (providerId: string) => `gproxy:provider-default:${providerId}`
export const ensureProviderDefaultSet = (providerId: string) => api<RuleSetDto>(`/admin/api/providers/${encodeURIComponent(providerId)}/default-rule-set`, { method: "POST" })
