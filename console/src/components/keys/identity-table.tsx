import { useMemo } from "react"
import { Building2Icon, UsersIcon, UserIcon } from "lucide-react"
import { useTranslation } from "react-i18next"
import type { OrganizationDto } from "@/generated/OrganizationDto"
import type { TeamDto } from "@/generated/TeamDto"
import type { UserDto } from "@/generated/UserDto"
import { DataTable, type DataTableColumn } from "@/components/data-table"
import { Switch } from "@/components/ui/switch"
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs"

type IdentityTableProps = {
  organizations: Array<OrganizationDto>
  teams: Array<TeamDto>
  users: Array<UserDto>
  pending: boolean
  onOrganizationToggle: (value: OrganizationDto) => void
  onTeamToggle: (value: TeamDto) => void
  onUserToggle: (value: UserDto) => void
  onOrganizationOpen?: (value: OrganizationDto) => void
  onTeamOpen?: (value: TeamDto) => void
  onUserOpen?: (value: UserDto) => void
}

function Toggle({ name, enabled, pending, onChange }: { name: string; enabled: boolean; pending: boolean; onChange: () => void }) {
  const { t } = useTranslation()
  const action = t(enabled ? "common.actions.disable" : "common.actions.enable")
  return <span onClick={(event) => event.stopPropagation()}><Switch checked={enabled} disabled={pending} aria-label={`${action} ${name}`} onCheckedChange={onChange} /></span>
}

export function IdentityTable(props: IdentityTableProps) {
  const { t } = useTranslation()
  const organizationNames = useMemo(() => new Map(props.organizations.map((value) => [value.id, value.name])), [props.organizations])
  const teamNames = useMemo(() => new Map(props.teams.map((value) => [value.id, value.name])), [props.teams])
  const organizationColumns: Array<DataTableColumn<OrganizationDto>> = [
    { key: "name", label: t("common.name"), header: t("common.name"), cell: (value) => <span className="flex items-center gap-2"><Building2Icon aria-hidden />{value.name}</span> },
    { key: "status", label: t("common.status.label"), header: t("common.status.label"), cell: (value) => <Toggle name={value.name} enabled={value.enabled} pending={props.pending} onChange={() => props.onOrganizationToggle(value)} /> },
  ]
  const teamColumns: Array<DataTableColumn<TeamDto>> = [
    { key: "name", label: t("common.name"), header: t("common.name"), cell: (value) => <span className="flex items-center gap-2"><UsersIcon aria-hidden />{value.name}</span> },
    { key: "organization", label: t("users.fields.organization"), header: t("users.fields.organization"), cell: (value) => organizationNames.get(value.organization_id) ?? value.organization_id },
    { key: "status", label: t("common.status.label"), header: t("common.status.label"), cell: (value) => <Toggle name={value.name} enabled={value.enabled} pending={props.pending} onChange={() => props.onTeamToggle(value)} /> },
  ]
  const userColumns: Array<DataTableColumn<UserDto>> = [
    { key: "name", label: t("common.name"), header: t("common.name"), cell: (value) => <span className="flex items-center gap-2"><UserIcon aria-hidden />{value.name}</span> },
    { key: "organization", label: t("users.fields.organization"), header: t("users.fields.organization"), cell: (value) => value.organization_id == null ? t("common.none") : organizationNames.get(value.organization_id) ?? value.organization_id },
    { key: "team", label: t("users.fields.team"), header: t("users.fields.team"), cell: (value) => value.team_id == null ? t("common.none") : teamNames.get(value.team_id) ?? value.team_id },
    { key: "status", label: t("common.status.label"), header: t("common.status.label"), cell: (value) => <Toggle name={value.name} enabled={value.enabled} pending={props.pending} onChange={() => props.onUserToggle(value)} /> },
  ]

  return (
    <Tabs defaultValue="users">
      <TabsList className="max-w-full overflow-x-auto">
        <TabsTrigger value="users">{t("access.subjectKinds.user")}</TabsTrigger>
        <TabsTrigger value="teams">{t("users.fields.team")}</TabsTrigger>
        <TabsTrigger value="organizations">{t("users.fields.organization")}</TabsTrigger>
      </TabsList>
      <TabsContent value="organizations" className="pt-4">
        <DataTable columns={organizationColumns} rows={props.organizations} rowKey={(value) => value.id} searchText={(value) => value.name} renderCard={(value) => <div className="flex items-center justify-between gap-3"><span className="flex items-center gap-2"><Building2Icon aria-hidden />{value.name}</span><Toggle name={value.name} enabled={value.enabled} pending={props.pending} onChange={() => props.onOrganizationToggle(value)} /></div>} empty={t("common.none")} storageKey="organizations" selectable onRowClick={props.onOrganizationOpen} />
      </TabsContent>
      <TabsContent value="teams" className="pt-4">
        <DataTable columns={teamColumns} rows={props.teams} rowKey={(value) => value.id} searchText={(value) => `${value.name} ${organizationNames.get(value.organization_id) ?? value.organization_id}`} renderCard={(value) => <div className="flex items-center justify-between gap-3"><div><p className="flex items-center gap-2"><UsersIcon aria-hidden />{value.name}</p><p className="text-xs text-muted-foreground">{organizationNames.get(value.organization_id) ?? value.organization_id}</p></div><Toggle name={value.name} enabled={value.enabled} pending={props.pending} onChange={() => props.onTeamToggle(value)} /></div>} empty={t("common.none")} storageKey="teams" selectable onRowClick={props.onTeamOpen} />
      </TabsContent>
      <TabsContent value="users" className="pt-4">
        <DataTable columns={userColumns} rows={props.users} rowKey={(value) => value.id} searchText={(value) => `${value.name} ${value.organization_id ?? ""} ${value.team_id ?? ""}`} renderCard={(value) => <div className="flex items-center justify-between gap-3"><div><p className="flex items-center gap-2"><UserIcon aria-hidden />{value.name}</p><p className="text-xs text-muted-foreground">{value.organization_id == null ? t("common.none") : organizationNames.get(value.organization_id) ?? value.organization_id} · {value.team_id == null ? t("common.none") : teamNames.get(value.team_id) ?? value.team_id}</p></div><Toggle name={value.name} enabled={value.enabled} pending={props.pending} onChange={() => props.onUserToggle(value)} /></div>} empty={t("users.empty")} storageKey="users" selectable onRowClick={props.onUserOpen} />
      </TabsContent>
    </Tabs>
  )
}
