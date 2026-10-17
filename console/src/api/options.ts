import type { Family, ListFilter } from "./admin"
import type { OptionSource, SelectOption } from "@/components/searchable-select"
import { providerModels } from "./configuration"
import { models } from "./models"
import * as portal from "./portal"

/** Fetch one requested page only; never walk the directory for a picker. */
export function optionSource<D, W, P>(family: Family<D, W, P>, option: (row: D) => SelectOption, filter: ListFilter = {}): OptionSource {
  return {
    key: ["admin", family.path, filter],
    list: async (search, page, pageSize) => {
      const result = await family.list({ ...filter, search: search.trim() || undefined, page, pageSize })
      return { items: result.items.map(option), total: result.total }
    },
  }
}

/**
 * Model names a quota, budget or permission pattern can name: the chosen
 * provider's models, else the catalog, else the models the caller can reach.
 */
export function modelSource(has: (capability: string) => boolean, providerId?: string): OptionSource {
  if (providerId && has("configuration.provider-models")) return optionSource(providerModels, row => ({ value: row.upstreamName, label: row.upstreamName }), { providerId })
  if (has("configuration.models")) return optionSource(models, row => ({ value: row.name, label: row.name }))
  return {
    key: ["portal", "models"],
    list: async (search, page, pageSize) => {
      const needle = search.trim().toLowerCase()
      const names = (await portal.models()).map(row => row.name).filter(name => name.toLowerCase().includes(needle))
      return { items: names.slice((page - 1) * pageSize, page * pageSize).map(value => ({ value, label: value })), total: names.length }
    },
  }
}
