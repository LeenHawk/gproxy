import type { DefaultModelDto, DefaultModelPricingDto, ProviderDto, ProviderModelDto } from "@/generated/sdk"

export function defaultMetadata(model: DefaultModelDto) {
  const metadata: Record<string, unknown> = { ...model }
  for (const key of ["modelId", "pricing", "displayName", "contextWindow", "maxOutputTokens"]) delete metadata[key]
  return { ...metadata, display_name: model.displayName, context_window: model.contextWindow, max_output_tokens: model.maxOutputTokens }
}

export function modelBasename(name: string) { return name.trim().split("/").at(-1) ?? "" }

/** Same lookup as the backend: exact first, otherwise a unique basename. */
export function matchingModel<T>(rows: readonly T[], name: string, key: (row: T) => string): T | undefined {
  const needle = name.trim().toLowerCase()
  if (!needle) return undefined
  const exact = rows.find(row => key(row).toLowerCase() === needle)
  if (exact) return exact
  const basename = modelBasename(needle)
  const matches = rows.filter(row => modelBasename(key(row)).toLowerCase() === basename)
  return matches.length === 1 ? matches[0] : undefined
}

/** Bundled price patterns use *basename*: the most specific fragment wins. */
export function defaultPriceFor(models: readonly DefaultModelDto[], name: string): DefaultModelPricingDto | undefined {
  const needle = modelBasename(name).toLowerCase()
  let best: DefaultModelPricingDto | undefined
  let length = 0
  for (const model of models) {
    const price = model.pricing
    if (!price || !price.modelPattern.startsWith("*") || !price.modelPattern.endsWith("*")) continue
    const fragment = price.modelPattern.slice(1, -1).toLowerCase()
    if (fragment.length > length && needle.includes(fragment)) { best = price; length = fragment.length }
  }
  return best
}

/** Associate catalog rows with saved provider models, preferring explicit IDs. */
export function providersByModel<T extends { name: string; local?: { id: string } }>(
  rows: readonly T[], providers: readonly ProviderDto[], bindings: readonly ProviderModelDto[],
): Map<T, ProviderDto[]> {
  const providersById = new Map(providers.map(provider => [provider.id, provider]))
  const localModels = new Map(rows.filter(row => row.local).map(row => [row.local!.id, row]))
  const result = new Map<T, Map<string, ProviderDto>>()
  for (const binding of bindings) {
    const provider = providersById.get(binding.providerId)
    if (!provider) continue
    const row = binding.modelId ? localModels.get(binding.modelId) : matchingModel(rows, binding.upstreamName, row => row.name)
    if (!row) continue
    const attached = result.get(row) ?? new Map<string, ProviderDto>()
    attached.set(provider.id, provider)
    result.set(row, attached)
  }
  return new Map([...result].map(([row, providers]) => [row, [...providers.values()].sort((a, b) => a.name.localeCompare(b.name))]))
}
