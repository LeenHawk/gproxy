import type { DefaultModelDto, DefaultModelPricingDto } from "@/generated/sdk"

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
