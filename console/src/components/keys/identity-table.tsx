import { useMemo } from "react"
import { useTranslation } from "react-i18next"
import type { OrganizationDto } from "@/generated/OrganizationDto"
import type { TeamDto } from "@/generated/TeamDto"
import type { UserDto } from "@/generated/UserDto"
import { Switch } from "@/components/ui/switch"
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from "@/components/ui/table"
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs"

type IdentityTableProps = {
  organizations: Array<OrganizationDto>
  teams: Array<TeamDto>
  users: Array<UserDto>
  pending: boolean
  onOrganizationToggle: (value: OrganizationDto) => void
  onTeamToggle: (value: TeamDto) => void
  onUserToggle: (value: UserDto) => void
}

function Toggle({ name, enabled, pending, onChange }: { name: string; enabled: boolean; pending: boolean; onChange: () => void }) {
  const { t } = useTranslation()
  const action = t(enabled ? "common.actions.disable" : "common.actions.enable")
  return <Switch checked={enabled} disabled={pending} aria-label={`${action} ${name}`} onCheckedChange={onChange} />
}

export function IdentityTable(props: IdentityTableProps) {
  const { t } = useTranslation()
  const organizationNames = useMemo(() => new Map(props.organizations.map((value) => [value.id, value.name])), [props.organizations])
  const teamNames = useMemo(() => new Map(props.teams.map((value) => [value.id, value.name])), [props.teams])
  const statusHead = <TableHead>{t("common.status.label")}</TableHead>

  return (
    <Tabs defaultValue="users">
      <TabsList className="max-w-full overflow-x-auto">
        <TabsTrigger value="users">{t("access.subjectKinds.user")}</TabsTrigger>
        <TabsTrigger value="teams">{t("users.fields.team")}</TabsTrigger>
        <TabsTrigger value="organizations">{t("users.fields.organization")}</TabsTrigger>
      </TabsList>
      <TabsContent value="organizations" className="pt-4">
        <Table><TableHeader><TableRow><TableHead>{t("common.name")}</TableHead>{statusHead}</TableRow></TableHeader>
          <TableBody>{props.organizations.length === 0 ? <TableRow><TableCell colSpan={2}>{t("common.none")}</TableCell></TableRow> : props.organizations.map((value) => <TableRow key={value.id}><TableCell>{value.name}</TableCell><TableCell><Toggle name={value.name} enabled={value.enabled} pending={props.pending} onChange={() => props.onOrganizationToggle(value)} /></TableCell></TableRow>)}</TableBody>
        </Table>
      </TabsContent>
      <TabsContent value="teams" className="pt-4">
        <Table><TableHeader><TableRow><TableHead>{t("common.name")}</TableHead><TableHead>{t("users.fields.organization")}</TableHead>{statusHead}</TableRow></TableHeader>
          <TableBody>{props.teams.length === 0 ? <TableRow><TableCell colSpan={3}>{t("common.none")}</TableCell></TableRow> : props.teams.map((value) => <TableRow key={value.id}><TableCell>{value.name}</TableCell><TableCell>{organizationNames.get(value.organization_id) ?? value.organization_id}</TableCell><TableCell><Toggle name={value.name} enabled={value.enabled} pending={props.pending} onChange={() => props.onTeamToggle(value)} /></TableCell></TableRow>)}</TableBody>
        </Table>
      </TabsContent>
      <TabsContent value="users" className="pt-4">
        <Table><TableHeader><TableRow><TableHead>{t("common.name")}</TableHead><TableHead>{t("users.fields.organization")}</TableHead><TableHead>{t("users.fields.team")}</TableHead>{statusHead}</TableRow></TableHeader>
          <TableBody>{props.users.length === 0 ? <TableRow><TableCell colSpan={4}>{t("users.empty")}</TableCell></TableRow> : props.users.map((value) => <TableRow key={value.id}><TableCell>{value.name}</TableCell><TableCell>{value.organization_id == null ? t("common.none") : organizationNames.get(value.organization_id) ?? value.organization_id}</TableCell><TableCell>{value.team_id == null ? t("common.none") : teamNames.get(value.team_id) ?? value.team_id}</TableCell><TableCell><Toggle name={value.name} enabled={value.enabled} pending={props.pending} onChange={() => props.onUserToggle(value)} /></TableCell></TableRow>)}</TableBody>
        </Table>
      </TabsContent>
    </Tabs>
  )
}
