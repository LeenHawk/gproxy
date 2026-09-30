//! The session: who is signed in, and the console context derived from it.
//!
//! One query answers both, so it is also the query that decides whether the
//! sign-in page or the shell is rendered: a 401 is not an error to report, it
//! is the answer "nobody".
//!
//! The portal establishes the session; the admin context supplies its scopes
//! and management sections. Each scope owns an independent query cache.

import { useQuery, useQueryClient, type UseQueryResult } from "@tanstack/react-query"
import { createContext, useCallback, useContext, useEffect } from "react"
import { ApiError, UNAUTHORIZED_EVENT, setAdminScope } from "@/api/client"
import { context as fetchContext, type SessionContext } from "@/api/session"
import { type ConsoleContext } from "@/capability/capability"

export const SESSION_KEY = ["session", "context"] as const

export const ConsoleContextStore = createContext<ConsoleContext | null>(null)

/**
 * The capability set of the signed-in caller.
 *
 * Throws outside the shell, which is deliberate: a page that renders without
 * a session is a routing bug, and a silent `null` would turn it into an empty
 * table that looks like an empty instance.
 */
export function useConsoleContext() {
  const value = useContext(ConsoleContextStore)
  if (!value) throw new Error("useConsoleContext must be used inside the signed-in shell")
  return value
}

export function useSessionContext(): UseQueryResult<SessionContext, ApiError> {
  return useQuery<SessionContext, ApiError>({ queryKey: SESSION_KEY, queryFn: fetchContext,
    retry: false, refetchOnWindowFocus: true, staleTime: 30_000 })
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
    setAdminScope(null)
    void client.resetQueries()
  }, [client])
  useEffect(() => {
    const refresh = () => { void client.invalidateQueries({ queryKey: SESSION_KEY }) }
    window.addEventListener("gproxy:context-refresh", refresh)
    window.addEventListener(UNAUTHORIZED_EVENT, reset)
    return () => { window.removeEventListener(UNAUTHORIZED_EVENT, reset); window.removeEventListener("gproxy:context-refresh", refresh) }
  }, [reset, client])
}
