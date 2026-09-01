import type { AliasDto } from "@/generated/AliasDto"
import type { AliasWriteRequest } from "@/generated/AliasWriteRequest"
import type { CredentialDto } from "@/generated/CredentialDto"
import type { CredentialSecretResponse } from "@/generated/CredentialSecretResponse"
import type { CredentialWriteRequest } from "@/generated/CredentialWriteRequest"
import type { ModelAliasDto } from "@/generated/ModelAliasDto"
import type { ModelDiscoverRequest } from "@/generated/ModelDiscoverRequest"
import type { ModelDiscoverResponse } from "@/generated/ModelDiscoverResponse"
import type { ModelTestRequest } from "@/generated/ModelTestRequest"
import type { ModelTestResponse } from "@/generated/ModelTestResponse"
import type { ProviderModelDto } from "@/generated/ProviderModelDto"
import type { ProviderModelWriteRequest } from "@/generated/ProviderModelWriteRequest"
import type { ModelAliasWriteRequest } from "@/generated/ModelAliasWriteRequest"
import type { InstanceSettingsDto } from "@/generated/InstanceSettingsDto"
import type { PriceRateDto } from "@/generated/PriceRateDto"
import type { PriceRateWriteRequest } from "@/generated/PriceRateWriteRequest"
import type { PriceRuleDto } from "@/generated/PriceRuleDto"
import type { PriceRuleWriteRequest } from "@/generated/PriceRuleWriteRequest"
import type { ProviderDto } from "@/generated/ProviderDto"
import type { QuotaProbeResponse } from "@/generated/QuotaProbeResponse"
import type { QuotaResetResponse } from "@/generated/QuotaResetResponse"
import type { ProviderWriteRequest } from "@/generated/ProviderWriteRequest"
import type { TokenizerFetchRequest } from "@/generated/TokenizerFetchRequest"
import type { TokenizerDeleteRequest } from "@/generated/TokenizerDeleteRequest"
import type { TokenizerDownloadProgressDto } from "@/generated/TokenizerDownloadProgressDto"
import type { TokenizerVocabDto } from "@/generated/TokenizerVocabDto"
import type { TokenizerAuthDto } from "@/generated/TokenizerAuthDto"
import type { TokenizerAuthUpdate } from "@/generated/TokenizerAuthUpdate"
import type { TokenizerAuthRevealResponse } from "@/generated/TokenizerAuthRevealResponse"
import type { BatchActionDto } from "@/generated/BatchActionDto"
import type { BatchResponse } from "@/generated/BatchResponse"
import type { Entity } from "@/generated/Entity"
import type { ConfigurationExportDto } from "@/generated/ConfigurationExportDto"
import type { ConfigurationExportRequest } from "@/generated/ConfigurationExportRequest"
import type { ConfigurationImportRequest } from "@/generated/ConfigurationImportRequest"
import type { ConfigurationImportResponse } from "@/generated/ConfigurationImportResponse"
import type { ConnectivityTestRequest } from "@/generated/ConnectivityTestRequest"
import type { ConnectivityTestResponse } from "@/generated/ConnectivityTestResponse"
import type { RouteDto } from "@/generated/RouteDto"
import type { RouteMemberDto } from "@/generated/RouteMemberDto"
import type { RouteMemberWriteRequest } from "@/generated/RouteMemberWriteRequest"
import type { RouteWriteRequest } from "@/generated/RouteWriteRequest"
import type { ProviderRuleSetDto } from "@/generated/ProviderRuleSetDto"
import type { ProviderRuleSetWriteRequest } from "@/generated/ProviderRuleSetWriteRequest"
import type { RoutingRuleDto } from "@/generated/RoutingRuleDto"
import type { RoutingRuleWriteRequest } from "@/generated/RoutingRuleWriteRequest"
import type { RuleDto } from "@/generated/RuleDto"
import type { RuleSetDto } from "@/generated/RuleSetDto"
import type { RuleSetWriteRequest } from "@/generated/RuleSetWriteRequest"
import type { RuleWriteRequest } from "@/generated/RuleWriteRequest"
import type { RulePresetDto } from "@/generated/RulePresetDto"
import { api, json } from "@/api/client"

const save = <T>(path: string, value: T, id?: number) =>
  api(id == null ? path : `${path}/${id}`, json(id == null ? "POST" : "PATCH", value))

export const probeCredentialQuota = (id: number) =>
  api<QuotaProbeResponse>(`/admin/api/credentials/${id}/quota-probe`, json("POST", {}))
export const resetCredentialQuota = (id: number) =>
  api<QuotaResetResponse>(`/admin/api/credentials/${id}/quota-reset`, json("POST", {}))
export const resetCredentialHealth = (id: number) =>
  api<void>(`/admin/api/credentials/${id}/health-reset`, json("POST", {}))
export const revealCredentialSecret = (id: number) =>
  api<CredentialSecretResponse>(`/admin/api/credentials/${id}/reveal`, json("POST", {}))

export const batch = (entity: Entity, action: BatchActionDto, ids: Array<number>) =>
  api<BatchResponse>(`/admin/api/batch/${entity}`, json("POST", { action, ids }))
export const deleteEntity = (entity: Entity, id: number) =>
  api<void>(`/admin/api/${entity}/${id}`, { method: "DELETE" })
export const exportConfiguration = (value: ConfigurationExportRequest) =>
  api<ConfigurationExportDto>("/admin/api/export", json("POST", value))
export const importConfiguration = (value: ConfigurationImportRequest) =>
  api<ConfigurationImportResponse>("/admin/api/import", json("POST", value))
