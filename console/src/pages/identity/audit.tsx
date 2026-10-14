import { usePagination } from "@/lib/use-pagination"
//! The audit trail.
//!
//! Non-channel API operations, including reads; model and vendor service traffic uses request logs.

import { useState } from "react"
import { useQuery } from "@tanstack/react-query"
import { useTranslation } from "react-i18next"
import * as admin from "@/api/admin"
import { InstantCell, MaybeCell } from "@/components/cells"
import { DataTable, IdCell, Pagination } from "@/components/data-table"
import { Page, PageHeader } from "@/components/page"
import { EmptyNotice, QueryState } from "@/components/state"
import { Badge } from "@/components/ui/badge"
import { Input } from "@/components/ui/input"


export function AuditPage() {
  const { t } = useTranslation()
  const { page, pageSize, setPage, setPageSize } = usePagination("audit")
  const [action, setAction] = useState("")
  const [actor, setActor] = useState("")

  const filter = {
    page,
    pageSize,
    action: action.trim() || undefined,
    actorUserId: actor.trim() || undefined,
  }
  const list = useQuery({ queryKey: ["admin", "/audit", filter], queryFn: () => admin.audit.list(filter) })

  return (
    <Page>
      <PageHeader title={t("nav.audit")} />
      <div className="flex flex-wrap gap-2">
        <Input
          className="max-w-xs"
          aria-label={t("fields.action")} placeholder={t("fields.action")}
          value={action}
          onChange={(event) => { setAction(event.target.value); setPage(1) }}
        />
        <Input
          className="max-w-xs"
          aria-label={t("fields.actorUserId")} placeholder={t("fields.actorUserId")}
          value={actor}
          onChange={(event) => { setActor(event.target.value); setPage(1) }}
        />
      </div>
      <QueryState isPending={list.isPending} error={list.error}>
        <div className="space-y-3">
          <DataTable paginate={false}
            columns={[
              { key: "createdAtMs", cell: (row) => <InstantCell value={row.createdAtMs} /> },
              { key: "action", cell: (row) => <IdCell value={row.action} /> },
              {
                key: "outcome",
                cell: (row) => (
                  <Badge variant={row.outcome === "ok" ? "success" : "destructive"}>{row.outcome}</Badge>
                ),
              },
              { key: "actorUserId", cell: (row) => <MaybeCell value={row.actorUserId} mono /> },
              { key: "entityKind", cell: (row) => <MaybeCell value={row.entityKind} /> },
              { key: "entityId", cell: (row) => <MaybeCell value={row.entityId} mono /> },
              { key: "sourceIp", cell: (row) => <MaybeCell value={row.sourceIp} mono /> },
              {
                key: "detail",
                cell: (row) => (
                  <code className="font-mono text-xs text-muted-foreground">{JSON.stringify(row.detail)}</code>
                ),
              },
            ]}
            rows={list.data?.items ?? []}
            rowKey={(row) => row.id}
            empty={<EmptyNotice title={t("state.emptyTitle")} />}
          />
          <Pagination page={page} pageSize={pageSize} total={list.data?.total ?? 0} onPage={setPage} onPageSize={setPageSize} />
        </div>
      </QueryState>
    </Page>
  )
}
