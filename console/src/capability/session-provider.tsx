import { QueryClient, QueryClientProvider, useQueryClient } from "@tanstack/react-query"
import { useEffect, useState, type ReactNode } from "react"
import { ApiError, setAdminScope } from "@/api/client"
import { context as fetchContext, type SessionContext } from "@/api/session"
import { consoleContext } from "@/capability/capability"
import { ConsoleContextStore, SESSION_KEY } from "./session"
import { LoadingRows } from "@/components/state"

export function ConsoleContextProvider({ context, children }: { context: SessionContext; children: ReactNode }) {
  const root = useQueryClient()
  const selector = context.admin?.scope?.selector ?? "none"
  // A separate cache per caller/scope prevents late responses from entering
  // the new scope even when a transport cannot cancel its old request.
  const [scopedClient] = useState(() => new QueryClient({ defaultOptions: { queries: {
    staleTime: 10_000, refetchOnWindowFocus: false,
    retry: (attempt, error) => !(error instanceof ApiError && [401, 403].includes(error.status)) && attempt < 1,
  } } }))
  const [switching, setSwitching] = useState(false)
  useEffect(() => () => { void scopedClient.cancelQueries(); scopedClient.clear() }, [scopedClient])
  const switchScope = async (next: string) => {
    if (switching || scopedClient.isMutating() || !context.admin?.scopes.some(scope => scope.selector === next)) return
    setSwitching(true)
    await Promise.all([scopedClient.cancelQueries(), root.cancelQueries({ queryKey: SESSION_KEY })])
    setAdminScope(next, context.admin.scopeHeader)
    try { root.setQueryData(SESSION_KEY, await fetchContext()) }
    catch (error) { setAdminScope(context.admin.scope?.selector ?? null); throw error }
    finally { setSwitching(false) }
  }
  return <ConsoleContextStore.Provider value={{ ...consoleContext(context), switchScope }}>
    {switching ? <LoadingRows /> : <QueryClientProvider client={scopedClient}><div key={`${context.user.id}:${selector}`}>{children}</div></QueryClientProvider>}
  </ConsoleContextStore.Provider>
}
