import { useState } from "react"
import { useQuery } from "@tanstack/react-query"
import { useTranslation } from "react-i18next"
import * as history from "@/api/observation"
import { UsagePage } from "@/pages/self/usage"
import { type HistoryFilter } from "./filters"
import { PageSection } from "@/components/page"
import { DataTable, Pagination, IdCell } from "@/components/data-table"
import { InstantCell } from "@/components/cells"
import { EmptyNotice, QueryState } from "@/components/state"
import { usePagination } from "@/lib/use-pagination"
import { formatCost, formatCount } from "@/lib/format"
import { Input } from "@/components/ui/input"
import { Field, FieldLabel } from "@/components/ui/field"
import { Button } from "@/components/ui/button"
import { Dialog, DialogContent, DialogHeader, DialogTitle, DialogDescription } from "@/components/ui/dialog"
import type { UsageRecordDto } from "@/generated/sdk"

export function GlobalUsagePage() {
  return <UsagePage global renderRecords={filter => <UsageRecords key={JSON.stringify(filter)} filter={filter} />} />
}

function UsageRecords({ filter }: { filter: HistoryFilter }) {
  const { t, i18n } = useTranslation()
  const [requestId, setRequestId] = useState("")
  const { page, pageSize, setPage, setPageSize } = usePagination()
  const [selected, setSelected] = useState<UsageRecordDto | null>(null)
  const records = useQuery({ queryKey: ["admin", "usage-records", filter, requestId, page, pageSize], queryFn: () => history.records({ ...filter, requestId, page, pageSize }) })
  return <div className="flex flex-col gap-6">
    <PageSection title={t("observation.records")}>
      <Field className="max-w-sm"><FieldLabel htmlFor="usage-record-request">{t("observation.requestId")}</FieldLabel><Input id="usage-record-request" value={requestId} onChange={event => { setRequestId(event.target.value); setPage(1) }} /></Field>
      <QueryState isPending={records.isPending} error={records.error}><DataTable rows={records.data?.items ?? []} rowKey={row => row.requestId} empty={<EmptyNotice title={t("usage.emptyTitle")} />} columns={[
        { key: "startedAtMs", cell: row => <InstantCell value={row.startedAtMs} /> },
        { key: "requestId", cell: row => <IdCell value={row.requestId} /> },
        { key: "userId", cell: row => <IdCell value={row.userId ?? "—"} /> },
        { key: "model", cell: row => row.model },
        { key: "inputTokens", cell: row => row.tokens.inputTokens === null ? "—" : formatCount(row.tokens.inputTokens, i18n.language) },
        { key: "outputTokens", cell: row => row.tokens.outputTokens === null ? "—" : formatCount(row.tokens.outputTokens, i18n.language) },
        { key: "cost", cell: row => row.cost === null ? "—" : formatCost(row.cost, i18n.language) },
      ]} actions={row => <Button size="sm" variant="ghost" onClick={() => setSelected(row)}>{t("observation.detail")}</Button>} />
        <Pagination page={page} pageSize={pageSize} total={records.data?.total ?? 0} onPage={setPage} onPageSize={setPageSize} />
      </QueryState>
    </PageSection>
    <Dialog open={selected !== null} onOpenChange={open => { if (!open) setSelected(null) }}><DialogContent className="max-h-[85vh] overflow-y-auto sm:max-w-3xl"><DialogHeader><DialogTitle>{t("observation.records")}</DialogTitle><DialogDescription className="break-all">{selected?.requestId}</DialogDescription></DialogHeader><pre className="overflow-auto whitespace-pre-wrap break-all text-xs">{JSON.stringify(selected, null, 2)}</pre></DialogContent></Dialog>
  </div>
}
