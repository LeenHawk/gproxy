import { useMemo } from "react"
import { useQueries } from "@tanstack/react-query"
import { useTranslation } from "react-i18next"
import { providers } from "@/api/control"
import {
  organizations,
  permissions,
  quotas,
  rateLimits,
  teams,
  userKeys,
  users,
} from "@/api/identity"
import { channels } from "@/api/observability"
import { KeysWorkspace } from "@/components/keys/keys-workspace"
import { PageLayout } from "@/components/page-layout"
import { QueryState } from "@/components/query-state"

export function KeysPage() {
  const { t } = useTranslation()
  const [organizationQuery, teamQuery, userQuery, keyQuery, permissionQuery, rateQuery, quotaQuery, providerQuery, channelQuery] = useQueries({
    queries: [
      { queryKey: ["organizations"], queryFn: organizations },
      { queryKey: ["teams"], queryFn: teams },
      { queryKey: ["users"], queryFn: users },
      { queryKey: ["user-keys"], queryFn: userKeys },
      { queryKey: ["permissions"], queryFn: permissions },
      { queryKey: ["rate-limits"], queryFn: rateLimits },
      { queryKey: ["quotas"], queryFn: quotas },
      { queryKey: ["providers"], queryFn: providers },
      { queryKey: ["channels"], queryFn: channels },
    ],
  })
  const queries = [organizationQuery, teamQuery, userQuery, keyQuery, permissionQuery, rateQuery, quotaQuery, providerQuery, channelQuery]
  const groups = useMemo(
    () => [...new Set((channelQuery.data ?? []).flatMap((channel) => channel.supports.map((support) => support.group)))].sort(),
    [channelQuery.data],
  )
  return (
    <PageLayout title={t("users.title")} description={t("users.subtitle")}>
      <QueryState loading={queries.some((query) => query.isLoading)} error={queries.some((query) => query.error) ? t("users.loadError") : ""}>
        <KeysWorkspace organizations={organizationQuery.data ?? []} teams={teamQuery.data ?? []} users={userQuery.data ?? []} keys={keyQuery.data ?? []} providers={providerQuery.data ?? []} groups={groups} permissions={permissionQuery.data ?? []} rateLimits={rateQuery.data ?? []} quotas={quotaQuery.data ?? []} />
      </QueryState>
    </PageLayout>
  )
}
