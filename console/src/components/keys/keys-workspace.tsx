import type { OrganizationDto } from "@/generated/OrganizationDto"
import type { PermissionDto } from "@/generated/PermissionDto"
import type { ProviderDto } from "@/generated/ProviderDto"
import type { QuotaDto } from "@/generated/QuotaDto"
import type { RateLimitDto } from "@/generated/RateLimitDto"
import type { TeamDto } from "@/generated/TeamDto"
import type { UserDto } from "@/generated/UserDto"
import type { UserKeyDto } from "@/generated/UserKeyDto"
import { AccessManager } from "@/components/keys/access-manager"
import { KeyManagement } from "@/components/keys/key-management"
import { RecentRequestsSetting } from "@/components/portal/recent-requests-setting"

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
  return (
    <div className="flex flex-col gap-6">
      <KeyManagement {...identity} />
      <AccessManager {...identity} providers={props.providers} groups={props.groups} permissions={props.permissions} rateLimits={props.rateLimits} quotas={props.quotas} />
      <RecentRequestsSetting />
    </div>
  )
}
