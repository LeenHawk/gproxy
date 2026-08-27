import type { AliasDto } from "@/generated/AliasDto"
import type { AliasWriteRequest } from "@/generated/AliasWriteRequest"
import type { CredentialDto } from "@/generated/CredentialDto"
import type { CredentialWriteRequest } from "@/generated/CredentialWriteRequest"
import type { ModelAliasDto } from "@/generated/ModelAliasDto"
import type { ModelAliasWriteRequest } from "@/generated/ModelAliasWriteRequest"
import type { InstanceSettingsDto } from "@/generated/InstanceSettingsDto"
import type { PriceRateDto } from "@/generated/PriceRateDto"
import type { PriceRateWriteRequest } from "@/generated/PriceRateWriteRequest"
import type { PriceRuleDto } from "@/generated/PriceRuleDto"
import type { PriceRuleWriteRequest } from "@/generated/PriceRuleWriteRequest"
import type { ProviderDto } from "@/generated/ProviderDto"
import type { ProviderWriteRequest } from "@/generated/ProviderWriteRequest"
import type { TokenizerFetchRequest } from "@/generated/TokenizerFetchRequest"
import type { TokenizerVocabDto } from "@/generated/TokenizerVocabDto"
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
import { api, json } from "@/api/client"

const save = <T>(path: string, value: T, id?: number) =>
  api(id == null ? path : `${path}/${id}`, json(id == null ? "POST" : "PATCH", value))

export const batch = (entity: Entity, action: BatchActionDto, ids: Array<number>) =>
  api<BatchResponse>(`/admin/batch/${entity}`, json("POST", { action, ids }))
export const exportConfiguration = (value: ConfigurationExportRequest) =>
  api<ConfigurationExportDto>("/admin/export", json("POST", value))
export const importConfiguration = (value: ConfigurationImportRequest) =>
  api<ConfigurationImportResponse>("/admin/import", json("POST", value))
export const testConnectivity = (value: ConnectivityTestRequest) =>
  api<ConnectivityTestResponse>("/admin/connectivity/test", json("POST", value))

export const providers = () => api<Array<ProviderDto>>("/admin/providers")
export const saveProvider = (value: ProviderWriteRequest, id?: number) =>
  save("/admin/providers", value, id)
export const credentials = () => api<Array<CredentialDto>>("/admin/credentials")
export const saveCredential = (value: CredentialWriteRequest, id?: number) =>
  save("/admin/credentials", value, id)
export const routes = () => api<Array<RouteDto>>("/admin/routes")
export const saveRoute = (value: RouteWriteRequest, id?: number) =>
  save("/admin/routes", value, id)
export const routeMembers = () => api<Array<RouteMemberDto>>("/admin/route-members")
export const saveRouteMember = (value: RouteMemberWriteRequest, id?: number) =>
  save("/admin/route-members", value, id)
export const aliases = () => api<Array<AliasDto>>("/admin/aliases")
export const saveAlias = (value: AliasWriteRequest, id?: number) =>
  save("/admin/aliases", value, id)
export const modelAliases = () => api<Array<ModelAliasDto>>("/admin/model-aliases")
export const saveModelAlias = (value: ModelAliasWriteRequest, id?: number) =>
  save("/admin/model-aliases", value, id)
export const priceRules = () => api<Array<PriceRuleDto>>("/admin/price-rules")
export const savePriceRule = (value: PriceRuleWriteRequest, id?: number) =>
  save("/admin/price-rules", value, id)
export const priceRates = () => api<Array<PriceRateDto>>("/admin/price-rates")
export const savePriceRate = (value: PriceRateWriteRequest, id?: number) =>
  save("/admin/price-rates", value, id)
export const deletePriceRate = (id: number) =>
  api<void>(`/admin/price-rates/${id}`, { method: "DELETE" })
export const routingRules = () => api<Array<RoutingRuleDto>>("/admin/routing-rules")
export const saveRoutingRule = (value: RoutingRuleWriteRequest, id?: number) => save("/admin/routing-rules", value, id)
export const deleteRoutingRule = (id: number) => api<void>(`/admin/routing-rules/${id}`, { method: "DELETE" })
export const ruleSets = () => api<Array<RuleSetDto>>("/admin/rule-sets")
export const saveRuleSet = (value: RuleSetWriteRequest, id?: number) => save("/admin/rule-sets", value, id)
export const deleteRuleSet = (id: number) => api<void>(`/admin/rule-sets/${id}`, { method: "DELETE" })
export const rules = () => api<Array<RuleDto>>("/admin/rules")
export const saveRule = (value: RuleWriteRequest, id?: number) => save("/admin/rules", value, id)
export const deleteRule = (id: number) => api<void>(`/admin/rules/${id}`, { method: "DELETE" })
export const providerRuleSets = () => api<Array<ProviderRuleSetDto>>("/admin/provider-rule-sets")
export const saveProviderRuleSet = (value: ProviderRuleSetWriteRequest, id?: number) => save("/admin/provider-rule-sets", value, id)
export const deleteProviderRuleSet = (id: number) => api<void>(`/admin/provider-rule-sets/${id}`, { method: "DELETE" })
export const instanceSettings = () => api<InstanceSettingsDto>("/admin/instance-settings")
export const saveInstanceSettings = (value: InstanceSettingsDto) =>
  api<InstanceSettingsDto>("/admin/instance-settings", json("PATCH", value))
export const tokenizerVocabs = () => api<Array<TokenizerVocabDto>>("/admin/tokenizer-vocabs")
export const fetchTokenizerVocab = (value: TokenizerFetchRequest) =>
  api<TokenizerVocabDto>("/admin/tokenizer-vocabs", json("POST", value))
export const deleteTokenizerVocab = (value: TokenizerFetchRequest) =>
  api<void>("/admin/tokenizer-vocabs", json("DELETE", value))
