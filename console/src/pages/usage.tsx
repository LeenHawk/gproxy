import { useState } from "react"
import { keepPreviousData, useQueries } from "@tanstack/react-query"
import { useTranslation } from "react-i18next"
import type { UsageRecordQueryDto } from "@/generated/UsageRecordQueryDto"
import { usageRecords, usageSummary } from "@/api/observability"
import { credentials as fetchCredentials, providers as fetchProviders } from "@/api/control"
import { userKeys as fetchUserKeys, users as fetchUsers } from "@/api/identity"
import { PageLayout } from "@/components/page-layout"
import { QueryState } from "@/components/query-state"
import { UsageExplorer } from "@/components/usage/usage-explorer"
import { ObservabilityTabs } from "@/components/observability-tabs"

function initialQuery(): UsageRecordQueryDto {
  const to = Math.floor(Date.now() / 1000)
  return { from: to - 7 * 86_400, to, user_key_id: null, user_id: null, provider_id: null, credential_id: null, model: null, request_id: null, operation: null, usage_source: null, ended: null, page: 1, page_size: 10 }
}

export function UsagePage() {
  const { t } = useTranslation()
  const [draft, setDraft] = useState<UsageRecordQueryDto>(initialQuery)
  const [query, setQuery] = useState<UsageRecordQueryDto>(draft)
  const filter = { ...query, page: null, page_size: null }
  const [records, summary, credentialQuery, providerQuery, userQuery, keyQuery] = useQueries({ queries: [
    { queryKey: ["usage-records", query], queryFn: () => usageRecords(query), placeholderData: keepPreviousData },
    { queryKey: ["usage-summary", filter], queryFn: () => usageSummary(query) },
    { queryKey: ["credentials"], queryFn: fetchCredentials },
    { queryKey: ["providers"], queryFn: fetchProviders },
    { queryKey: ["users"], queryFn: fetchUsers },
    { queryKey: ["user-keys"], queryFn: fetchUserKeys },
  ] })
  return (
    <PageLayout title={t("nav.usage")} description={t("usage.description")}>
      <ObservabilityTabs value="usage" />
      <QueryState loading={records.isLoading} error={records.error ? t("common.loadError") : ""}>
        <UsageExplorer
          draft={draft} onDraft={setDraft}
          onApply={() => setQuery({ ...draft, page: 1, page_size: query.page_size })}
          onReset={() => { const next = initialQuery(); setDraft(next); setQuery(next) }}
          page={records.data ?? { items: [], total: 0, page: 1, page_size: 10 }}
          summary={summary.data ?? null} summaryError={Boolean(summary.error)} pending={records.isFetching}
          onPage={(page) => setQuery((current) => ({ ...current, page }))}
          onPageSize={(page_size) => setQuery((current) => ({ ...current, page: 1, page_size }))}
          credentials={credentialQuery.data ?? []} providers={providerQuery.data ?? []}
          users={userQuery.data ?? []} keys={keyQuery.data ?? []}
        />
      </QueryState>
    </PageLayout>
  )
}
