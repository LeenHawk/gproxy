import { lazy, Suspense, useEffect } from "react"
import { QueryClient, QueryClientProvider, useMutation, useQuery, useQueryClient } from "@tanstack/react-query"
import { useTranslation } from "react-i18next"
import { login, logout, session, setup } from "@/api/auth"
import { AppShell } from "@/components/app-shell"
import { AuthPanel } from "@/components/auth/auth-panel"
import { QueryState } from "@/components/query-state"
import { Toaster } from "@/components/ui/sonner"
import { TooltipProvider } from "@/components/ui/tooltip"
import { useAdminLocation } from "@/lib/admin-route"

const ChannelsPage = lazy(() => import("@/pages/channels").then((module) => ({ default: module.ChannelsPage })))
const KeysPage = lazy(() => import("@/pages/keys").then((module) => ({ default: module.KeysPage })))
const OverviewPage = lazy(() => import("@/pages/overview").then((module) => ({ default: module.OverviewPage })))
const ProvidersPage = lazy(() => import("@/pages/providers").then((module) => ({ default: module.ProvidersPage })))
const RoutesPage = lazy(() => import("@/pages/routes").then((module) => ({ default: module.RoutesPage })))
const UsagePage = lazy(() => import("@/pages/usage").then((module) => ({ default: module.UsagePage })))
const LogsPage = lazy(() => import("@/pages/logs").then((module) => ({ default: module.LogsPage })))
const PricingPage = lazy(() => import("@/pages/pricing").then((module) => ({ default: module.PricingPage })))
const SettingsPage = lazy(() => import("@/pages/settings").then((module) => ({ default: module.SettingsPage })))

const queryClient = new QueryClient({ defaultOptions: { queries: { staleTime: 15_000, retry: 1 } } })

function ConsoleApp() {
  const { t } = useTranslation()
  const client = useQueryClient()
  const { route } = useAdminLocation()
  const sessionQuery = useQuery({ queryKey: ["session"], queryFn: session, retry: false })
  const auth = useMutation({
    mutationFn: ({ username, password, firstBoot }: { username: string; password: string; firstBoot: boolean }) => firstBoot ? setup({ username, password }) : login({ username, password }),
    onSuccess: () => void client.invalidateQueries({ queryKey: ["session"] }),
  })
  const signOut = useMutation({ mutationFn: logout, onSuccess: () => { client.clear(); void client.invalidateQueries({ queryKey: ["session"] }) } })

  useEffect(() => {
    const unauthorized = () => void client.invalidateQueries({ queryKey: ["session"] })
    window.addEventListener("gproxy:unauthorized", unauthorized)
    return () => window.removeEventListener("gproxy:unauthorized", unauthorized)
  }, [client])

  if (sessionQuery.isLoading || sessionQuery.error) {
    return <main className="mx-auto max-w-2xl px-5 py-16"><QueryState loading={sessionQuery.isLoading} error={sessionQuery.error ? t("auth.session.loadError") : ""}>{null}</QueryState></main>
  }
  const state = sessionQuery.data
  if (!state?.user) {
    return <AuthPanel setup={state?.setup_required ?? false} pending={auth.isPending} failed={auth.isError} onSubmit={(username, password) => auth.mutate({ username, password, firstBoot: state?.setup_required ?? false })} />
  }
  const page = {
    overview: <OverviewPage />,
    providers: <ProvidersPage />,
    routes: <RoutesPage />,
    keys: <KeysPage />,
    usage: <UsagePage />,
    logs: <LogsPage />,
    channels: <ChannelsPage />,
    pricing: <PricingPage />,
    settings: <SettingsPage />,
  }[route]
  return (
    <AppShell route={route} username={state.user.username} onLogout={() => signOut.mutate()}>
      <Suspense fallback={<QueryState loading error="">{null}</QueryState>}>{page}</Suspense>
    </AppShell>
  )
}

export function AdminSurface() {
  return (
    <QueryClientProvider client={queryClient}>
      <TooltipProvider><ConsoleApp /><Toaster /></TooltipProvider>
    </QueryClientProvider>
  )
}
