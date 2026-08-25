import { useState } from "react"
import { useQueries } from "@tanstack/react-query"
import { useTranslation } from "react-i18next"
import type { UsageGroupByDto } from "@/generated/UsageGroupByDto"
import { credentialCycles, quotaWindows, usage } from "@/api/observability"
import { providers as fetchProviders } from "@/api/control"
import { userKeys as fetchUserKeys, users as fetchUsers } from "@/api/identity"
import { PageLayout } from "@/components/page-layout"
import { QueryState } from "@/components/query-state"
import { UsageExplorer } from "@/components/usage/usage-explorer"
import { useNow } from "@/lib/use-now"

export function UsagePage() {
  const { t } = useTranslation()
  const [group, setGroup] = useState<UsageGroupByDto>("provider")
  const [rangeDays, setRangeDays] = useState(7)
  const to = useNow()
  const from = to - rangeDays * 86_400
  const [usageQuery, quotaQuery, cycleQuery, providerQuery, userQuery, keyQuery] = useQueries({ queries: [
    { queryKey: ["usage", group, from, to], queryFn: () => usage({ from, to, group_by: group, user_key_id: null, user_id: null, provider_id: null, model: null }) },
    { queryKey: ["quota-windows"], queryFn: () => quotaWindows() },
    { queryKey: ["credential-cycles", from, to], queryFn: () => credentialCycles(from, to) },
    { queryKey: ["providers"], queryFn: fetchProviders },
    { queryKey: ["users"], queryFn: fetchUsers },
    { queryKey: ["user-keys"], queryFn: fetchUserKeys },
  ] })
  const queries = [usageQuery, quotaQuery, cycleQuery]
  return (
    <PageLayout title={t("nav.usage")} description={t("usage.description")}>
      <QueryState loading={queries.some((query) => query.isLoading)} error={queries.some((query) => query.error) ? t("common.loadError") : ""}>
        <UsageExplorer group={group} onGroup={setGroup} rangeDays={rangeDays} onRangeDays={setRangeDays} rows={usageQuery.data ?? []} quotas={quotaQuery.data ?? []} cycles={cycleQuery.data ?? []} providers={providerQuery.data ?? []} users={userQuery.data ?? []} keys={keyQuery.data ?? []} />
      </QueryState>
    </PageLayout>
  )
}
