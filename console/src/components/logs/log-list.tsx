import { useTranslation } from "react-i18next"
import type { LogPageDto } from "@/generated/LogPageDto"
import type { LogListItemDto } from "@/generated/LogListItemDto"
import { DataTable, type DataTableColumn } from "@/components/data-table"
import type { PageSize } from "@/components/data-table-state"
import { Button } from "@/components/ui/button"
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card"
import { formatInstant, formatNumber } from "@/lib/format"
import { cn } from "@/lib/utils"

export function LogList({ page, pageSize, selected, onSelect, onNext, onPageSize }: { page: LogPageDto; pageSize: PageSize; selected: string | null; onSelect: (requestId: string) => void; onNext: (cursor: number) => void; onPageSize: (pageSize: PageSize) => void }) {
  const { t, i18n } = useTranslation()
  const duration = (item: LogListItemDto) => item.duration_ms == null ? t("common.none") : t("logs.durationValue", { value: formatNumber(item.duration_ms, i18n.language) })
  const tps = (item: LogListItemDto) => item.tps == null ? t("common.none") : t("logs.tpsValue", { value: formatNumber(Number(item.tps), i18n.language) })
  const metrics = (item: LogListItemDto) => `${item.client_ip ?? t("common.none")} · ${duration(item)} · ${tps(item)}`
  const columns: Array<DataTableColumn<LogListItemDto>> = [
    { key: "request", label: t("logs.list.title"), header: t("logs.list.title"), cell: (item) => <div><p className="font-medium">{item.method} {item.path}</p><p className="font-mono text-xs text-muted-foreground">{item.request_id}</p><p className="text-xs text-muted-foreground">{metrics(item)}</p></div> },
    { key: "status", label: t("logs.filters.status"), header: t("logs.filters.status"), cell: (item) => <span className={cn("font-mono text-xs", item.response_status != null && item.response_status >= 400 ? "text-destructive" : "text-muted-foreground")}>{item.response_status ?? t("logs.pending")}</span> },
    { key: "time", label: t("common.filters.start"), header: t("common.filters.start"), cell: (item) => <span className="text-xs text-muted-foreground">{formatInstant(item.at, i18n.language)}</span> },
  ]
  return (
    <Card size="sm" className="min-w-0">
      <CardHeader><CardTitle>{t("logs.list.title")}</CardTitle></CardHeader>
      <CardContent className="flex flex-col gap-2">
        <DataTable columns={columns} rows={page.items} rowKey={(item) => item.id} searchText={(item) => `${item.method} ${item.path} ${item.request_id} ${item.response_status ?? ""} ${metrics(item)}`} renderCard={(item) => <div className="grid gap-1"><span className="flex items-center justify-between gap-3"><span className="truncate font-medium">{item.method} {item.path}</span><span className={cn("font-mono text-xs", item.response_status != null && item.response_status >= 400 ? "text-destructive" : "text-muted-foreground")}>{item.response_status ?? t("logs.pending")}</span></span><span className="truncate font-mono text-xs text-muted-foreground">{item.request_id}</span><span className="text-xs text-muted-foreground">{metrics(item)}</span><span className="text-xs text-muted-foreground">{formatInstant(item.at, i18n.language)}</span></div>} empty={t("logs.list.empty")} storageKey="logs" pageSize={pageSize} onPageSizeChange={onPageSize} activeRowKey={page.items.find((item) => item.request_id === selected)?.id} onRowClick={(item) => onSelect(item.request_id)} />
        {page.next_cursor != null ? <Button variant="outline" onClick={() => onNext(page.next_cursor!)}>{t("logs.list.next")}</Button> : null}
      </CardContent>
    </Card>
  )
}