export const testConnectivity = (value: ConnectivityTestRequest) =>
  api<ConnectivityTestResponse>("/admin/api/connectivity/test", json("POST", value))

export const providers = () => api<Array<ProviderDto>>("/admin/api/providers")
export const saveProvider = (value: ProviderWriteRequest, id?: number) =>
  save("/admin/api/providers", value, id)
export const credentials = () => api<Array<CredentialDto>>("/admin/api/credentials")
export const saveCredential = (value: CredentialWriteRequest, id?: number) =>
  save("/admin/api/credentials", value, id)
export const routes = () => api<Array<RouteDto>>("/admin/api/routes")
export const saveRoute = (value: RouteWriteRequest, id?: number) =>
  save("/admin/api/routes", value, id)
export const routeMembers = () => api<Array<RouteMemberDto>>("/admin/api/route-members")
export const saveRouteMember = (value: RouteMemberWriteRequest, id?: number) =>
  save("/admin/api/route-members", value, id)
export const aliases = () => api<Array<AliasDto>>("/admin/api/aliases")
export const saveAlias = (value: AliasWriteRequest, id?: number) =>
  save("/admin/api/aliases", value, id)
export const modelAliases = () => api<Array<ModelAliasDto>>("/admin/api/model-aliases")
export const saveModelAlias = (value: ModelAliasWriteRequest, id?: number) =>
  save("/admin/api/model-aliases", value, id)
export const discoverModels = (value: ModelDiscoverRequest) =>
  api<ModelDiscoverResponse>("/admin/api/models/discover", json("POST", value))
export const testModel = (value: ModelTestRequest) =>
  api<ModelTestResponse>("/admin/api/models/test", json("POST", value))
export const providerModels = () => api<Array<ProviderModelDto>>("/admin/api/provider-models")
export const saveProviderModel = (value: ProviderModelWriteRequest, id?: number) =>
  save("/admin/api/provider-models", value, id)
export const priceRules = () => api<Array<PriceRuleDto>>("/admin/api/price-rules")
export const savePriceRule = (value: PriceRuleWriteRequest, id?: number) =>
  save("/admin/api/price-rules", value, id)
export const priceRates = () => api<Array<PriceRateDto>>("/admin/api/price-rates")
export const savePriceRate = (value: PriceRateWriteRequest, id?: number) =>
  save("/admin/api/price-rates", value, id)
export const deletePriceRate = (id: number) =>
  deleteEntity("price-rates", id)
export const routingRules = () => api<Array<RoutingRuleDto>>("/admin/api/routing-rules")
export const saveRoutingRule = (value: RoutingRuleWriteRequest, id?: number) => save("/admin/api/routing-rules", value, id)
export const deleteRoutingRule = (id: number) => deleteEntity("routing-rules", id)
export const resetRoutingDefaults = (providerId: number) =>
  api<void>(`/admin/api/providers/${providerId}/routing-defaults/reset`, json("POST", {}))
export const ruleSets = () => api<Array<RuleSetDto>>("/admin/api/rule-sets")
export const saveRuleSet = (value: RuleSetWriteRequest, id?: number) => save("/admin/api/rule-sets", value, id)
export const deleteRuleSet = (id: number) => deleteEntity("rule-sets", id)
export const rules = () => api<Array<RuleDto>>("/admin/api/rules")
export const saveRule = (value: RuleWriteRequest, id?: number) => save("/admin/api/rules", value, id)
export const deleteRule = (id: number) => deleteEntity("rules", id)
export const providerRuleSets = () => api<Array<ProviderRuleSetDto>>("/admin/api/provider-rule-sets")
export const saveProviderRuleSet = (value: ProviderRuleSetWriteRequest, id?: number) => save("/admin/api/provider-rule-sets", value, id)
export const deleteProviderRuleSet = (id: number) => deleteEntity("provider-rule-sets", id)
export const rulePresets = () => api<Array<RulePresetDto>>("/admin/api/rule-presets")
export const applyRulePreset = (providerId: number, preset: string) =>
  api<RuleSetDto>(`/admin/api/providers/${providerId}/rule-presets/${preset}`, json("POST", {}))
export const instanceSettings = () => api<InstanceSettingsDto>("/admin/api/instance-settings")
export const saveInstanceSettings = (value: InstanceSettingsDto) =>
  api<InstanceSettingsDto>("/admin/api/instance-settings", json("PATCH", value))
export const tokenizerVocabs = () => api<Array<TokenizerVocabDto>>("/admin/api/tokenizer-vocabs")
export const fetchTokenizerVocab = (value: TokenizerFetchRequest) =>
  api<TokenizerVocabDto>("/admin/api/tokenizer-vocabs", json("POST", value))
export const tokenizerVocabProgress = (name: string) =>
  api<TokenizerDownloadProgressDto | null>(`/admin/api/tokenizer-vocabs/progress?${new URLSearchParams({ name })}`)
export const deleteTokenizerVocab = (value: TokenizerDeleteRequest) =>
  api<void>("/admin/api/tokenizer-vocabs", json("DELETE", value))
export const tokenizerAuth = () => api<TokenizerAuthDto>("/admin/api/tokenizer-auth")
export const updateTokenizerAuth = (value: TokenizerAuthUpdate) =>
  api<TokenizerAuthDto>("/admin/api/tokenizer-auth", json("PATCH", value))
export const revealTokenizerAuth = () =>
  api<TokenizerAuthRevealResponse>("/admin/api/tokenizer-auth/reveal", json("POST", {}))
