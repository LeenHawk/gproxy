import { useState } from "react"

export const PAGE_SIZES = [10, 20, 50, 100] as const
export type PageSize = (typeof PAGE_SIZES)[number]

type StoredColumns = { version: 1; hidden: Array<string> }

function readHidden(storageKey: string) {
  try {
    const value = JSON.parse(window.localStorage.getItem(storageKey) ?? "null") as StoredColumns | null
    return value?.version === 1 ? new Set(value.hidden) : new Set<string>()
  } catch {
    return new Set<string>()
  }
}

export function useColumnVisibility(storageKey: string) {
  const [hidden, setHidden] = useState(() => readHidden(storageKey))
  const toggle = (key: string) => {
    setHidden((current) => {
      const next = new Set(current)
      if (next.has(key)) next.delete(key)
      else next.add(key)
      try {
        window.localStorage.setItem(storageKey, JSON.stringify({ version: 1, hidden: [...next] } satisfies StoredColumns))
      } catch {
        // Column visibility remains usable for this session when storage is unavailable.
      }
      return next
    })
  }
  return { hidden, toggle }
}

// The page size an operator last chose for a list, or the list's default.
// A stored value outside the current choices falls back to the default.
export function readPageSize(storageKey: string, fallback: PageSize): PageSize {
  try {
    const value = Number(window.localStorage.getItem(pageSizeKey(storageKey)))
    return (PAGE_SIZES as ReadonlyArray<number>).includes(value) ? (value as PageSize) : fallback
  } catch {
    return fallback
  }
}

export function storePageSize(storageKey: string, pageSize: PageSize) {
  try {
    window.localStorage.setItem(pageSizeKey(storageKey), String(pageSize))
  } catch {
    // The choice still applies for this session when storage is unavailable.
  }
}

export function usePageSize(storageKey: string, fallback: PageSize) {
  const [pageSize, setState] = useState(() => readPageSize(storageKey, fallback))
  const setPageSize = (next: PageSize) => {
    storePageSize(storageKey, next)
    setState(next)
  }
  return [pageSize, setPageSize] as const
}

function pageSizeKey(storageKey: string) {
  return `gproxy.table.${storageKey}.page-size`
}
