import { useState } from "react"

export const PAGE_SIZES = [10, 20, 50] as const
export type PageSize = typeof PAGE_SIZES[number]

export function usePagination(resetKey = "") {
  const [state, setState] = useState({ page: 1, pageSize: 10 as PageSize, resetKey })
  if (state.resetKey !== resetKey) setState({ ...state, page: 1, resetKey })
  return {
    page: state.resetKey === resetKey ? state.page : 1,
    pageSize: state.pageSize,
    setPage: (page: number) => setState(previous => ({ ...previous, page, resetKey })),
    setPageSize: (pageSize: PageSize) => setState({ page: 1, pageSize, resetKey }),
  }
}
