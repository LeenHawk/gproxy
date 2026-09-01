import { useTranslation } from "react-i18next"
import type { OrganizationDto } from "@/generated/OrganizationDto"
import type { TeamDto } from "@/generated/TeamDto"
import type { AccessManagerProps } from "@/components/keys/access-manager"
import { ScopeAccessEditor } from "@/components/keys/access-manager"
import { DataTable, type DataTableColumn } from "@/components/data-table"
import { IdentityDetailHeader } from "@/components/identity/identity-detail-header"
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card"
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs"

export type OrganizationPanel = "profile" | "teams" | "access"

export function OrganizationDetail({ organization, teams, access, panel, onPanel, onTeam, onDeleted }: {
  organization: OrganizationDto
  teams: Array<TeamDto>
  access: AccessManagerProps
  panel: OrganizationPanel
  onPanel: (panel: OrganizationPanel) => void
  onTeam: (team: TeamDto) => void
  onDeleted: () => void
}) {
  const { t } = useTranslation()
  const scopedTeams = teams.filter((team) => team.organization_id === organization.id)
  const columns: Array<DataTableColumn<TeamDto>> = [
    { key: "name", label: t("common.name"), header: t("common.name"), cell: (team) => team.name },
    { key: "status", label: t("common.status.label"), header: t("common.status.label"), cell: (team) => t(`common.status.${team.enabled ? "enabled" : "disabled"}`) },
  ]
  return (
    <div className="flex flex-col gap-5">
      <IdentityDetailHeader title={organization.name} description={t("users.organizationSummary", { count: scopedTeams.length })} enabled={organization.enabled} entity="organizations" id={organization.id} queryKeys={["organizations", "teams", "users"]} onDeleted={onDeleted} />
      <Tabs value={panel} onValueChange={(next) => onPanel(next as OrganizationPanel)}>
        <TabsList variant="line"><TabsTrigger value="profile">{t("users.detail.profile")}</TabsTrigger><TabsTrigger value="teams">{t("users.fields.team")}</TabsTrigger><TabsTrigger value="access">{t("access.title")}</TabsTrigger></TabsList>
        <TabsContent value="profile" className="pt-5"><Card><CardHeader><CardTitle>{t("users.detail.profile")}</CardTitle></CardHeader><CardContent><dl className="grid gap-5 sm:grid-cols-2"><div><dt className="text-xs text-muted-foreground">{t("users.fields.team")}</dt><dd className="mt-1 text-sm">{scopedTeams.length}</dd></div><div><dt className="text-xs text-muted-foreground">{t("common.status.label")}</dt><dd className="mt-1 text-sm">{t(`common.status.${organization.enabled ? "enabled" : "disabled"}`)}</dd></div></dl></CardContent></Card></TabsContent>
        <TabsContent value="teams" className="pt-5"><Card><CardHeader><CardTitle>{t("users.fields.team")}</CardTitle><CardDescription>{t("users.organizationTeamsDescription", { organization: organization.name })}</CardDescription></CardHeader><CardContent><DataTable columns={columns} rows={scopedTeams} rowKey={(team) => team.id} searchText={(team) => team.name} renderCard={(team) => <div><p className="font-medium">{team.name}</p><p className="text-xs text-muted-foreground">{t(`common.status.${team.enabled ? "enabled" : "disabled"}`)}</p></div>} empty={t("common.none")} storageKey="organization-teams" onRowClick={onTeam} /></CardContent></Card></TabsContent>
        <TabsContent value="access" className="pt-5"><ScopeAccessEditor {...access} scope="organization" scopeId={organization.id} /></TabsContent>
      </Tabs>
    </div>
  )
}
