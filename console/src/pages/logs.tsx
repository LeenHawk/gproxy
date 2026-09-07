import { useRef, useState } from "react"
import { readPageSize } from "@/components/data-table-state"
import { useQueries, useQuery } from "@tanstack/react-query"
import { useTranslation } from "react-i18next"
import type { LogQueryDto } from "@/generated/LogQueryDto"
import { logDetail, logs } from "@/api/observability"
import { providers } from "@/api/control"
import { userKeys, users } from "@/api/identity"
import { LogExplorer } from "@/components/logs/log-explorer"
import { PageLayout } from "@/components/page-layout"
import { QueryState } from "@/components/query-state"
import { ObservabilityTabs } from "@/components/observability-tabs"
import { adminPath, navigateAdminPath, useAdminLocation } from "@/lib/admin-route"

const now = () => Math.floor(Date.now() / 1000)

function initialQuery(): LogQueryDto {
  const end = now()
  return { start: end - 86_400, end, user_id: null, user_key_id: null, provider_id: null, status: null, request_id: null, cursor: null, limit: readPageSize("logs", 50) }
}

export function LogsPage() {
  const { t } = useTranslation()
  const [draft, setDraft] = useState<LogQueryDto>(initialQuery)
  const [query, setQuery] = useState<LogQueryDto>(draft)
  // The default window ends "now", and now keeps moving: a search that the
  // operator has not pinned to an explicit end must not stop at the moment
  // the page was opened.
  const pinnedEnd = useRef(false)
  const editDraft: typeof setDraft = (update) => setDraft((previous) => {
    const next = typeof update === "function" ? update(previous) : update
    if (next.end !== previous.end) pinnedEnd.current = true
    return next
  })
  const search = () => {
    const end = pinnedEnd.current ? draft.end : now()
    setDraft((value) => ({ ...value, end }))
    setQuery({ ...draft, end, cursor: null })
  }
  const location = useAdminLocation()
  const selected = location.segments[0] ?? null
  const [logQuery, providerQuery, userQuery, keyQuery] = useQueries({ queries: [
    { queryKey: ["logs", query], queryFn: () => logs(query) },
    { queryKey: ["providers"], queryFn: providers },
    { queryKey: ["users"], queryFn: users },
    { queryKey: ["user-keys"], queryFn: userKeys },
  ] })
  const detailQuery = useQuery({ queryKey: ["log-detail", selected], queryFn: () => logDetail(selected!), enabled: selected != null })
  const loading = [logQuery, providerQuery, userQuery, keyQuery].some((item) => item.isLoading)
  const error = [logQuery, providerQuery, userQuery, keyQuery].some((item) => item.error)
  return (
    <PageLayout title={t("logs.title")} description={t("logs.description")}>
      <ObservabilityTabs value="logs" />
      <QueryState loading={loading} error={error ? t("common.loadError") : ""}>
        <LogExplorer
          draft={draft}
          onDraft={editDraft}
          onSearch={() => { navigateAdminPath(adminPath("logs"), true); search() }}
          onReset={() => { const next = initialQuery(); pinnedEnd.current = false; setDraft(next); setQuery(next); navigateAdminPath(adminPath("logs"), true) }}
          page={logQuery.data!}
          providers={providerQuery.data ?? []}
          users={userQuery.data ?? []}
          keys={keyQuery.data ?? []}
          selected={selected}
          onSelect={(requestId) => navigateAdminPath(`/admin/logs/${encodeURIComponent(requestId)}`)}
          detail={detailQuery.data ?? null}
          detailLoading={detailQuery.isLoading}
          detailError={Boolean(detailQuery.error)}
          onNext={(cursor) => setQuery((value) => ({ ...value, cursor }))}
          onPageSize={(limit) => { setDraft((value) => ({ ...value, limit })); setQuery((value) => ({ ...value, limit, cursor: null })) }}
        />
      </QueryState>
    </PageLayout>
  )
}
