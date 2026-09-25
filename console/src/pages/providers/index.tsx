import { invalidateConfiguration } from "@/api/invalidation"
import { useConsoleContext } from "@/capability/session"
import { PROVIDERS_READ } from "@/api/configuration"
import { ScopedProvidersPage } from "./scoped"
import { useConfigBatch } from "@/components/config-batch"
import { useState } from "react"
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query"
import { useTranslation } from "react-i18next"
import { ArrowLeft, Plus, Search } from "lucide-react"
import { toast } from "sonner"
import { channels, providers, providerPath } from "@/api/configuration"
import type { ProviderWrite } from "@/generated/sdk"
import { Pagination } from "@/components/data-table"
import { EmptyNotice, QueryState } from "@/components/state"
import { Button } from "@/components/ui/button"
import { InputGroup, InputGroupAddon, InputGroupInput } from "@/components/ui/input-group"
import { Link, useNavigate } from "@/lib/router"
import { usePagination } from "@/lib/use-pagination"
import { cn } from "@/lib/utils"
import { ProviderDialog } from "@/pages/providers/provider-form"
import { ProviderDetailPage } from "@/pages/providers/detail"

export function ProvidersPage({ providerId, tab = "credentials" }: { providerId?: string; tab?: string }) {
  const context = useConsoleContext()
  return context.has(PROVIDERS_READ) ? <InstanceProvidersPage providerId={providerId} tab={tab} /> : <ScopedProvidersPage providerId={providerId} tab={tab} />
}

function InstanceProvidersPage({ providerId, tab }: { providerId?: string; tab: string }) {
  const { t } = useTranslation(), navigate = useNavigate(), client = useQueryClient()
  const [search, setSearch] = useState("")
  const { page, pageSize, setPage, setPageSize } = usePagination(search)
  const [creating, setCreating] = useState(false)
  const request = { page, pageSize, search: search.trim() || undefined }
  const list = useQuery({ queryKey: ["admin", "/providers", request], queryFn: () => providers.list(request) })
  const catalog = useQuery({ queryKey: ["configuration", "channels"], queryFn: channels })
  if (list.data && page > Math.max(1, Math.ceil(list.data.total / pageSize))) setPage(Math.max(1, Math.ceil(list.data.total / pageSize)))
  const create = useMutation({ mutationFn: (body: Record<string, unknown>) => providers.create(body as Partial<ProviderWrite>), onSuccess: async row => {
    setCreating(false)
    setSearch("")
    setPage(1)
    await invalidateConfiguration(client, "/providers")
    navigate(providerPath(row.id))
    toast.success(t("toast.created"))
  } })
  const batch = useConfigBatch({ family: providers, context: JSON.stringify(request), rows: list.data?.items ?? [], onSaved: async () => { navigate("/providers") } })
  return <>
    <div className="min-h-[calc(100dvh-9rem)] overflow-hidden rounded-xl border bg-background lg:grid lg:grid-cols-[16rem_minmax(0,1fr)]">
      <section aria-label={t("nav.providers")} className={cn("min-w-0 flex-col lg:flex lg:max-h-[calc(100dvh-9rem)] lg:border-r", providerId ? "hidden" : "flex")}>
        <div className="flex flex-col gap-3 border-b p-3">
          <div className="flex items-center justify-between gap-2"><h1 className="text-lg font-medium">{t("nav.providers")}</h1><div className="flex items-center gap-2">{batch.trigger}<Button size="icon-sm" aria-label={t("create.providers")} title={t("create.providers")} onClick={() => { create.reset(); setCreating(true) }}><Plus /></Button></div></div>
          <InputGroup><InputGroupAddon><Search /></InputGroupAddon><InputGroupInput value={search} onChange={event => setSearch(event.target.value)} aria-label={t("providers.search")} placeholder={t("providers.search")} /></InputGroup>
        </div>
        {batch.active ? <div className="p-3">{batch.toolbar}</div> : null}
        <div className="min-h-0 flex-1 overflow-y-auto p-2">
          <QueryState isPending={list.isPending} error={list.error}>
            {list.data?.items.length ? <ul className="flex flex-col gap-1">{list.data.items.map(row => <li key={row.id} className="flex items-center gap-1">{batch.checkbox(row.id, row.displayName ?? row.name)}<Link to={`${providerPath(row.id)}/${tab}`} aria-current={row.id === providerId ? "page" : undefined} className={cn("flex min-h-11 min-w-0 flex-1 items-center justify-between gap-3 rounded-lg px-3 py-2 text-sm outline-none focus-visible:ring-2 focus-visible:ring-ring", row.id === providerId ? "bg-accent font-medium text-accent-foreground" : "hover:bg-muted")}><span className="flex min-w-0 flex-col gap-1"><span className="truncate" title={row.displayName ?? row.name}>{row.displayName ?? row.name}</span><span className="truncate font-mono text-xs text-muted-foreground" title={t("providers.routePrefix")}>/{row.name}</span></span>{!row.enabled ? <span className="shrink-0">{t("values.disabled")}</span> : null}</Link></li>)}</ul> : <EmptyNotice title={t("state.emptyTitle")} />}
          </QueryState>
        </div>
        <div className="border-t p-3"><Pagination page={page} pageSize={pageSize} total={list.data?.total ?? 0} onPage={setPage} onPageSize={setPageSize} /></div>
      </section>
      <section className={cn("min-w-0 p-4", providerId ? "block" : "hidden lg:block")}>
        {providerId ? <><Button variant="ghost" className="mb-3 lg:hidden" onClick={() => navigate("/providers")}><ArrowLeft data-icon="inline-start" />{t("nav.providers")}</Button><ProviderDetailPage key={providerId} providerId={providerId} tab={tab} /></> : <EmptyNotice title={t("providers.select")} />}
      </section>
    </div>
    <QueryState isPending={creating && catalog.isPending} error={creating ? catalog.error : null}><ProviderDialog open={creating} onOpenChange={setCreating} catalog={catalog.data ?? []} onSubmit={body => create.mutate(body)} pending={create.isPending} error={create.error} /></QueryState>
  </>
}
