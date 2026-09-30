//! Typed access to the SDK configuration routes mounted under `/admin/api`.

import type {
  ConnectionProfileDto, ChannelDescriptor, CredentialDto, CredentialPatch, CredentialWrite,
  ProviderDto, ProviderPatch, ProviderWrite,
  ProviderModelDto, ProviderModelPatch, ProviderModelWrite,
} from "@/generated/sdk"
import { configFamily as family } from "@/api/config-family"
import { api } from "@/api/client"

/** The configuration collection routes, relative to `/admin/api`. */
export const CONFIGURATION_FAMILIES = {
  providers: "/providers",
  credentials: "/credentials",
  models: "/models",
  providerModels: "/provider-models",
  routes: "/routes",
  routeMembers: "/route-members",
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

export const providers = family<ProviderDto, Partial<ProviderWrite>, Partial<ProviderPatch>>(CONFIGURATION_FAMILIES.providers)
export const credentials = family<CredentialDto, Partial<CredentialWrite>, Partial<CredentialPatch>>(CONFIGURATION_FAMILIES.credentials)
export const providerModels = family<ProviderModelDto, Partial<ProviderModelWrite>, Partial<ProviderModelPatch>>(CONFIGURATION_FAMILIES.providerModels)
export const channels = () => api<Array<ChannelDescriptor>>("/admin/api/channels")

export const providerPath = (id: string) => `/providers/${encodeURIComponent(id)}`

export const connectionProfiles = family<ConnectionProfileDto, Partial<import("@/generated/sdk").ConnectionProfileWrite>, Partial<import("@/generated/sdk").ConnectionProfilePatch>>("/connection-profiles")
