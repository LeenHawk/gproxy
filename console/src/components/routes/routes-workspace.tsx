import { useState } from "react"
import { useTranslation } from "react-i18next"
import type { AliasDto } from "@/generated/AliasDto"
import type { CredentialDto } from "@/generated/CredentialDto"
import type { ModelAliasDto } from "@/generated/ModelAliasDto"
import type { ProviderDto } from "@/generated/ProviderDto"
import type { RouteDto } from "@/generated/RouteDto"
import type { RouteMemberDto } from "@/generated/RouteMemberDto"
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs"
import { MembersPanel } from "@/components/routes/members-panel"
import { ModelAliases } from "@/components/routes/model-aliases"
import { RouteList } from "@/components/routes/route-list"
import { RoutingAliases } from "@/components/routes/routing-aliases"

export function RoutesWorkspace({
  routes,
  members,
  providers,
  credentials,
  routingAliases,
  modelAliases,
  onRoutesChanged,
  onMembersChanged,
  onRoutingAliasesChanged,
  onModelAliasesChanged,
}: {
  routes: Array<RouteDto>
  members: Array<RouteMemberDto>
  providers: Array<ProviderDto>
  credentials: Array<CredentialDto>
  routingAliases: Array<AliasDto>
  modelAliases: Array<ModelAliasDto>
  onRoutesChanged: () => void
  onMembersChanged: () => void
  onRoutingAliasesChanged: () => void
  onModelAliasesChanged: () => void
}) {
  const { t } = useTranslation()
  const [selectedRouteId, setSelectedRouteId] = useState<number | null>(null)
  const selectedRoute = routes.find(({ id }) => id === selectedRouteId) ?? routes[0] ?? null

  return (
    <Tabs defaultValue="routes">
      <TabsList className="max-w-full overflow-x-auto">
        <TabsTrigger value="routes">{t("routes.title")}</TabsTrigger>
        <TabsTrigger value="routing-aliases">{t("routes.routingAliases.title")}</TabsTrigger>
        <TabsTrigger value="model-aliases">{t("routes.aliases.title")}</TabsTrigger>
      </TabsList>
      <TabsContent value="routes" className="pt-4">
        <div className="grid gap-5 xl:grid-cols-[minmax(20rem,0.8fr)_minmax(28rem,1.2fr)]">
          <RouteList
            routes={routes}
            selectedId={selectedRoute?.id ?? null}
            onSelect={setSelectedRouteId}
            onChanged={onRoutesChanged}
          />
          {selectedRoute ? (
            <MembersPanel
              route={selectedRoute}
              members={members}
              providers={providers}
              credentials={credentials}
              onChanged={onMembersChanged}
            />
          ) : null}
        </div>
      </TabsContent>
      <TabsContent value="routing-aliases" className="pt-4">
        <RoutingAliases aliases={routingAliases} providers={providers} onChanged={onRoutingAliasesChanged} />
      </TabsContent>
      <TabsContent value="model-aliases" className="pt-4">
        <ModelAliases aliases={modelAliases} routes={routes} onChanged={onModelAliasesChanged} />
      </TabsContent>
    </Tabs>
  )
}
