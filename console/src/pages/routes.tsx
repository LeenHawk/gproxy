//! The route table, and the one thing it does beyond matching: refuse.
//!
//! Every route here has a navigation entry in `@/capability/navigation`, and
//! [`mayEnter`] is asked before a page is rendered. That matters for a route
//! reached by typing rather than clicking — a bookmark, a shared link, or a
//! session that lost a capability while the tab was open — which the sidebar
//! alone cannot guard.

import { lazy, Suspense, type ComponentType } from "react"
import { useTranslation } from "react-i18next"
import { mayEnter } from "@/capability/navigation"
import { useConsoleContext } from "@/capability/session"
import { EmptyNotice, LoadingRows } from "@/components/state"
import { Page, PageHeader } from "@/components/page"
import { useRoute, useScrollReset } from "@/lib/router"

const AboutPage = lazy(() => import("@/pages/about").then(module => ({ default: module.AboutPage })))
const ApiKeysPage = lazy(() => import("@/pages/identity/families").then(module => ({ default: module.ApiKeysPage })))
const OAuthClientsPage = lazy(() => import("@/pages/identity/families").then(module => ({ default: module.OAuthClientsPage })))
const OrganizationsPage = lazy(() => import("@/pages/identity/families").then(module => ({ default: module.OrganizationsPage })))
const PermissionsPage = lazy(() => import("@/pages/identity/families").then(module => ({ default: module.PermissionsPage })))
const RateLimitsPage = lazy(() => import("@/pages/identity/families").then(module => ({ default: module.RateLimitsPage })))
const TeamsPage = lazy(() => import("@/pages/identity/families").then(module => ({ default: module.TeamsPage })))
const UsersPage = lazy(() => import("@/pages/identity/families").then(module => ({ default: module.UsersPage })))
const AuditPage = lazy(() => import("@/pages/identity/audit").then(module => ({ default: module.AuditPage })))
const SessionsPage = lazy(() => import("@/pages/identity/sessions").then(module => ({ default: module.SessionsPage })))
const AccountPage = lazy(() => import("@/pages/self/account").then(module => ({ default: module.AccountPage })))
const KeysPage = lazy(() => import("@/pages/self/keys").then(module => ({ default: module.KeysPage })))
const ModelsPage = lazy(() => import("@/pages/self/models").then(module => ({ default: module.ModelsPage })))
const OverviewPage = lazy(() => import("@/pages/self/overview").then(module => ({ default: module.OverviewPage })))
const RequestsPage = lazy(() => import("@/pages/self/requests").then(module => ({ default: module.RequestsPage })))
const UsagePage = lazy(() => import("@/pages/self/usage").then(module => ({ default: module.UsagePage })))
const ProvidersPage = lazy(() => import("@/pages/providers").then(module => ({ default: module.ProvidersPage })))
const RuleSetsPage = lazy(() => import("@/pages/rules").then(module => ({ default: module.RuleSetsPage })))
const ClientsPage = lazy(() => import("@/pages/clients").then(module => ({ default: module.ClientsPage })))
const UpdatePage = lazy(() => import("@/pages/update").then(module => ({ default: module.UpdatePage })))
const SettingsPage = lazy(() => import("@/pages/settings").then(module => ({ default: module.SettingsPage })))
const TokenizerPage = lazy(() => import("@/pages/tokenizer").then(module => ({ default: module.TokenizerPage })))
const ModelRoutesPage = lazy(() => import("@/pages/model-routes").then(module => ({ default: module.ModelRoutesPage })))
const TransferPage = lazy(() => import("@/pages/transfer").then(module => ({ default: module.TransferPage })))
const ModelCatalogPage = lazy(() => import("@/pages/model-catalog").then(module => ({ default: module.ModelCatalogPage })))
const GlobalUsagePage = lazy(() => import("@/pages/observation/usage").then(module => ({ default: module.GlobalUsagePage })))
const DownstreamLogsPage = lazy(() => import("@/pages/observation/logs").then(module => ({ default: module.DownstreamLogsPage })))
const UpstreamLogsPage = lazy(() => import("@/pages/observation/logs").then(module => ({ default: module.UpstreamLogsPage })))

const ROUTES: Record<string, ComponentType> = {
  "/about": AboutPage,
  "/model-routes": ModelRoutesPage,
  "/transfer": TransferPage,
  "/": OverviewPage,
  "/keys": KeysPage,
  "/models": ModelsPage,
  "/usage": UsagePage,
  "/observation/usage": GlobalUsagePage,
  "/observation/downstream": DownstreamLogsPage,
  "/observation/upstream": UpstreamLogsPage,
  "/requests": RequestsPage,
  "/account": AccountPage,
  "/model-catalog": ModelCatalogPage,
  "/settings": SettingsPage,
  "/settings/transfer": SettingsPage,
  "/rule-sets": RuleSetsPage,
  "/update": UpdatePage,
  "/clients": ClientsPage,
  "/tokenizer": TokenizerPage,
  "/identity/users": UsersPage,
  "/identity/api-keys": ApiKeysPage,
  "/identity/organizations": OrganizationsPage,
  "/identity/teams": TeamsPage,
  "/identity/permissions": PermissionsPage,
  "/identity/rate-limits": RateLimitsPage,
  "/identity/oauth-clients": OAuthClientsPage,
  "/identity/sessions": SessionsPage,
  "/identity/audit": AuditPage,
}

function NotFound() {
  const { t } = useTranslation()
  return (
    <Page>
      <PageHeader title={t("state.notFoundTitle")} />
      <EmptyNotice title={t("state.notFoundTitle")} />
    </Page>
  )
}

function Forbidden() {
  const { t } = useTranslation()
  return (
    <Page>
      <PageHeader title={t("state.forbidden")} />
      <EmptyNotice title={t("state.forbidden")} />
    </Page>
  )
}

export function Routes() {
  const route = useRoute()
  const context = useConsoleContext()
  useScrollReset(route)

  const provider = /^\/providers\/([^/]+)(?:\/(credentials|models|settings|rules|routing|operations|endpoints))?$/.exec(route)
  if (provider || route === "/providers") {
    if (!mayEnter(context, route)) return <Forbidden />
    const providerId = provider ? decodeURIComponent(provider[1]) : undefined
    const tab = provider?.[2]
    return <Suspense fallback={<LoadingRows />}><ProvidersPage providerId={providerId} tab={tab === "operations" ? "routing" : tab === "endpoints" ? "settings" : tab ?? "credentials"} /></Suspense>
  }
  const Match = ROUTES[route]
  if (!Match) return <NotFound />
  if (!mayEnter(context, route)) return <Forbidden />
  return <Suspense fallback={<LoadingRows />}><Match /></Suspense>
}
