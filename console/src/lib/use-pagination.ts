import { useState } from "react"

export const PAGE_SIZES = [10, 20, 50] as const
export type PageSize = typeof PAGE_SIZES[number]

function readPageSize(storageKey: string): PageSize {
  try {
    const value = Number(localStorage.getItem(`gproxy.table.${storageKey}.page-size`))
    return PAGE_SIZES.includes(value as PageSize) ? value as PageSize : 10
  } catch { return 10 }
}

export function usePagination(storageKey = "", resetKey = "") {
  const initial = () => ({ page: 1, pageSize: storageKey ? readPageSize(storageKey) : 10 as PageSize, storageKey, resetKey })
  const [state, setState] = useState(initial)
  const current = state.storageKey !== storageKey ? initial() : state.resetKey !== resetKey ? { ...state, page: 1, resetKey } : state
  if (current !== state) setState(current)
  return {
    page: current.page,
    pageSize: current.pageSize,
    setPage: (page: number) => setState(previous => ({ ...previous, page })),
    setPageSize: (pageSize: PageSize) => {
      if (storageKey) {
        try { localStorage.setItem(`gproxy.table.${storageKey}.page-size`, String(pageSize)) } catch { /* The choice remains usable for this session. */ }
      }
      setState({ page: 1, pageSize, storageKey, resetKey })
    },
  }
}
