import { useQueries } from "@tanstack/react-query"
import { useMemo } from "react"
import { useTranslation } from "react-i18next"
import { audit } from "@/api/observability"
import { users } from "@/api/identity"
import type { AuditEventDto } from "@/generated/AuditEventDto"
import { DataTable, type DataTableColumn } from "@/components/data-table"
import { ObservabilityTabs } from "@/components/observability-tabs"
import { PageLayout } from "@/components/page-layout"
import { QueryState } from "@/components/query-state"
import { formatInstant } from "@/lib/format"

export function AuditPage() {
  const { t, i18n } = useTranslation()
  const [eventQuery, userQuery] = useQueries({ queries: [
    { queryKey: ["audit"], queryFn: () => audit(500) },
    { queryKey: ["users"], queryFn: users },
  ] })
  const userNames = useMemo(() => new Map((userQuery.data ?? []).map((user) => [user.id, user.name])), [userQuery.data])
  const action = (event: AuditEventDto) => {
    const verb = event.action.split(".").at(-1) ?? event.action
    return `${t(`audit.verbs.${verb}`, { defaultValue: verb })} · ${t(`audit.targets.${event.target_kind}`, { defaultValue: event.target_kind })}`
  }
  const target = (event: AuditEventDto) => event.target_id == null ? t("common.none") : `#${event.target_id}`
  const columns: Array<DataTableColumn<AuditEventDto>> = [
    { key: "time", label: t("audit.time"), header: t("audit.time"), cell: (event) => <span className="text-xs">{formatInstant(event.at, i18n.language)}</span> },
    { key: "actor", label: t("audit.actor"), header: t("audit.actor"), cell: (event) => userNames.get(event.actor_user_id) ?? `#${event.actor_user_id}` },
    { key: "action", label: t("audit.action"), header: t("audit.action"), cell: action },
    { key: "target", label: t("audit.target"), header: t("audit.target"), cell: target },
  ]
  const loading = eventQuery.isLoading || userQuery.isLoading
  const error = eventQuery.error || userQuery.error
  return (
    <PageLayout title={t("audit.title")} description={t("audit.description")}>
      <ObservabilityTabs value="audit" />
      <QueryState loading={loading} error={error ? t("common.loadError") : ""}>
        <DataTable columns={columns} rows={eventQuery.data ?? []} rowKey={(event) => event.id} searchText={(event) => `${userNames.get(event.actor_user_id) ?? event.actor_user_id} ${event.action} ${event.target_kind} ${event.target_id ?? ""}`} renderCard={(event) => <div className="flex flex-col gap-2"><div className="flex items-center justify-between gap-3"><p className="font-medium">{action(event)}</p><span className="text-xs text-muted-foreground">{formatInstant(event.at, i18n.language)}</span></div><p className="text-xs text-muted-foreground">{userNames.get(event.actor_user_id) ?? `#${event.actor_user_id}`} · {target(event)}</p></div>} empty={t("audit.empty")} storageKey="audit-events" />
      </QueryState>
    </PageLayout>
  )
}
