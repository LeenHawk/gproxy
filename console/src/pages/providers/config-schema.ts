import type { ConfigKey } from "@/generated/sdk"

export type ConfigObject = Record<string, unknown>
export type ControlKind =
  | "string"
  | "password"
  | "bool"
  | "integer"
  | "list"
  | "pairs"
  | "choice"
  | "dialects"
  | "routing"
  | "model-routing"

export const choices: Record<string, Array<string>> = {
  credential_strategy: ["round_robin", "earliest_reset", "sticky", "round_robin_affinity"],
  fallback_mode: ["off", "default", "models"],
  account_type: ["individual", "business", "enterprise"],
  product: ["platform", "code"],
  tier: ["zen", "go"],
  login_provider: ["github", "google"],
}

export const dialects = ["openai", "openai_chat", "claude", "gemini", "openai_responses_websocket"]

export function controlFor(field: ConfigKey): ControlKind {
  if (choices[field.name]) return "choice"
  if (["quota_api_key"].includes(field.name)) return "password"
  if (field.name === "allowed_headers" || field.name === "fallback_models") return "list"
  if (["headers", "models", "endpoints"].includes(field.name)) return "pairs"
  if (field.name === "dialects") return "dialects"
  if (field.name === "provider") return "routing"
  if (field.name === "model_providers") return "model-routing"
  if (field.kind === "bool" || field.kind === "integer" || field.kind === "string") return field.kind
  throw new Error(`No provider form control for ${field.name}`)
}

export function booleanDefault(name: string) {
  return ["usage_accounting", "normalize_service_tier", "synthesize_cli_identity"].includes(name)
}

export function setConfigValue(config: ConfigObject, name: string, value: unknown): ConfigObject {
  const next = { ...config }
  if (value === undefined) delete next[name]
  else next[name] = value
  return next
}

/** Resolve only the address placeholders declared by the channel. Values stay unset. */
export function configPlaceholder(field: ConfigKey | undefined, fields: readonly ConfigKey[], config: ConfigObject, baseUrl: string): string | undefined {
  if (!field?.placeholder) return undefined
  let placeholder = field.placeholder
  if (placeholder === "https://{location}-aiplatform.googleapis.com" && config.location === "global") return "https://aiplatform.googleapis.com"
  placeholder = placeholder.replace(/\{([a-z_]+)\}/g, (token, name: string) => {
    const value = name === "base_url" ? baseUrl : config[name]
    return (typeof value === "string" && value.trim()) || fields.find(candidate => candidate.name === name)?.placeholder || token
  })
  return placeholder
}
