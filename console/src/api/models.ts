import { type ListFilter, type Family } from "@/api/admin"
import { configFamily as family } from "@/api/config-family"
import { api, json } from "@/api/client"
import type { ApplyDefaultPricesReportDto, DefaultModelCatalogDto, DiscoveredModelDto, ModelTestResultDto, PriceRuleDto, PriceRuleWrite, PriceRulePatch, PriceRateDto, PriceRateWrite, PriceRatePatch, PriceTierDto, PriceTierWrite, PriceTierPatch, ModelDto, ModelWrite, ModelPatch } from "@/generated/sdk"
export async function directory<D, W, P>(resource: Family<D, W, P>, filter: ListFilter = {}) {
  const rows: D[] = []
  for (let page = 1; ; page++) { const result = await resource.list({ ...filter, page, pageSize: 500 }); rows.push(...result.items); if (!result.items.length || rows.length >= result.total) return rows }
}
export const priceRules = family<PriceRuleDto, Partial<PriceRuleWrite>, Partial<PriceRulePatch>>("/price-rules")
export const priceRates = family<PriceRateDto, Partial<PriceRateWrite>, Partial<PriceRatePatch>>("/price-rates")
export const priceTiers = family<PriceTierDto, Partial<PriceTierWrite>, Partial<PriceTierPatch>>("/price-tiers")
export const discoverModels = (providerId: string, credentialId: string | null = null) => api<DiscoveredModelDto[]>("/admin/api/models/discover", json("POST", { providerId, credentialId }))
export const testModel = (providerId: string, model: string, credentialId: string | null = null) => api<ModelTestResultDto>("/admin/api/models/test", json("POST", { providerId, model, credentialId }))
export const defaultModels = () => api<DefaultModelCatalogDto>("/admin/api/default-model-catalog")
export const applyDefaultPrices = (providerId: string | null, modelIds: string[]) => api<ApplyDefaultPricesReportDto>("/admin/api/default-model-catalog/apply-prices", json("POST", { providerId, modelIds, overwrite: false }))

export const models = family<ModelDto, Partial<ModelWrite>, Partial<ModelPatch>>("/models")
