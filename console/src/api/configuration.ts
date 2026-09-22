//! Typed access to the SDK configuration routes mounted under `/admin/api`.

import type {
  ConnectionProfileDto, Page, ChannelDescriptor, CredentialDto, CredentialPatch, CredentialWrite,
  ProviderDto, ProviderPatch, ProviderWrite,
  ProviderModelDto, ProviderModelPatch, ProviderModelWrite,
} from "@/generated/sdk"
import { family } from "@/api/admin"
import { api } from "@/api/client"

/** The configuration collection routes, relative to `/admin/api`. */
export const CONFIGURATION_FAMILIES = {
  providers: "/providers",
  credentials: "/credentials",
  models: "/models",
  providerModels: "/provider-models",
  routes: "/routes",
  routeMembers: "/route-members",
  exposedModels: "/exposed-models",
  connectionProfiles: "/connection-profiles",
  ruleSets: "/rule-sets",
  rules: "/rules",
  providerRuleSets: "/provider-rule-sets",
  operationRules: "/operation-rules",
  operationEndpoints: "/operation-endpoints",
  quotas: "/quotas",
  priceRules: "/price-rules",
  priceRates: "/price-rates",
  priceTiers: "/price-tiers",
} as const

/** The singletons and one-off operations that are not a family. */
export const CONFIGURATION_OPERATIONS = {
  settings: "/settings",
  export: "/export",
  import: "/import",
  connectivityTest: "/connectivity/test",
  channels: "/channels",
  tlsPresets: "/tls-presets",
  rulePresets: "/rule-presets",
  defaultModelCatalog: "/default-model-catalog",
  tokenizerVocabularies: "/tokenizer-vocabs",
  tokenizerAuth: "/tokenizer-auth",
} as const

export type ConfigurationFamily = keyof typeof CONFIGURATION_FAMILIES

export const PROVIDERS_READ = "configuration.providers"
export const PROVIDER_NAV_KEY = ["admin", "/providers", "navigation"] as const

export const providers = family<ProviderDto, Partial<ProviderWrite>, Partial<ProviderPatch>>(CONFIGURATION_FAMILIES.providers)
export const credentials = family<CredentialDto, Partial<CredentialWrite>, Partial<CredentialPatch>>(CONFIGURATION_FAMILIES.credentials)
export const providerModels = family<ProviderModelDto, Partial<ProviderModelWrite>, Partial<ProviderModelPatch>>(CONFIGURATION_FAMILIES.providerModels)
export const channels = () => api<Array<ChannelDescriptor>>("/admin/api/channels")

/** The sidebar needs the whole provider directory, not the first API page. */
export async function providerDirectory() {
  const rows: Array<ProviderDto> = []
  let page = 1
  while (true) {
    const result = await providers.list({ page, pageSize: 500 })
    rows.push(...result.items)
    if (rows.length >= result.total || result.items.length === 0) break
    page += 1
  }
  return rows.sort((a, b) => a.name.localeCompare(b.name))
}

export const providerPath = (id: string) => `/providers/${encodeURIComponent(id)}`

export async function connectionProfiles() {
  const rows: Array<ConnectionProfileDto> = []
  let page = 1
  while (true) {
    const result = await api<Page<ConnectionProfileDto>>(`/admin/api/connection-profiles?page=${page}&pageSize=500`)
    rows.push(...result.items)
    if (rows.length >= result.total || result.items.length === 0) return rows
    page += 1
  }
}
