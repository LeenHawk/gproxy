import { useTranslation } from "react-i18next"
import type { OrganizationDto } from "@/generated/OrganizationDto"
import type { TeamDto } from "@/generated/TeamDto"
import type { UserDto } from "@/generated/UserDto"
import type { UserWriteRequest } from "@/generated/UserWriteRequest"
import type { AccessManagerProps } from "@/components/keys/access-manager"
import { ScopeAccessEditor } from "@/components/keys/access-manager"
import { IdentityDetailHeader } from "@/components/identity/identity-detail-header"
import { UserKeyPanel } from "@/components/identity/user-key-panel"
import { UserProfileForm } from "@/components/identity/user-profile-form"
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card"
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs"

export type UserPanel = "profile" | "keys" | "access"

export function UserDetail({ user, organizations, teams, access, panel, pending, onPanel, onSave, onDeleted }: {
  user: UserDto
  organizations: Array<OrganizationDto>
  teams: Array<TeamDto>
  access: AccessManagerProps
  panel: UserPanel
  pending: boolean
  onPanel: (panel: UserPanel) => void
  onSave: (value: UserWriteRequest, password: string) => Promise<void>
  onDeleted: () => void
}) {
  const { t } = useTranslation()
  const organization = organizations.find((value) => value.id === user.organization_id)
  const team = teams.find((value) => value.id === user.team_id)
  return (
    <div className="flex flex-col gap-5">
      <IdentityDetailHeader title={user.name} description={[organization?.name, team?.name].filter(Boolean).join(" · ") || t("common.none")} enabled={user.enabled} entity="users" id={user.id} queryKeys={["users", "user-keys"]} onDeleted={onDeleted} />
      <Tabs value={panel} onValueChange={(next) => onPanel(next as UserPanel)}>
        <TabsList variant="line"><TabsTrigger value="profile">{t("users.detail.profile")}</TabsTrigger><TabsTrigger value="keys">{t("users.keys.title")}</TabsTrigger><TabsTrigger value="access">{t("access.title")}</TabsTrigger></TabsList>
        <TabsContent value="profile" className="pt-5"><Card><CardHeader><CardTitle>{t("users.detail.profile")}</CardTitle></CardHeader><CardContent><UserProfileForm key={`${user.id}-${user.name}-${user.organization_id}-${user.team_id}-${user.enabled}-${user.is_admin}`} user={user} organizations={organizations} teams={teams} pending={pending} onSave={onSave} /></CardContent></Card></TabsContent>
        <TabsContent value="keys" className="pt-5"><UserKeyPanel user={user} access={access} /></TabsContent>
        <TabsContent value="access" className="pt-5"><ScopeAccessEditor {...access} scope="user" scopeId={user.id} /></TabsContent>
      </Tabs>
    </div>
  )
}
