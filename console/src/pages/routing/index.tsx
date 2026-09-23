import { useState } from "react"
import { useQuery } from "@tanstack/react-query"
import { useTranslation } from "react-i18next"
import { routes, routeMembers, exposedModels } from "@/api/routing-rules"
import { providerDirectory } from "@/api/configuration"
import type { RouteDto } from "@/generated/sdk"
import { CollectionPage } from "@/pages/identity/collection"
import { BoolCell } from "@/components/cells"
import { QueryState } from "@/components/state"
import { Button } from "@/components/ui/button"
import { Dialog, DialogBody, DialogContent, DialogHeader, DialogTitle, DialogDescription } from "@/components/ui/dialog"
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs"

export function RoutingPage() {
  const { t } = useTranslation()
  const [selected, setSelected] = useState<RouteDto | null>(null)
  return <>
    <CollectionPage id="routes" family={routes} searchable rowId={(r) => r.id} rowLabel={(r) => r.name}
      columns={[{ key: "name", cell: (r) => r.name }, { key: "strategy", cell: (r) => t(`values.${r.strategy}`) }, { key: "maxAttempts", cell: (r) => r.maxAttempts }, { key: "enabled", cell: (r) => <BoolCell value={r.enabled} /> }]}
      fields={[{ name: "name", kind: "text", required: true }, { name: "strategy", kind: "select", options: ["round_robin", "weighted", "failover"] }, { name: "maxAttempts", kind: "number" }, { name: "enabled", kind: "switch" }]}
      rowActions={(r) => <Button variant="ghost" size="sm" onClick={() => setSelected(r)}>{t("routing.configure")}</Button>}
    />
    {selected ? <RouteDetails key={selected.id} route={selected} onClose={() => setSelected(null)} /> : null}
  </>
}
function RouteDetails({ route, onClose }: { route: RouteDto; onClose: () => void }) {
  const { t } = useTranslation()
  const providers = useQuery({ queryKey: ["admin", "/providers", "directory"], queryFn: providerDirectory })
  return <Dialog open onOpenChange={(open) => { if (!open) onClose() }}><DialogContent className="sm:max-w-5xl" closeLabel={t("actions.close")}>
    <DialogHeader><DialogTitle>{t("routing.title", { name: route.name })}</DialogTitle><DialogDescription>{t("routing.help")}</DialogDescription></DialogHeader>
    <DialogBody><Tabs defaultValue="members"><TabsList><TabsTrigger value="members">{t("nav.route-members")}</TabsTrigger><TabsTrigger value="names">{t("nav.exposed-models")}</TabsTrigger></TabsList>
      <TabsContent value="members"><QueryState isPending={providers.isPending} error={providers.error}><CollectionPage embedded id="route-members" family={routeMembers} filter={{ routeId: route.id }} create={(body) => routeMembers.create({ ...body, routeId: route.id })} rowId={(r) => r.id} rowLabel={(r) => r.upstreamModel}
        columns={[{ key: "providerId", cell: (r) => providers.data?.find((p) => p.id === r.providerId)?.name ?? r.providerId }, { key: "upstreamModel", cell: (r) => r.upstreamModel }, { key: "tier", cell: (r) => r.tier }, { key: "weight", cell: (r) => r.weight }, { key: "enabled", cell: (r) => <BoolCell value={r.enabled} /> }]}
        fields={[{ name: "providerId", kind: "select", required: true, choices: providers.data?.map((p) => ({ value: p.id, label: p.name })) ?? [] }, { name: "upstreamModel", kind: "text", required: true }, { name: "tier", kind: "number" }, { name: "weight", kind: "number" }, { name: "enabled", kind: "switch" }]}
      /></QueryState></TabsContent>
      <TabsContent value="names"><CollectionPage embedded id="exposed-models" family={exposedModels} filter={{ routeId: route.id }} create={(body) => exposedModels.create({ ...body, routeId: route.id })} rowId={(r) => r.id} rowLabel={(r) => r.name} columns={[{ key: "name", cell: (r) => r.name }, { key: "enabled", cell: (r) => <BoolCell value={r.enabled} /> }]} fields={[{ name: "name", kind: "text", required: true }, { name: "enabled", kind: "switch" }]} /></TabsContent>
    </Tabs></DialogBody>
  </DialogContent></Dialog>
}
