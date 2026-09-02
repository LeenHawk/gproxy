import { useTranslation } from "react-i18next"
import type { OrganizationDto } from "@/generated/OrganizationDto"
import type { TeamDto } from "@/generated/TeamDto"
import type { TeamWriteRequest } from "@/generated/TeamWriteRequest"
import type { AccessManagerProps } from "@/components/keys/access-manager"
import { ScopeAccessEditor } from "@/components/keys/access-manager"
import { IdentityDetailHeader } from "@/components/identity/identity-detail-header"
import { TeamProfileForm } from "@/components/identity/team-profile-form"
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card"
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs"

export type TeamPanel = "profile" | "access"

export function TeamDetail({ team, organizations, access, panel, pending, onPanel, onSave, onDeleted }: {
  team: TeamDto
  organizations: Array<OrganizationDto>
  access: AccessManagerProps
  panel: TeamPanel
  pending: boolean
  onPanel: (panel: TeamPanel) => void
  onSave: (value: TeamWriteRequest) => Promise<void>
  onDeleted: () => void
}) {
  const { t } = useTranslation()
  const organization = organizations.find((value) => value.id === team.organization_id)
  return (
    <div className="flex flex-col gap-5">
      <IdentityDetailHeader title={team.name} description={organization?.name ?? t("common.none")} enabled={team.enabled} entity="teams" id={team.id} queryKeys={["teams", "users"]} onDeleted={onDeleted} />
      <Tabs value={panel} onValueChange={(next) => onPanel(next as TeamPanel)}>
        <TabsList variant="line"><TabsTrigger value="profile">{t("users.detail.profile")}</TabsTrigger><TabsTrigger value="access">{t("access.title")}</TabsTrigger></TabsList>
        <TabsContent value="profile" className="pt-5"><Card><CardHeader><CardTitle>{t("users.detail.profile")}</CardTitle></CardHeader><CardContent><TeamProfileForm key={`${team.id}-${team.name}-${team.organization_id}-${team.enabled}`} team={team} organizations={organizations} pending={pending} onSave={onSave} /></CardContent></Card></TabsContent>
        <TabsContent value="access" className="pt-5"><ScopeAccessEditor {...access} scope="team" scopeId={team.id} /></TabsContent>
      </Tabs>
    </div>
  )
}
