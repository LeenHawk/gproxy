import { useState } from "react"
import { useQuery } from "@tanstack/react-query"
import { useTranslation } from "react-i18next"
import { credentialDirectory } from "@/api/credentials"
import { providerPath } from "@/api/configuration"
import { useConsoleContext } from "@/capability/session"
import { ProviderCredentials } from "@/pages/credentials"
import { Page, PageHeader } from "@/components/page"
import { EmptyNotice, QueryState } from "@/components/state"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import { Badge } from "@/components/ui/badge"
import { Link } from "@/lib/router"

/** Tenant administrators use the same provider context without gateway configuration. */
export function ScopedProvidersPage({ providerId, tab }: { providerId?: string; tab: string }) {
  const { t } = useTranslation()
  const context = useConsoleContext()
  const [search, setSearch] = useState("")
  const directory = useQuery({ queryKey: ["credential-providers"], queryFn: credentialDirectory })
  const selected = directory.data?.find(provider => provider.id === providerId)
  const rows = directory.data?.filter(provider => `${provider.name} ${provider.channel}`.toLowerCase().includes(search.trim().toLowerCase())) ?? []
  return <Page><PageHeader title={selected?.name ?? t("nav.providers")} actions={<Badge variant="outline">{context.scope?.name}</Badge>} />
    {selected ? <p className="break-all text-sm text-muted-foreground">{t("providers.routePrefix")}: <code>/{selected.name}</code></p> : null}
    <QueryState isPending={directory.isPending} error={directory.error}>
      {providerId ? <><Button variant="outline" className="self-start" asChild><Link to="/providers">{t("nav.providers")}</Link></Button>{!selected ? <EmptyNotice title={t("state.notFoundTitle")} /> : tab !== "credentials" ? <EmptyNotice title={t("state.forbidden")} /> : <ProviderCredentials key={selected.id} providerId={selected.id} />}</> : <>
        <Input className="max-w-sm" aria-label={t("providers.search")} placeholder={t("providers.search")} value={search} onChange={event => setSearch(event.target.value)} />
        <div className="grid gap-3 sm:grid-cols-2">{rows.map(provider => <Link key={provider.id} to={`${providerPath(provider.id)}/credentials`} className="flex items-center justify-between gap-3 rounded-lg border p-4 text-sm hover:bg-muted"><span className="flex min-w-0 flex-col gap-1"><span className="truncate">{provider.name}</span><span className="truncate font-mono text-xs text-muted-foreground" title={t("providers.routePrefix")}>/{provider.name}</span></span><Badge variant="outline">{provider.channel}</Badge></Link>)}</div>
        {!rows.length ? <EmptyNotice title={t("state.emptyTitle")} /> : null}
      </>}
    </QueryState>
  </Page>
}
