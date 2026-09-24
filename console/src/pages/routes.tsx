//! The route table, and the one thing it does beyond matching: refuse.
//!
//! Every route here has a navigation entry in `@/capability/navigation`, and
//! [`mayEnter`] is asked before a page is rendered. That matters for a route
//! reached by typing rather than clicking — a bookmark, a shared link, or a
//! session that lost a capability while the tab was open — which the sidebar
//! alone cannot guard.

import { useTranslation } from "react-i18next"
import { mayEnter } from "@/capability/navigation"
import { useConsoleContext } from "@/capability/session"
import { EmptyNotice } from "@/components/state"
import { Page, PageHeader } from "@/components/page"
import { useRoute, useScrollReset } from "@/lib/router"
import {
  ApiKeysPage, OAuthClientsPage, OrganizationsPage, PermissionsPage, RateLimitsPage, TeamsPage, UsersPage,
} from "@/pages/identity/families"
import { AuditPage } from "@/pages/identity/audit"
import { SessionsPage } from "@/pages/identity/sessions"
import { AccountPage } from "@/pages/self/account"
import { KeysPage } from "@/pages/self/keys"
import { ModelsPage } from "@/pages/self/models"
import { OverviewPage } from "@/pages/self/overview"
import { QuotaPage } from "@/pages/self/quota"
import { RequestsPage } from "@/pages/self/requests"
import { UsagePage } from "@/pages/self/usage"
import { ProvidersPage } from "@/pages/providers"

import { RuleSetsPage } from "@/pages/rules"
import { ClientsPage } from "@/pages/clients"
import { UpdatePage } from "@/pages/update"
import { SettingsPage } from "@/pages/settings"
import { TokenizerPage } from "@/pages/tokenizer"

import { ModelCatalogPage } from "@/pages/model-catalog"

const ROUTES: Record<string, () => React.ReactElement> = {
  "/": OverviewPage,
  "/keys": KeysPage,
  "/models": ModelsPage,
  "/usage": UsagePage,
  "/quota": QuotaPage,
  "/requests": RequestsPage,
  "/account": AccountPage,
  "/model-catalog": ModelCatalogPage,
  "/settings": SettingsPage,
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
    return <ProvidersPage providerId={providerId} tab={tab === "operations" ? "routing" : tab === "endpoints" ? "settings" : tab ?? "credentials"} />
  }
  const Match = ROUTES[route]
  if (!Match) return <NotFound />
  if (!mayEnter(context, route)) return <Forbidden />
  return <Match />
}
