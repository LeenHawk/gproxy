import { useState } from "react"
import { PlusIcon } from "lucide-react"
import { useTranslation } from "react-i18next"
import { saveRoute } from "@/api/control"
import type { CredentialDto } from "@/generated/CredentialDto"
import type { ModelAliasDto } from "@/generated/ModelAliasDto"
import type { ProviderDto } from "@/generated/ProviderDto"
import type { RouteDto } from "@/generated/RouteDto"
import type { RouteMemberDto } from "@/generated/RouteMemberDto"
import { BatchActions } from "@/components/batch-actions"
import { EntityDeleteButton } from "@/components/entity-delete-button"
import { EnabledSwitch } from "@/components/routes/enabled-switch"
import { MembersPanel } from "@/components/routes/members-panel"
import { ModelAliases } from "@/components/routes/model-aliases"
import { RouteEditor, RouteForm } from "@/components/routes/route-form"
import { Button } from "@/components/ui/button"
import { Card, CardAction, CardContent, CardHeader, CardTitle } from "@/components/ui/card"
import { Empty, EmptyDescription, EmptyHeader, EmptyTitle } from "@/components/ui/empty"
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs"
import { WorkspaceLayout } from "@/components/workspace/workspace-layout"
import { adminPath, navigateAdminPath, useAdminLocation } from "@/lib/admin-route"

type Props = {
  routes: Array<RouteDto>
  members: Array<RouteMemberDto>
  providers: Array<ProviderDto>
  credentials: Array<CredentialDto>
  modelAliases: Array<ModelAliasDto>
  onRoutesChanged: () => void
  onMembersChanged: () => void
  onModelAliasesChanged: () => void
}

export function RoutesWorkspace(props: Props) {
  const { t } = useTranslation()
  const location = useAdminLocation()
  const [form, setForm] = useState<{ opener: HTMLElement } | null>(null)
  const selectedId = Number(location.segments[0])
  const selected = props.routes.find((route) => route.id === selectedId) ?? null
  const detailTab = location.segments[1] === "models" || location.segments[1] === "settings"
    ? location.segments[1]
    : "members"

  return <>
    <WorkspaceLayout
      storageKey="gproxy.workspace.routes.width"
      title={t("routes.listTitle")}
      items={props.routes}
      selectedId={selected?.id ?? null}
      getSearchText={(route) => route.name}
      renderTitle={(route) => route.name}
      renderSummary={(route) => t("routes.summary", { attempts: route.max_attempts })}
      renderAction={(route) => <EnabledSwitch
        checked={route.enabled}
        label={`${route.name}: ${t("routes.fields.enabled")}`}
        errorMessage={t("routes.form.updateError")}
        onChange={(enabled) => saveRoute({ name: route.name, max_attempts: route.max_attempts, enabled }, route.id)}
        onChanged={props.onRoutesChanged}
      />}
      onSelect={(route) => navigateAdminPath(`/admin/routes/${route.id}/members`)}
      onBack={() => navigateAdminPath(adminPath("routes"))}
      searchPlaceholder={t("routes.search")}
      emptyLabel={t("routes.empty")}
      resizeLabel={t("routes.resize")}
      selectAllLabel={t("common.dataTable.selectAll")}
      selectRowLabel={(route) => `${t("common.dataTable.selectRow")}: ${route.name}`}
      selectedLabel={(count) => t("common.dataTable.selected", { count })}
      mobileBackLabel={t("common.actions.back")}
      createAction={<Button size="icon-sm" aria-label={t("routes.add")} onClick={(event) => setForm({ opener: event.currentTarget })}><PlusIcon aria-hidden /></Button>}
      batchActions={(rows, done) => <BatchActions entity="routes" rows={rows} queryKeys={["routes", "route-members", "model-aliases"]} onApplied={done} size="xs" />}
      emptyState={<Empty><EmptyHeader><EmptyTitle>{t("routes.listTitle")}</EmptyTitle><EmptyDescription>{t("routes.selectPrompt")}</EmptyDescription></EmptyHeader></Empty>}
    >
      {selected ? <Tabs value={detailTab} onValueChange={(tab) => navigateAdminPath(`/admin/routes/${selected.id}/${tab}`, true)}>
        <TabsList variant="line">
          <TabsTrigger value="members">{t("routes.members.title")}</TabsTrigger>
          <TabsTrigger value="models">{t("routes.aliases.title")}</TabsTrigger>
          <TabsTrigger value="settings">{t("routes.tabs.settings")}</TabsTrigger>
        </TabsList>
        <TabsContent value="members" className="pt-4">
          <MembersPanel route={selected} members={props.members} providers={props.providers} credentials={props.credentials} onChanged={props.onMembersChanged} />
        </TabsContent>
        <TabsContent value="models" className="pt-4">
          <ModelAliases aliases={props.modelAliases} routes={[selected]} routeId={selected.id} onChanged={props.onModelAliasesChanged} />
        </TabsContent>
        <TabsContent value="settings" className="pt-4">
          <Card>
            <CardHeader>
              <CardTitle>{selected.name}</CardTitle>
              <CardAction className="flex items-center gap-2">
                <EntityDeleteButton entity="routes" id={selected.id} label={selected.name} queryKeys={["routes", "route-members", "model-aliases"]} />
              </CardAction>
            </CardHeader>
            <CardContent><RouteEditor key={`${selected.id}-${selected.name}-${selected.max_attempts}-${selected.enabled}`} route={selected} onChanged={props.onRoutesChanged} /></CardContent>
          </Card>
        </TabsContent>
      </Tabs> : null}
    </WorkspaceLayout>
    {form ? <RouteForm opener={form.opener} onOpenChange={(open) => { if (!open) setForm(null) }} onChanged={props.onRoutesChanged} /> : null}
  </>
}
