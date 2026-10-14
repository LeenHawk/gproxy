import { usePagination } from "@/lib/use-pagination"
//! Live console and portal sessions, as the operator sees them.
//!
//! There is no token field and no digest field anywhere in this page, because
//! there is none in `UserSessionDto`: a session can be listed and ended, never
//! read back. Ending one is the whole of the writing half.

import { useState } from "react"
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query"
import { useTranslation } from "react-i18next"
import { toast } from "sonner"
import * as admin from "@/api/admin"
import { InstantCell } from "@/components/cells"
import { ConfirmButton } from "@/components/confirm"
import { DataTable, IdCell, Pagination } from "@/components/data-table"
import { Page, PageHeader } from "@/components/page"
import { EmptyNotice, QueryState } from "@/components/state"
import { Input } from "@/components/ui/input"


export function SessionsPage() {
  const { t } = useTranslation()
  const client = useQueryClient()
  const { page, pageSize, setPage, setPageSize } = usePagination("sessions")
  const [userId, setUserId] = useState("")

  const filter = { page, pageSize, userId: userId.trim() || undefined }
  const list = useQuery({ queryKey: ["admin", "/sessions", filter], queryFn: () => admin.sessions.list(filter) })
  const revoke = useMutation({
    mutationFn: (id: string) => admin.sessions.revoke(id),
    onSuccess: () => {
      void client.invalidateQueries({ queryKey: ["admin", "/sessions"] })
      toast.success(t("toast.revoked"))
    },
    onError: (error: Error) => toast.error(error.message),
  })

  return (
    <Page>
      <PageHeader title={t("nav.sessions")} />
      <Input
        className="max-w-xs"
        aria-label={t("fields.userId")} placeholder={t("fields.userId")}
        value={userId}
        onChange={(event) => { setUserId(event.target.value); setPage(1) }}
      />
      <QueryState isPending={list.isPending} error={list.error}>
        <div className="space-y-3">
          <DataTable paginate={false}
            columns={[
              { key: "userId", cell: (row) => <IdCell value={row.userId} /> },
              { key: "createdAtMs", cell: (row) => <InstantCell value={row.createdAtMs} /> },
              { key: "expiresAtMs", cell: (row) => <InstantCell value={row.expiresAtMs} /> },
              { key: "id", cell: (row) => <IdCell value={row.id} /> },
            ]}
            rows={list.data?.items ?? []}
            rowKey={(row) => row.id}
            empty={<EmptyNotice title={t("state.emptyTitle")} />}
            actions={(row) => (
              <ConfirmButton
                title={t("confirm.revokeTitle", { name: row.id })}
                confirmLabel={t("actions.revoke")}
                onConfirm={() => revoke.mutate(row.id)}
              >
                {t("actions.revoke")}
              </ConfirmButton>
            )}
          />
          <Pagination page={page} pageSize={pageSize} total={list.data?.total ?? 0} onPage={setPage} onPageSize={setPageSize} />
        </div>
      </QueryState>
    </Page>
  )
}
