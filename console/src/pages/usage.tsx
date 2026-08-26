import { useState } from "react"
import { useQueries } from "@tanstack/react-query"
import { useTranslation } from "react-i18next"
import type { UsageQueryDto } from "@/generated/UsageQueryDto"
import { credentialCycles, quotaWindows, usage } from "@/api/observability"
import { providers as fetchProviders } from "@/api/control"
import { userKeys as fetchUserKeys, users as fetchUsers } from "@/api/identity"
import { PageLayout } from "@/components/page-layout"
import { QueryState } from "@/components/query-state"
import { UsageExplorer } from "@/components/usage/usage-explorer"

function initialQuery(): UsageQueryDto {
  const to = Math.floor(Date.now() / 1000)
  return { from: to - 7 * 86_400, to, group_by: "provider", user_key_id: null, user_id: null, provider_id: null, model: null }
}

export function UsagePage() {
  const { t } = useTranslation()
  const [draft, setDraft] = useState<UsageQueryDto>(initialQuery)
  const [query, setQuery] = useState<UsageQueryDto>(draft)
  const [usageQuery, quotaQuery, cycleQuery, providerQuery, userQuery, keyQuery] = useQueries({ queries: [
    { queryKey: ["usage", query], queryFn: () => usage(query) },
    { queryKey: ["quota-windows"], queryFn: () => quotaWindows() },
    { queryKey: ["credential-cycles", query.from, query.to], queryFn: () => credentialCycles(query.from, query.to) },
    { queryKey: ["providers"], queryFn: fetchProviders },
    { queryKey: ["users"], queryFn: fetchUsers },
    { queryKey: ["user-keys"], queryFn: fetchUserKeys },
  ] })
  const queries = [usageQuery, quotaQuery, cycleQuery]
  return (
    <PageLayout title={t("nav.usage")} description={t("usage.description")}>
      <QueryState loading={queries.some((query) => query.isLoading)} error={queries.some((query) => query.error) ? t("common.loadError") : ""}>
        <UsageExplorer
          draft={draft}
          onDraft={setDraft}
          onApply={() => setQuery(draft)}
          onReset={() => { const next = initialQuery(); setDraft(next); setQuery(next) }}
          rows={usageQuery.data ?? []}
          quotas={quotaQuery.data ?? []}
          cycles={cycleQuery.data ?? []}
          providers={providerQuery.data ?? []}
          users={userQuery.data ?? []}
          keys={keyQuery.data ?? []}
        />
      </QueryState>
    </PageLayout>
  )
}
