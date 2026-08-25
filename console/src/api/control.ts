import type { AliasDto } from "@/generated/AliasDto"
import type { AliasWriteRequest } from "@/generated/AliasWriteRequest"
import type { CredentialDto } from "@/generated/CredentialDto"
import type { CredentialWriteRequest } from "@/generated/CredentialWriteRequest"
import type { ModelAliasDto } from "@/generated/ModelAliasDto"
import type { ModelAliasWriteRequest } from "@/generated/ModelAliasWriteRequest"
import type { PriceRateDto } from "@/generated/PriceRateDto"
import type { PriceRateWriteRequest } from "@/generated/PriceRateWriteRequest"
import type { PriceRuleDto } from "@/generated/PriceRuleDto"
import type { PriceRuleWriteRequest } from "@/generated/PriceRuleWriteRequest"
import type { ProviderDto } from "@/generated/ProviderDto"
import type { ProviderWriteRequest } from "@/generated/ProviderWriteRequest"
import type { RouteDto } from "@/generated/RouteDto"
import type { RouteMemberDto } from "@/generated/RouteMemberDto"
import type { RouteMemberWriteRequest } from "@/generated/RouteMemberWriteRequest"
import type { RouteWriteRequest } from "@/generated/RouteWriteRequest"
import { api, json } from "@/api/client"

const save = <T>(path: string, value: T, id?: number) =>
  api(id == null ? path : `${path}/${id}`, json(id == null ? "POST" : "PATCH", value))

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
