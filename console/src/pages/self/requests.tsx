//! The caller's own recent requests.
//!
//! What is missing from every row is the point: no bodies, no headers, no URL,
//! no client address, no credential id and no provider id — only the
//! provider's display name. That is `PortalRequestDto`, and this page shows
//! all of it and nothing more.

import { useQuery } from "@tanstack/react-query"
import { useTranslation } from "react-i18next"
import * as portal from "@/api/portal"
import { InstantCell, MaybeCell } from "@/components/cells"
import { DataTable, IdCell } from "@/components/data-table"
import { Page, PageHeader } from "@/components/page"
import { EmptyNotice, QueryState } from "@/components/state"
import { Badge } from "@/components/ui/badge"
import { formatDurationMs } from "@/lib/format"

function stateTone(state: string) {
  if (state === "completed") return "success" as const
  if (state === "failed") return "destructive" as const
  if (state === "cancelled") return "warning" as const
  return "outline" as const
}

export function RequestsPage() {
  const { t, i18n } = useTranslation()
  const list = useQuery({ queryKey: ["portal", "requests"], queryFn: portal.requests })
  return (
    <Page>
      <PageHeader title={t("nav.requests")} />
      <QueryState isPending={list.isPending} error={list.error}>
        <DataTable
          columns={[
            { key: "startedAtMs", cell: (row) => <InstantCell value={row.startedAtMs} /> },
            { key: "model", cell: (row) => <MaybeCell value={row.model} mono /> },
            { key: "operation", cell: (row) => <MaybeCell value={row.operation} /> },
            { key: "provider", cell: (row) => <MaybeCell value={row.provider} /> },
            {
              key: "state",
              cell: (row) => <Badge variant={stateTone(row.state)}>{t(`values.${row.state}`)}</Badge>,
            },
            { key: "responseStatus", cell: (row) => <MaybeCell value={row.responseStatus?.toString() ?? null} /> },
            {
              key: "durationMs",
              cell: (row) => (row.durationMs === null ? "—" : formatDurationMs(row.durationMs, i18n.language)),
            },
            { key: "requestId", cell: (row) => <IdCell value={row.requestId} /> },
          ]}
          rows={list.data ?? []}
          rowKey={(row) => row.requestId}
          empty={<EmptyNotice title={t("requests.emptyTitle")} />}
        />
      </QueryState>
    </Page>
  )
}
