import type { Family, ListFilter } from "./admin"
import type { OptionSource, SelectOption } from "@/components/searchable-select"

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
