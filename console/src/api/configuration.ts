//! The seam the sdk's configuration families land on. **Nothing here calls
//! anything, on purpose.**
//!
//! As of `db524db8` the routes exist: `/admin/api` mounts the handle's half —
//! 17 families and their extras — next to the identity half in
//! `crates/gproxy-host-axum/src/admin/config.rs`. What does not exist is a
//! console for them: providers, credentials, models, routes, settings,
//! pricing, rewrite, endpoints, transfer, connectivity and the tokenizer are
//! each a page with real shape to it, and this phase deliberately delivers the
//! shell, sign-in and the identity and self-service surfaces rather than a
//! thin pass over twenty more tables.
//!
//! The TypeScript is already there — `console/src/generated/sdk` has every one
//! of these DTOs — so what a configuration page needs is this file turning
//! into the same `family` factory [`@/api/admin`](./admin.ts) uses, and a
//! `configuration` section in `@/capability/navigation`.
//!
//! The table below is transcribed from `config.rs`'s router, in its order. It
//! is written down rather than left implicit so that "the console has no
//! configuration pages" is a statement about this console and not a question
//! about the server.

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
