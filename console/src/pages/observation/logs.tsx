import { useState } from "react"
import { useQuery, useQueryClient } from "@tanstack/react-query"
import { useTranslation } from "react-i18next"
import * as history from "@/api/observation"
import { DataTable, IdCell } from "@/components/data-table"
import { InstantCell, MaybeCell } from "@/components/cells"
import { Page, PageHeader, PageSection } from "@/components/page"
import { EmptyNotice, QueryState } from "@/components/state"
import { Button } from "@/components/ui/button"
import { Badge } from "@/components/ui/badge"
import { Dialog, DialogContent, DialogHeader, DialogTitle, DialogDescription } from "@/components/ui/dialog"
import type { CaptureRecordDto, CaptureEventDto, LogBodyDto } from "@/generated/sdk"
import { HistoryFilters, type HistoryFilter } from "./filters"
import { useHistoryBatch } from "./history-batch"

type Cursor = { cursor?: number; cursorId?: string }
function Body({ body }: { body: LogBodyDto }) {
  const { t } = useTranslation()
  return <div className="flex flex-col gap-2"><span><Badge variant="outline">{body.state}</Badge> · {body.encoding} · {body.bytes} B {body.truncated ? t("observation.truncated") : ""}</span><pre className="max-h-80 overflow-auto whitespace-pre-wrap break-all text-xs">{body.content || t("observation.noContent")}</pre></div>
}
function Exchange({ record, events }: { record: CaptureRecordDto; events: CaptureEventDto[] }) {
  const { t } = useTranslation()
  return <section className="flex min-w-0 flex-col gap-4 rounded-lg border p-4">
    <h3 className="break-all font-medium">{record.side} · {record.id}</h3>
    <dl className="grid grid-cols-2 gap-2 text-sm">{["initiatorRequestId", "attemptId", "attemptOrdinal", "userId", "apiKeyId", "providerId", "credentialId", "model", "operation", "state", "responseStatus", "requestMethod", "requestUrl", "requestQuery", "clientIp", "error", "reason"].map(key => <div key={key} className="min-w-0"><dt className="text-muted-foreground">{t(`observation.${key}`)}</dt><dd className="break-all">{key === "reason" && record.reason ? t(`observation.reasons.${record.reason}`, { defaultValue: record.reason }) : String(record[key as keyof CaptureRecordDto] ?? "—")}</dd></div>)}</dl>
    <PageSection title={t("observation.requestHeaders")}><pre className="overflow-auto whitespace-pre-wrap break-all text-xs">{JSON.stringify(record.requestHeaders, null, 2)}</pre></PageSection>
    <PageSection title={t("observation.requestBody")}><Body body={record.requestBody} /></PageSection>
    <PageSection title={t("observation.responseHeaders")}><pre className="overflow-auto whitespace-pre-wrap break-all text-xs">{JSON.stringify(record.responseHeaders, null, 2)}</pre></PageSection>
    <PageSection title={t("observation.responseBody")}><Body body={record.responseBody} /></PageSection>
    {record.metrics ? <PageSection title={t("observation.metrics")}><pre className="overflow-auto whitespace-pre-wrap break-all text-xs">{JSON.stringify(record.metrics, null, 2)}</pre></PageSection> : null}
    {events.length ? <PageSection title={t("observation.events")}>{events.map(event => <details key={event.sequence} className="min-w-0"><summary className="cursor-pointer">#{event.sequence} · {event.direction} · {event.kind}</summary><Body body={event.payload} /></details>)}</PageSection> : null}
  </section>
}
function LogDetail({ id, side }: { id: string; side: history.LogSide }) {
  const { t } = useTranslation()
  const result = useQuery({ queryKey: ["admin", "logs", side, id], queryFn: async () => {
    if (side === "downstream") { const data = await history.detail(id); return { records: [data.downstream, ...data.upstream], events: data.events, truncated: data.eventsTruncated, usage: data.usage } }
    const data = await history.capture(id)
    return { records: [data.record], events: data.events, truncated: data.eventsTruncated, usage: null }
  } })
  return <QueryState isPending={result.isPending} error={result.error}>{result.data ? <div className="flex flex-col gap-4">
    {result.data.truncated ? <p role="status">{t("observation.truncated")}</p> : null}
    {result.data.records.map(record => <Exchange key={record.id} record={record} events={result.data.events.filter(event => event.captureId === record.id)} />)}
    {result.data.usage?.length ? <PageSection title={t("nav.usage")}><pre className="overflow-auto whitespace-pre-wrap break-all text-xs">{JSON.stringify(result.data.usage, null, 2)}</pre></PageSection> : null}
  </div> : null}</QueryState>
}
function LogsPage({ side }: { side: history.LogSide }) {
  const { t } = useTranslation()
  const [filter, setFilter] = useState<HistoryFilter>({})
  const [cursors, setCursors] = useState<Cursor[]>([{}])
  const [selected, setSelected] = useState<string | null>(null)
  const list = useQuery({ queryKey: ["admin", "logs", side, filter, cursors.at(-1)], queryFn: () => history.logs(side, { ...filter, ...cursors.at(-1), limit: 50 }) })
  const client = useQueryClient()
  const batch = useHistoryBatch({ context: JSON.stringify([side, filter, cursors.at(-1)]), rows: list.data?.items ?? [], remove: ids => history.deleteLogs(side, ids), clear: () => history.clearLogs(side), onDeleted: async () => {
    setCursors([{}])
    await client.invalidateQueries({ queryKey: ["admin", "logs", side] })
  } })
  return <Page><PageHeader title={t(side === "downstream" ? "nav.downstreamLogs" : "nav.upstreamLogs")} actions={<>{batch.actions}<Button variant="outline" size="sm" onClick={() => void list.refetch()}>{t("observation.refresh")}</Button></>} />
    <HistoryFilters logs reasons={side === "upstream"} onApply={value => { setFilter(value); setCursors([{}]) }} />
    {batch.toolbar}
    <QueryState isPending={list.isPending} error={list.error}><DataTable rows={list.data?.items ?? []} rowKey={row => row.requestId} empty={<EmptyNotice title={t("requests.emptyTitle")} />} columns={[
      ...batch.column,
      { key: "startedAtMs", cell: row => <InstantCell value={row.startedAtMs} /> },
      { key: "requestId", cell: row => <IdCell value={row.requestId} /> },
      ...(["userId", "model", "operation", "providerId", "credentialId", "state", "responseStatus"] as const).map(key => ({ key, header: t(`observation.${key}`), cell: (row: NonNullable<typeof list.data>["items"][number]) => <MaybeCell value={row[key]?.toString() ?? null} /> })),
      ...(side === "upstream" ? [{ key: "reason", header: t("observation.reason"), className: "min-w-28", cell: (row: NonNullable<typeof list.data>["items"][number]) => row.reason ? <Badge variant="outline">{t(`observation.reasons.${row.reason}`, { defaultValue: row.reason })}</Badge> : <MaybeCell value={null} /> }] : []),
      { key: "requestUrl", header: t("observation.requestUrl"), cell: row => <MaybeCell value={row.requestUrl} /> },
    ]} actions={row => <Button size="sm" variant="ghost" onClick={() => setSelected(row.requestId)}>{t("observation.detail")}</Button>} />
      <div className="mt-3 flex gap-2"><Button variant="outline" size="sm" disabled={cursors.length === 1 || list.isFetching} onClick={() => setCursors(cursors.slice(0, -1))}>{t("observation.previous")}</Button><Button variant="outline" size="sm" disabled={list.data?.nextCursor == null || list.isFetching} onClick={() => { if (list.data?.nextCursor != null && list.data.nextCursorId) setCursors([...cursors, { cursor: list.data.nextCursor, cursorId: list.data.nextCursorId }]) }}>{t("observation.next")}</Button></div>
    </QueryState>
    <Dialog open={selected !== null} onOpenChange={open => { if (!open) setSelected(null) }}><DialogContent className="max-h-[85vh] overflow-y-auto sm:max-w-4xl"><DialogHeader><DialogTitle>{t("observation.detail")}</DialogTitle><DialogDescription className="break-all">{selected}</DialogDescription></DialogHeader>{selected ? <LogDetail id={selected} side={side} /> : null}</DialogContent></Dialog>
  </Page>
}
export function DownstreamLogsPage() { return <LogsPage side="downstream" /> }
export function UpstreamLogsPage() { return <LogsPage side="upstream" /> }
