//! The session: who is signed in, and the console context derived from it.
//!
//! One query answers both, so it is also the query that decides whether the
//! sign-in page or the shell is rendered: a 401 is not an error to report, it
//! is the answer "nobody".
//!
//! Today the query is `GET /portal/api/context`; when `/admin/api/context`
//! lands it is that, and the change is confined to this file and to
//! [`consoleContext`](@/capability/capability). `retry: false` matters —
//! TanStack Query's default of three attempts with a backoff would make an
//! ordinary signed-out visit take a couple of seconds to show a form.

import { useQuery, useQueryClient, type UseQueryResult } from "@tanstack/react-query"
import { createContext, useCallback, useContext, useEffect, type ReactNode } from "react"
import { ApiError, UNAUTHORIZED_EVENT } from "@/api/client"
import { context as fetchContext } from "@/api/session"
import { consoleContext, type ConsoleContext } from "@/capability/capability"
import type { PortalContextDto } from "@/generated/app"

export const SESSION_KEY = ["session", "context"] as const

const Context = createContext<ConsoleContext | null>(null)

/**
 * The capability set of the signed-in caller.
 *
 * Throws outside the shell, which is deliberate: a page that renders without
 * a session is a routing bug, and a silent `null` would turn it into an empty
 * table that looks like an empty instance.
 */
export function useConsoleContext() {
  const value = useContext(Context)
  if (!value) throw new Error("useConsoleContext must be used inside the signed-in shell")
  return value
}

export function ConsoleContextProvider({ context, children }: { context: PortalContextDto; children: ReactNode }) {
  return <Context.Provider value={consoleContext(context)}>{children}</Context.Provider>
}

export function useSessionContext(): UseQueryResult<PortalContextDto, ApiError> {
  return useQuery<PortalContextDto, ApiError>({
    queryKey: SESSION_KEY,
    queryFn: fetchContext,
    retry: false,
    // A session that ended elsewhere should be noticed on the next glance at
    // the tab rather than on the next click that fails.
    refetchOnWindowFocus: true,
    staleTime: 30_000,
  })
}

/**
 * Drop every cached answer the moment any request reports a dead session.
 *
 * `resetQueries` rather than `clear`: clearing empties the cache but leaves a
 * mounted observer holding the last result it was given, so the shell would
 * keep rendering an account the console can no longer read. Resetting drops
 * the data *and* re-asks the one query the shell depends on, which is what
 * turns a dead session into the sign-in page.
 *
 * The guard is not an optimization. Without it the very first load of a
 * signed-out console spins forever: the session query itself answers 401,
 * which raises the event, which resets the cache, which makes the query
 * refetch, which answers 401. A console with no session cannot lose one — so
 * the reset only fires when there is a session to end, and the refetch it
 * causes finds `undefined` here and stops.
 */
export function useUnauthorizedReset() {
  const client = useQueryClient()
  const reset = useCallback(() => {
    if (!client.getQueryData(SESSION_KEY)) return
    void client.resetQueries()
  }, [client])
  useEffect(() => {
    window.addEventListener(UNAUTHORIZED_EVENT, reset)
    return () => window.removeEventListener(UNAUTHORIZED_EVENT, reset)
  }, [reset])
}
