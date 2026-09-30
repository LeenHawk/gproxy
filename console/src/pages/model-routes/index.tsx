import { MemberForm } from "./member-form"
import { useState } from "react"
import { useQuery } from "@tanstack/react-query"
import { useTranslation } from "react-i18next"
import { configFamily } from "@/api/config-family"
import { providers as providerFamily } from "@/api/configuration"
import type { RouteDto, RouteWrite, RoutePatch, RouteMemberDto, RouteMemberWrite, RouteMemberPatch } from "@/generated/sdk"
import { CollectionPage } from "@/pages/identity/collection"
import { ManagementDialog } from "@/components/management-dialog"
import { BoolCell } from "@/components/cells"
import { Button } from "@/components/ui/button"

const routes = configFamily<RouteDto, Partial<RouteWrite>, Partial<RoutePatch>>("/routes")
const members = configFamily<RouteMemberDto, Partial<RouteMemberWrite>, Partial<RouteMemberPatch>>("/route-members")
export function ModelRoutesPage() {
  const { t } = useTranslation()
  const [selected, setSelected] = useState<RouteDto | null>(null)
  return <><CollectionPage id="routes" family={routes} searchable rowId={row => row.id} rowLabel={row => row.name} onOpen={setSelected}
    columns={[{ key: "name", header: t("modelRoutes.modelName"), cell: row => row.name }, { key: "strategy", cell: row => row.strategy }, { key: "sessionAffinity", cell: row => <BoolCell value={row.sessionAffinity} /> }, { key: "maxAttempts", cell: row => row.maxAttempts }, { key: "enabled", cell: row => <BoolCell value={row.enabled} /> }]}
    fields={[{ name: "name", label: t("modelRoutes.modelName"), kind: "text", required: true }, { name: "strategy", kind: "select", choices: ["round_robin", "weighted", "failover"].map(value => ({ value, label: t(`values.${value}`) })) }, { name: "sessionAffinity", kind: "switch", defaultChecked: false }, { name: "maxAttempts", kind: "number" }, { name: "enabled", kind: "switch" }]}
    rowActions={row => <Button variant="ghost" size="sm" onClick={() => setSelected(row)}>{t("management.manage")}</Button>}
  />
    {selected ? <RouteDetails route={selected} onClose={() => setSelected(null)} /> : null}
  </>
}
function RouteDetails({ route, onClose }: { route: RouteDto; onClose: () => void }) {
  return <ManagementDialog title={route.name} onClose={onClose}>
    <CollectionPage renderForm={props => props.open ? <MemberForm original={props.original} onSubmit={props.onSubmit} onClose={() => props.onOpenChange(false)} pending={props.pending} error={props.error} /> : null} embedded id="route-members" family={members} filter={{ routeId: route.id }} create={body => members.create({ ...body, routeId: route.id })} rowId={row => row.id} rowLabel={row => row.upstreamModel}
      columns={[{ key: "providerId", cell: row => <ProviderName id={row.providerId} /> }, { key: "upstreamModel", cell: row => row.upstreamModel }, { key: "tier", cell: row => row.tier }, { key: "weight", cell: row => row.weight }, { key: "enabled", cell: row => <BoolCell value={row.enabled} /> }]}
      fields={[{ name: "providerId", kind: "select", required: true }, { name: "upstreamModel", kind: "text", required: true }, { name: "tier", kind: "number" }, { name: "weight", kind: "number" }, { name: "enabled", kind: "switch" }]}
    />
  </ManagementDialog>
}

function ProviderName({ id }: { id: string }) {
  const provider = useQuery({ queryKey: ["admin", "/providers", "detail", id], queryFn: () => providerFamily.get(id) })
  return provider.data?.name ?? id
}
