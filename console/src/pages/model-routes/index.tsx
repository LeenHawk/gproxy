import { MemberForm } from "./member-form"
import { useState } from "react"
import { useQuery } from "@tanstack/react-query"
import { useTranslation } from "react-i18next"
import { configFamily } from "@/api/config-family"
import { directory } from "@/api/models"
import { credentialDirectory } from "@/api/credentials"
import type { RouteDto, RouteWrite, RoutePatch, RouteMemberDto, RouteMemberWrite, RouteMemberPatch, ExposedModelDto, ExposedModelWrite, ExposedModelPatch } from "@/generated/sdk"
import { CollectionPage } from "@/pages/identity/collection"
import { ManagementDialog } from "@/components/management-dialog"
import { BoolCell } from "@/components/cells"
import { Button } from "@/components/ui/button"
import { QueryState } from "@/components/state"
import { Tabs, TabsList, TabsTrigger, TabsContent } from "@/components/ui/tabs"

const routes = configFamily<RouteDto, Partial<RouteWrite>, Partial<RoutePatch>>("/routes")
const members = configFamily<RouteMemberDto, Partial<RouteMemberWrite>, Partial<RouteMemberPatch>>("/route-members")
const exposed = configFamily<ExposedModelDto, Partial<ExposedModelWrite>, Partial<ExposedModelPatch>>("/exposed-models")
export function ModelRoutesPage() {
  const { t } = useTranslation()
  const [selected, setSelected] = useState<RouteDto | null>(null)
  return <><Tabs defaultValue="routes"><TabsList><TabsTrigger value="routes">{t("nav.routes")}</TabsTrigger><TabsTrigger value="exposed">{t("nav.exposed-models")}</TabsTrigger></TabsList><TabsContent value="routes"><CollectionPage id="routes" family={routes} searchable rowId={row => row.id} rowLabel={row => row.name} onOpen={setSelected}
    columns={[{ key: "name", cell: row => row.name }, { key: "strategy", cell: row => row.strategy }, { key: "maxAttempts", cell: row => row.maxAttempts }, { key: "enabled", cell: row => <BoolCell value={row.enabled} /> }]}
    fields={[{ name: "name", kind: "text", required: true }, { name: "strategy", kind: "select", choices: ["round_robin", "weighted", "failover"].map(value => ({ value, label: t(`values.${value}`) })) }, { name: "maxAttempts", kind: "number" }, { name: "enabled", kind: "switch" }]}
    rowActions={row => <Button variant="ghost" size="sm" onClick={() => setSelected(row)}>{t("management.manage")}</Button>}
  /></TabsContent><TabsContent value="exposed"><ExposedModels /></TabsContent></Tabs>
    {selected ? <RouteDetails route={selected} onClose={() => setSelected(null)} /> : null}
  </>
}
function RouteDetails({ route, onClose }: { route: RouteDto; onClose: () => void }) {
  const providers = useQuery({ queryKey: ["credential-providers"], queryFn: credentialDirectory })
  return <ManagementDialog title={route.name} onClose={onClose}><QueryState isPending={providers.isPending} error={providers.error}>
    <CollectionPage renderForm={props => props.open ? <MemberForm original={props.original} providers={providers.data ?? []} onSubmit={props.onSubmit} onClose={() => props.onOpenChange(false)} pending={props.pending} error={props.error} /> : null} embedded id="route-members" family={members} filter={{ routeId: route.id }} create={body => members.create({ ...body, routeId: route.id })} rowId={row => row.id} rowLabel={row => row.upstreamModel}
      columns={[{ key: "providerId", cell: row => providers.data?.find(provider => provider.id === row.providerId)?.name ?? row.providerId }, { key: "upstreamModel", cell: row => row.upstreamModel }, { key: "tier", cell: row => row.tier }, { key: "weight", cell: row => row.weight }, { key: "enabled", cell: row => <BoolCell value={row.enabled} /> }]}
      fields={[{ name: "providerId", kind: "select", required: true, choices: (providers.data ?? []).map(row => ({ value: row.id, label: row.name })) }, { name: "upstreamModel", kind: "text", required: true }, { name: "tier", kind: "number" }, { name: "weight", kind: "number" }, { name: "enabled", kind: "switch" }]}
    /><ExposedModels routeId={route.id} />
  </QueryState></ManagementDialog>
}
function ExposedModels({ routeId }: { routeId?: string }) {
  const list = useQuery({ queryKey: ["admin", "/routes", "directory"], queryFn: () => directory(routes) })
  return <QueryState isPending={list.isPending} error={list.error}><CollectionPage embedded={!!routeId} id="exposed-models" family={exposed} filter={routeId ? { routeId } : undefined} searchable rowId={row => row.id} rowLabel={row => row.name}
    create={body => exposed.create({ ...body, ...(routeId ? { routeId } : {}) })}
    columns={[{ key: "name", cell: row => row.name }, { key: "routeId", cell: row => list.data?.find(route => route.id === row.routeId)?.name ?? row.routeId }, { key: "enabled", cell: row => <BoolCell value={row.enabled} /> }]}
    fields={[{ name: "name", kind: "text", required: true }, ...(!routeId ? [{ name: "routeId", kind: "select" as const, required: true, choices: (list.data ?? []).map(row => ({ value: row.id, label: row.name })) }] : []), { name: "enabled", kind: "switch" }]}
  /></QueryState>
}
