import { useRef, useState } from "react"
import { readPageSize } from "@/components/data-table-state"
import { keepPreviousData, useQueries } from "@tanstack/react-query"
import { useTranslation } from "react-i18next"
import type { UsageRecordQueryDto } from "@/generated/UsageRecordQueryDto"
import { credentialCycles, usageRecords, usageSummary } from "@/api/observability"
import { credentials as fetchCredentials, providers as fetchProviders } from "@/api/control"
import { userKeys as fetchUserKeys, users as fetchUsers } from "@/api/identity"
import { PageLayout } from "@/components/page-layout"
import { QueryState } from "@/components/query-state"
import { UsageExplorer } from "@/components/usage/usage-explorer"
import { QuotaHistory } from "@/components/usage/quota-history"
import { ObservabilityTabs } from "@/components/observability-tabs"
import { ToggleGroup, ToggleGroupItem } from "@/components/ui/toggle-group"

type UsageView = "records" | "quotas"

const now = () => Math.floor(Date.now() / 1000)

function initialQuery(): UsageRecordQueryDto {
  const to = now()
  return { from: to - 7 * 86_400, to, user_key_id: null, user_id: null, provider_id: null, credential_id: null, model: null, request_id: null, operation: null, usage_source: null, ended: null, page: 1, page_size: readPageSize("usage-records", 10) }
}

export function UsagePage() {
  const { t } = useTranslation()
  const [view, setView] = useState<UsageView>("records")
  const [draft, setDraft] = useState<UsageRecordQueryDto>(initialQuery)
  const [query, setQuery] = useState<UsageRecordQueryDto>(draft)
  // Same rule as the request audit: an unpinned end of the range follows the clock.
  const pinnedTo = useRef(false)
  const editDraft: typeof setDraft = (update) => setDraft((previous) => {
    const next = typeof update === "function" ? update(previous) : update
    if (next.to !== previous.to) pinnedTo.current = true
    return next
  })
  const apply = () => {
    const to = pinnedTo.current ? draft.to : now()
    setDraft((value) => ({ ...value, to }))
    setQuery({ ...draft, to, page: 1, page_size: query.page_size })
  }
  const filter = { ...query, page: null, page_size: null }
  const [records, summary, credentialQuery, providerQuery, userQuery, keyQuery, cycleQuery] = useQueries({ queries: [
    { queryKey: ["usage-records", query], queryFn: () => usageRecords(query), placeholderData: keepPreviousData, enabled: view === "records" },
    { queryKey: ["usage-summary", filter], queryFn: () => usageSummary(query), enabled: view === "records" },
    { queryKey: ["credentials"], queryFn: fetchCredentials },
    { queryKey: ["providers"], queryFn: fetchProviders },
    { queryKey: ["users"], queryFn: fetchUsers, enabled: view === "records" },
    { queryKey: ["user-keys"], queryFn: fetchUserKeys, enabled: view === "records" },
    { queryKey: ["credential-cycles", "history", query.from, query.to, query.credential_id], queryFn: () => credentialCycles(query.from, query.to, query.credential_id ?? undefined, true), refetchInterval: view === "quotas" ? 60_000 : false, enabled: view === "quotas" },
  ] })
  const loading = view === "records" ? records.isLoading : cycleQuery.isLoading
  const error = view === "records" ? records.error : cycleQuery.error
  return (
    <PageLayout title={t("nav.usage")} description={t("usage.description")}>
      <ObservabilityTabs value="usage" />
      <ToggleGroup type="single" variant="outline" size="sm" spacing={0} value={view} aria-label={t("usage.view.label")} onValueChange={(next) => { if (next) setView(next as UsageView) }}>
        <ToggleGroupItem value="records">{t("usage.view.records")}</ToggleGroupItem>
        <ToggleGroupItem value="quotas">{t("usage.view.quotas")}</ToggleGroupItem>
      </ToggleGroup>
      <QueryState loading={loading} error={error ? t("common.loadError") : ""}>
        <UsageExplorer
          view={view}
          draft={draft} onDraft={editDraft}
          onApply={apply}
          onReset={() => { const next = initialQuery(); pinnedTo.current = false; setDraft(next); setQuery(next) }}
          page={records.data ?? { items: [], total: 0, page: 1, page_size: 10 }}
          summary={summary.data ?? null} summaryError={Boolean(summary.error)} pending={records.isFetching}
          onPage={(page) => setQuery((current) => ({ ...current, page }))}
          onPageSize={(page_size) => setQuery((current) => ({ ...current, page: 1, page_size }))}
          credentials={credentialQuery.data ?? []} providers={providerQuery.data ?? []}
          users={userQuery.data ?? []} keys={keyQuery.data ?? []}
        >
          <QuotaHistory
            cycles={(cycleQuery.data ?? []).filter((cycle) => query.provider_id == null || credentialQuery.data?.some((credential) => credential.id === cycle.credential_id && credential.provider_id === query.provider_id))}
            providers={providerQuery.data ?? []} credentials={credentialQuery.data ?? []}
            loading={cycleQuery.isLoading || providerQuery.isLoading || credentialQuery.isLoading}
            error={cycleQuery.isError || providerQuery.isError || credentialQuery.isError}
          />
        </UsageExplorer>
      </QueryState>
    </PageLayout>
  )
}
