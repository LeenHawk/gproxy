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
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card"
import { Button } from "@/components/ui/button"
import { adminPath, navigateAdminPath, useAdminLocation } from "@/lib/admin-route"
import { cn } from "@/lib/utils"

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
  const location = useAdminLocation()
  const top = location.segments[0] === "routing-aliases" ? "routing-aliases" : location.segments[0] === "model-aliases" ? "model-aliases" : "routes"
  const selectedRouteId = top === "routes" ? Number(location.segments[0]) : Number.NaN
  const selectedRoute = routes.find(({ id }) => id === selectedRouteId) ?? routes[0] ?? null
  const detailTab = location.segments[1] === "settings" ? "settings" : "members"

  return (
    <Tabs value={top} onValueChange={(value) => navigateAdminPath(value === "routes" ? adminPath("routes") : `/admin/routes/${value}`)}>
      <TabsList className="max-w-full">
        <TabsTrigger value="routes">{t("routes.title")}</TabsTrigger>
        <TabsTrigger value="routing-aliases">{t("routes.routingAliases.title")}</TabsTrigger>
        <TabsTrigger value="model-aliases">{t("routes.aliases.title")}</TabsTrigger>
      </TabsList>
      <TabsContent value="routes" className="pt-4">
        <div className="grid min-w-0 gap-5 md:grid-cols-[minmax(18rem,0.8fr)_minmax(0,1.2fr)]">
          <div className={cn(selectedRouteId && "hidden md:block")}><RouteList routes={routes} selectedId={selectedRoute?.id ?? null} onSelect={(id) => navigateAdminPath(`/admin/routes/${id}/members`)} onChanged={onRoutesChanged} /></div>
          <div className={cn("min-w-0", !selectedRouteId && "hidden md:block")}>
            {selectedRoute ? <>
              <Button className="mb-3 md:hidden" variant="ghost" onClick={() => navigateAdminPath(adminPath("routes"))}>{t("common.actions.back")}</Button>
              <Tabs value={detailTab} onValueChange={(value) => navigateAdminPath(`/admin/routes/${selectedRoute.id}/${value}`, true)}>
                <TabsList><TabsTrigger value="members">{t("routes.members.title")}</TabsTrigger><TabsTrigger value="settings">{t("routes.tabs.settings")}</TabsTrigger></TabsList>
                <TabsContent value="members" className="pt-4"><MembersPanel route={selectedRoute} members={members} providers={providers} credentials={credentials} onChanged={onMembersChanged} /></TabsContent>
                <TabsContent value="settings" className="pt-4"><Card><CardHeader><CardTitle>{selectedRoute.name}</CardTitle></CardHeader><CardContent className="grid gap-2 text-sm"><p>{t("routes.fields.maxAttempts")}: <span className="font-mono">{selectedRoute.max_attempts}</span></p><p>{t("routes.fields.enabled")}: {t(`common.status.${selectedRoute.enabled ? "enabled" : "disabled"}`)}</p></CardContent></Card></TabsContent>
              </Tabs>
            </> : <div className="grid min-h-80 place-items-center text-sm text-muted-foreground">{t("routes.selectPrompt")}</div>}
          </div>
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
