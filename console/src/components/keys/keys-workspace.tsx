import type { OrganizationDto } from "@/generated/OrganizationDto"
import type { PermissionDto } from "@/generated/PermissionDto"
import type { ProviderDto } from "@/generated/ProviderDto"
import type { QuotaDto } from "@/generated/QuotaDto"
import type { RateLimitDto } from "@/generated/RateLimitDto"
import type { TeamDto } from "@/generated/TeamDto"
import type { UserDto } from "@/generated/UserDto"
import type { UserKeyDto } from "@/generated/UserKeyDto"
import { useTranslation } from "react-i18next"
import { AccessManager, ScopeAccessEditor, type AccessScope } from "@/components/keys/access-manager"
import { KeyManagement } from "@/components/keys/key-management"
import { RecentRequestsSetting } from "@/components/portal/recent-requests-setting"
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs"
import { navigateAdminPath, useAdminLocation } from "@/lib/admin-route"

type Props = {
  organizations: Array<OrganizationDto>
  teams: Array<TeamDto>
  users: Array<UserDto>
  keys: Array<UserKeyDto>
  providers: Array<ProviderDto>
  groups: Array<string>
  permissions: Array<PermissionDto>
  rateLimits: Array<RateLimitDto>
  quotas: Array<QuotaDto>
}

export function KeysWorkspace(props: Props) {
  const identity = { organizations: props.organizations, teams: props.teams, users: props.users, keys: props.keys }
  const { t } = useTranslation()
  const location = useAdminLocation()
  const section = ["identities", "api-keys", "access", "requests"].includes(location.segments[0]) ? location.segments[0] : "identities"
  const scope = ["organization", "team", "user", "user_key"].includes(location.segments[1]) ? location.segments[1] as AccessScope : null
  const scopeId = Number(location.segments[2])
  const access = { ...identity, providers: props.providers, groups: props.groups, permissions: props.permissions, rateLimits: props.rateLimits, quotas: props.quotas }
  return (
    <Tabs value={section} onValueChange={(value) => navigateAdminPath(`/admin/keys/${value}`)}>
      <TabsList className="max-w-full"><TabsTrigger value="identities">{t("users.title")}</TabsTrigger><TabsTrigger value="api-keys">{t("users.keys.title")}</TabsTrigger><TabsTrigger value="access">{t("access.title")}</TabsTrigger><TabsTrigger value="requests">{t("portal.admin.recentRequests.title")}</TabsTrigger></TabsList>
      <TabsContent value="identities" className="pt-5"><KeyManagement {...identity} mode="identities" onScopeOpen={(kind, id) => navigateAdminPath(`/admin/keys/access/${kind}/${id}`)} /></TabsContent>
      <TabsContent value="api-keys" className="pt-5"><KeyManagement {...identity} mode="keys" /></TabsContent>
      <TabsContent value="access" className="pt-5">{scope && Number.isFinite(scopeId) ? <ScopeAccessEditor {...access} scope={scope} scopeId={scopeId} /> : <AccessManager {...access} />}</TabsContent>
      <TabsContent value="requests" className="pt-5"><RecentRequestsSetting /></TabsContent>
    </Tabs>
  )
}
