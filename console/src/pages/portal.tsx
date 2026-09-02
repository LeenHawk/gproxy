import { useEffect, useState } from "react"
import { useQueries, useQueryClient } from "@tanstack/react-query"
import { useTranslation } from "react-i18next"
import type { PortalContextDto } from "@/generated/PortalContextDto"
import {
  portalLogin,
  portalLogout,
  portalModels,
  portalQuotaWindows,
  portalRecentRequests,
  portalUsage,
  portalSession,
} from "@/api/portal"
import { AuthPanel } from "@/components/auth/auth-panel"
import { PortalDashboard } from "@/components/portal/portal-dashboard"
import { PortalShell } from "@/components/portal/portal-shell"
import type { UsageDays } from "@/components/portal/usage-panel"

type PortalSession = { context: PortalContextDto }

function continueOAuth() {
  const value = new URLSearchParams(window.location.search).get("oauth_return")
  if (value?.startsWith("/") && !value.startsWith("//")) window.location.assign(value)
}

export function PortalPage() {
  const { t } = useTranslation()
  const queryClient = useQueryClient()
  const [session, setSession] = useState<PortalSession | null>(null)
  const [sessionLoading, setSessionLoading] = useState(true)
  const [loginPending, setLoginPending] = useState(false)
  const [loginFailed, setLoginFailed] = useState(false)
  const [usageDays, setUsageDays] = useState<UsageDays>(7)
  const authenticated = session != null
  const recentEnabled = session?.context.recent_requests_enabled ?? false
  const [modelsQuery, usageQuery, quotaQuery, recentQuery] = useQueries({ queries: [
    {
      queryKey: ["portal", "models"],
      queryFn: ({ signal }) => portalModels(signal),
      enabled: authenticated,
    },
    {
      queryKey: ["portal", "usage", usageDays],
      queryFn: ({ signal }) => {
        const to = Math.floor(Date.now() / 1_000) + 1
        return portalUsage({ from: to - usageDays * 86_400, to }, signal)
      },
      enabled: authenticated,
    },
    {
      queryKey: ["portal", "quota-windows"],
      queryFn: ({ signal }) => portalQuotaWindows(signal),
      enabled: authenticated,
    },
    {
      queryKey: ["portal", "recent-requests"],
      queryFn: ({ signal }) => portalRecentRequests({ limit: 20 }, signal),
      enabled: authenticated && recentEnabled,
    },
  ] })

  useEffect(() => {
    void portalSession()
      .then((status) => {
        if (status.user) {
          setSession({ context: status.user })
          continueOAuth()
        }
      })
      .finally(() => setSessionLoading(false))
  }, [])

  async function login(username: string, password: string) {
    setLoginPending(true)
    setLoginFailed(false)
    try {
      const context = await portalLogin({ username, password })
      setSession({ context })
      continueOAuth()
    } catch {
      setLoginFailed(true)
    } finally {
      setLoginPending(false)
    }
  }

  async function logout() {
    try {
      await portalLogout()
      queryClient.clear()
      window.location.assign("/")
    } catch {
      return
    }
  }

  if (sessionLoading) return <PortalShell context={null}><p>{t("portal.login.checking")}</p></PortalShell>

  if (!session) {
    return (
      <AuthPanel
        setup={false}
        audience="portal"
        pending={loginPending}
        failed={loginFailed}
        onSubmit={(username, password) => void login(username, password)}
      />
    )
  }

  return (
      <PortalShell context={session.context} onLogout={() => void logout()}>
      <PortalDashboard
        context={session.context}
        apiKey={t("portal.connect.keyPlaceholder")}
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
