import { useState } from "react"
import { useMutation, useQueryClient } from "@tanstack/react-query"
import { useTranslation } from "react-i18next"
import { toast } from "sonner"
import { saveOrganization, saveTeam, saveUser, saveUserPassword } from "@/api/identity"
import type { OrganizationDto } from "@/generated/OrganizationDto"
import type { TeamDto } from "@/generated/TeamDto"
import type { UserDto } from "@/generated/UserDto"
import type { AccessManagerProps } from "@/components/keys/access-manager"
import { IdentityCreateDialog } from "@/components/identity/identity-create-dialog"
import { OrganizationWorkspace } from "@/components/identity/organization-workspace"
import { TeamWorkspace } from "@/components/identity/team-workspace"
import { UserWorkspace } from "@/components/identity/user-workspace"
import type { IdentityKind } from "@/components/keys/identity-forms"
import type { OrganizationPanel } from "@/components/identity/organization-detail"
import type { TeamPanel } from "@/components/identity/team-detail"
import type { UserPanel } from "@/components/identity/user-detail"
import { Tabs, TabsList, TabsTrigger } from "@/components/ui/tabs"
import { navigateAdminPath, useAdminLocation } from "@/lib/admin-route"

type Entity = "users" | "teams" | "organizations"

export function IdentityWorkspace(access: AccessManagerProps) {
  const { t } = useTranslation()
  const client = useQueryClient()
  const location = useAdminLocation()
  const entity = (["users", "teams", "organizations"] as const).includes(location.segments[0] as Entity) ? location.segments[0] as Entity : "users"
  const selectedId = Number(location.segments[1])
  const panel = location.segments[2]
  const [createKind, setCreateKind] = useState<IdentityKind | null>(null)
  const refresh = (...keys: Array<string>) => Promise.all(keys.map((key) => client.invalidateQueries({ queryKey: [key] })))
  const organizationMutation = useMutation({ mutationFn: ({ value, id }: { value: Parameters<typeof saveOrganization>[0]; id?: number }) => saveOrganization(value, id), onSuccess: async () => { toast.success(t("common.actions.saved")); await refresh("organizations") }, onError: () => toast.error(t("common.errors.save")) })
  const teamMutation = useMutation({ mutationFn: ({ value, id }: { value: Parameters<typeof saveTeam>[0]; id?: number }) => saveTeam(value, id), onSuccess: async () => { toast.success(t("common.actions.saved")); await refresh("teams") }, onError: () => toast.error(t("common.errors.save")) })
  const userMutation = useMutation({ mutationFn: ({ value, id }: { value: Parameters<typeof saveUser>[0]; id?: number }) => saveUser(value, id), onSuccess: async (_, input) => { toast.success(t(input.id == null ? "users.created" : "users.updated")); await refresh("users") }, onError: () => toast.error(t("users.saveError")) })
  const userProfileMutation = useMutation({ mutationFn: async ({ value, id, password }: { value: Parameters<typeof saveUser>[0]; id: number; password: string }) => { await saveUser(value, id); if (password) await saveUserPassword(id, { password }) }, onSuccess: async () => { toast.success(t("users.updated")); await refresh("users") }, onError: () => toast.error(t("users.saveError")) })
  const pending = organizationMutation.isPending || teamMutation.isPending || userMutation.isPending || userProfileMutation.isPending
  const go = (nextEntity: Entity, id?: number, nextPanel?: string) => navigateAdminPath(`/admin/identity/${nextEntity}${id == null ? "" : `/${id}/${nextPanel ?? "profile"}`}`)
  const common = { access, selectedId: Number.isFinite(selectedId) ? selectedId : null, pending, onCreate: () => setCreateKind(entity === "users" ? "user" : entity === "teams" ? "team" : "organization") }
  return (
    <div className="flex flex-col gap-5">
      <Tabs value={entity} onValueChange={(value) => go(value as Entity)}><TabsList><TabsTrigger value="users">{t("access.subjectKinds.user")}</TabsTrigger><TabsTrigger value="teams">{t("users.fields.team")}</TabsTrigger><TabsTrigger value="organizations">{t("users.fields.organization")}</TabsTrigger></TabsList></Tabs>
      {entity === "users" ? <UserWorkspace {...common} panel={(["profile", "keys", "access"].includes(panel) ? panel : "profile") as UserPanel} onSelect={(user) => go("users", user.id)} onBack={() => go("users")} onPanel={(next) => go("users", selectedId, next)} onToggle={(user: UserDto) => userMutation.mutate({ id: user.id, value: { organization_id: user.organization_id, team_id: user.team_id, name: user.name, enabled: !user.enabled, is_admin: user.is_admin, password: null } })} onSave={(user, value, password) => userProfileMutation.mutateAsync({ id: user.id, value, password })} /> : null}
      {entity === "teams" ? <TeamWorkspace {...common} panel={(["profile", "access"].includes(panel) ? panel : "profile") as TeamPanel} onSelect={(team) => go("teams", team.id)} onBack={() => go("teams")} onPanel={(next) => go("teams", selectedId, next)} onToggle={(team: TeamDto) => teamMutation.mutate({ id: team.id, value: { organization_id: team.organization_id, name: team.name, enabled: !team.enabled } })} onSave={async (team, value) => { await teamMutation.mutateAsync({ id: team.id, value }) }} /> : null}
      {entity === "organizations" ? <OrganizationWorkspace {...common} panel={(["profile", "teams", "access"].includes(panel) ? panel : "profile") as OrganizationPanel} onSelect={(organization) => go("organizations", organization.id)} onBack={() => go("organizations")} onPanel={(next) => go("organizations", selectedId, next)} onTeam={(team) => go("teams", team.id)} onToggle={(organization: OrganizationDto) => organizationMutation.mutate({ id: organization.id, value: { name: organization.name, enabled: !organization.enabled } })} onSave={async (organization, value) => { await organizationMutation.mutateAsync({ id: organization.id, value }) }} /> : null}
      {createKind ? <IdentityCreateDialog open onOpenChange={(open) => { if (!open) setCreateKind(null) }} kind={createKind} organizations={access.organizations} teams={access.teams} pending={pending} onOrganization={(name) => organizationMutation.mutateAsync({ value: { name, enabled: true } }).then(() => { setCreateKind(null) })} onTeam={(organization_id, name) => teamMutation.mutateAsync({ value: { organization_id, name, enabled: true } }).then(() => { setCreateKind(null) })} onUser={(organization_id, team_id, name, password) => userMutation.mutateAsync({ value: { organization_id, team_id, name, enabled: true, is_admin: false, password } }).then(() => { setCreateKind(null) })} /> : null}
    </div>
  )
}
