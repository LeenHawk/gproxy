import { useState } from "react"
import { useQueries, useQueryClient } from "@tanstack/react-query"
import type { PortalContextDto } from "@/generated/PortalContextDto"
import {
  portalContext,
  portalModels,
  portalQuotaWindows,
  portalRecentRequests,
  portalUsage,
} from "@/api/portal"
import { PortalDashboard } from "@/components/portal/portal-dashboard"
import { PortalLogin } from "@/components/portal/portal-login"
import { PortalShell } from "@/components/portal/portal-shell"
import type { UsageDays } from "@/components/portal/usage-panel"

type PortalSession = { apiKey: string; context: PortalContextDto }

export function PortalPage() {
  const queryClient = useQueryClient()
  const [session, setSession] = useState<PortalSession | null>(null)
  const [loginPending, setLoginPending] = useState(false)
  const [loginFailed, setLoginFailed] = useState(false)
  const [usageDays, setUsageDays] = useState<UsageDays>(7)
  const apiKey = session?.apiKey ?? ""
  const authenticated = session != null
  const recentEnabled = session?.context.recent_requests_enabled ?? false
  const [modelsQuery, usageQuery, quotaQuery, recentQuery] = useQueries({ queries: [
    {
      queryKey: ["portal", "models"],
      queryFn: ({ signal }) => portalModels(apiKey, signal),
      enabled: authenticated,
    },
    {
      queryKey: ["portal", "usage", usageDays],
      queryFn: ({ signal }) => {
        const to = Math.floor(Date.now() / 1_000) + 1
        return portalUsage(apiKey, { from: to - usageDays * 86_400, to }, signal)
      },
      enabled: authenticated,
    },
    {
      queryKey: ["portal", "quota-windows"],
      queryFn: ({ signal }) => portalQuotaWindows(apiKey, signal),
      enabled: authenticated,
    },
    {
      queryKey: ["portal", "recent-requests"],
      queryFn: ({ signal }) => portalRecentRequests(apiKey, { limit: 20 }, signal),
      enabled: authenticated && recentEnabled,
    },
  ] })

  async function login(key: string) {
    setLoginPending(true)
    setLoginFailed(false)
    try {
      const context = await portalContext(key)
      setSession({ apiKey: key, context })
    } catch {
      setLoginFailed(true)
    } finally {
      setLoginPending(false)
    }
  }

  function logout() {
    setSession(null)
    setUsageDays(7)
    setLoginFailed(false)
    void queryClient.cancelQueries({ queryKey: ["portal"] })
    queryClient.removeQueries({ queryKey: ["portal"] })
  }

  if (!session) {
    return (
      <PortalShell context={null}>
        <PortalLogin pending={loginPending} failed={loginFailed} onSubmit={(key) => void login(key)} />
      </PortalShell>
    )
  }

  return (
    <PortalShell context={session.context} onLogout={logout}>
      <PortalDashboard
        context={session.context}
        apiKey={session.apiKey}
        origin={window.location.origin}
        models={modelsQuery.data ?? []}
        modelsLoading={modelsQuery.isLoading}
        modelsError={modelsQuery.isError}
        usage={usageQuery.data}
        usageDays={usageDays}
        usageLoading={usageQuery.isLoading}
        usageError={usageQuery.isError}
        quotaWindows={quotaQuery.data ?? []}
        quotaLoading={quotaQuery.isLoading}
        quotaError={quotaQuery.isError}
        recentRequests={recentQuery.data ?? []}
        recentLoading={recentQuery.isLoading}
        recentError={recentQuery.isError}
        onUsageDaysChange={setUsageDays}
      />
    </PortalShell>
  )
}
