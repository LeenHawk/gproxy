import { Trash2Icon } from "lucide-react"
import { useTranslation } from "react-i18next"
import { DataTable, type DataTableColumn } from "@/components/data-table"
import { BatchActions } from "@/components/batch-actions"
import type { Entity } from "@/generated/Entity"
import { Button } from "@/components/ui/button"

type RuleTableProps = {
  rows: Array<{ id: number; subject: string; detail: string }>
  empty: string
  removeLabel: string
  removingId: number | null
  remove: (id: number) => void
  storageKey?: string
  entity: Entity
}

export function RuleTable(props: RuleTableProps) {
  const { t } = useTranslation()
  const remove = (row: RuleTableProps["rows"][number]) => <Button size="icon-xs" variant="ghost" aria-label={`${props.removeLabel} ${row.subject}`} disabled={props.removingId != null} onClick={() => props.remove(row.id)}><Trash2Icon /></Button>
  const columns: Array<DataTableColumn<RuleTableProps["rows"][number]>> = [
    { key: "subject", label: t("access.subject"), header: t("access.subject"), cell: (row) => <span className="font-mono text-xs">{row.subject}</span> },
    { key: "detail", label: t("access.title"), header: t("access.title"), cell: (row) => <span className="font-mono text-xs">{row.detail}</span> },
    { key: "actions", label: props.removeLabel, header: <span className="sr-only">{props.removeLabel}</span>, cell: remove },
  ]
  return (
    <DataTable columns={columns} rows={props.rows} rowKey={(row) => row.id} searchText={(row) => `${row.subject} ${row.detail}`} renderCard={(row) => <div className="flex items-start justify-between gap-3"><div><p className="font-mono text-xs">{row.subject}</p><p className="mt-1 font-mono text-xs text-muted-foreground">{row.detail}</p></div>{remove(row)}</div>} empty={props.empty} storageKey={props.storageKey ?? "access-rules"} selectable batchActions={(rows, onApplied) => <BatchActions entity={props.entity} rows={rows} queryKeys={[props.entity]} toggle={props.entity === "quotas"} remove onApplied={onApplied} />} />
  )
}
