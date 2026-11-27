import { useState } from "react"
import { useQuery } from "@tanstack/react-query"
import { useTranslation } from "react-i18next"
import { Plus } from "lucide-react"
import { directory } from "@/api/models"
import { providers as providerFamily } from "@/api/configuration"
import { readSettings, SETTINGS_ACCESS, SETTINGS_KEY } from "@/api/settings"
import { useConsoleContext } from "@/capability/session"
import { Page, PageHeader } from "@/components/page"
import { EmptyNotice, QueryState } from "@/components/state"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import { routeFamily, memberFamily } from "./families"
import { RouteCard, type RouteContext } from "./route-card"

/** What a fresh settings row allows when the caller cannot read the settings. */
const DEFAULT_ATTEMPTS = 6

export function ModelRoutesPage() {
  const { t } = useTranslation()
  const context = useConsoleContext()
  const [search, setSearch] = useState("")
  const [open, setOpen] = useState<string | null>(null)
  const [creating, setCreating] = useState(false)
  const routes = useQuery({ queryKey: ["admin", routeFamily.path, "all"], queryFn: () => directory(routeFamily) })
  const members = useQuery({ queryKey: ["admin", memberFamily.path, "all"], queryFn: () => directory(memberFamily) })
  const providers = useQuery({ queryKey: ["admin", providerFamily.path, "all"], queryFn: () => directory(providerFamily) })
  const canSettings = context.has(SETTINGS_ACCESS)
  const settings = useQuery({ queryKey: SETTINGS_KEY, queryFn: readSettings, enabled: canSettings })
  const routeContext: RouteContext = {
    providers: new Map((providers.data ?? []).map(provider => [provider.id, provider])),
    attemptCeiling: settings.data?.instance.maxAttempts ?? DEFAULT_ATTEMPTS,
  }
  const needle = search.trim().toLowerCase()
  const shown = (routes.data ?? []).filter(route => !needle || route.name.toLowerCase().includes(needle)).sort((a, b) => a.name.localeCompare(b.name))
  const membersOf = (routeId: string) => (members.data ?? []).filter(member => member.routeId === routeId)
  return <Page>
    <PageHeader title={t("nav.routes")} actions={<Button size="sm" disabled={creating} onClick={() => { setCreating(true); setOpen(null) }}><Plus data-icon="inline-start" />{t("modelRoutes.newRoute")}</Button>} />
    <Input className="max-w-xs" aria-label={t("actions.search")} placeholder={t("actions.search")} value={search} onChange={event => setSearch(event.target.value)} />
    <QueryState isPending={routes.isPending || members.isPending || providers.isPending} error={routes.error ?? members.error ?? providers.error}>
      <div className="flex flex-col gap-3">
        {creating ? <RouteCard route={null} members={[]} context={routeContext} open onOpenChange={setCreating} /> : null}
        {shown.map(route => <RouteCard key={route.id} route={route} members={membersOf(route.id)} context={routeContext} open={open === route.id} onOpenChange={next => setOpen(next ? route.id : null)} />)}
        {!shown.length && !creating ? <EmptyNotice title={t(needle ? "state.emptyTitle" : "modelRoutes.empty")} /> : null}
      </div>
    </QueryState>
  </Page>
}
