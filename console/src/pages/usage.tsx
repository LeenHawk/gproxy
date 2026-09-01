import { useState } from "react"
import { useQueries } from "@tanstack/react-query"
import { useTranslation } from "react-i18next"
import type { UsageQueryDto } from "@/generated/UsageQueryDto"
import { credentialCycles, usage } from "@/api/observability"
import { credentials as fetchCredentials, providers as fetchProviders } from "@/api/control"
import { userKeys as fetchUserKeys, users as fetchUsers } from "@/api/identity"
import { PageLayout } from "@/components/page-layout"
import { QueryState } from "@/components/query-state"
import { UsageExplorer } from "@/components/usage/usage-explorer"
import { ObservabilityTabs } from "@/components/observability-tabs"

function initialQuery(): UsageQueryDto {
  const to = Math.floor(Date.now() / 1000)
  return { from: to - 7 * 86_400, to, group_by: null, user_key_id: null, user_id: null, provider_id: null, credential_id: null, model: null }
}

export function UsagePage() {
  const { t } = useTranslation()
  const [draft, setDraft] = useState<UsageQueryDto>(initialQuery)
  const [query, setQuery] = useState<UsageQueryDto>(draft)
  const [usageQuery, cycleQuery, credentialQuery, providerQuery, userQuery, keyQuery] = useQueries({ queries: [
    { queryKey: ["usage", query], queryFn: () => usage(query) },
    { queryKey: ["credential-cycles", query.from, query.to, query.credential_id], queryFn: () => credentialCycles(query.from, query.to, query.credential_id ?? undefined) },
    { queryKey: ["credentials"], queryFn: fetchCredentials },
    { queryKey: ["providers"], queryFn: fetchProviders },
    { queryKey: ["users"], queryFn: fetchUsers },
    { queryKey: ["user-keys"], queryFn: fetchUserKeys },
  ] })
  const queries = [usageQuery, cycleQuery]
  return (
    <PageLayout title={t("nav.usage")} description={t("usage.description")}>
      <ObservabilityTabs value="usage" />
      <QueryState loading={queries.some((query) => query.isLoading)} error={queries.some((query) => query.error) ? t("common.loadError") : ""}>
        <UsageExplorer
          draft={draft}
          onDraft={setDraft}
          onApply={() => setQuery(draft)}
          onReset={() => { const next = initialQuery(); setDraft(next); setQuery(next) }}
          rows={usageQuery.data ?? []}
          cycles={cycleQuery.data ?? []}
          credentials={credentialQuery.data ?? []}
          providers={providerQuery.data ?? []}
          users={userQuery.data ?? []}
          keys={keyQuery.data ?? []}
        />
      </QueryState>
    </PageLayout>
  )
}
