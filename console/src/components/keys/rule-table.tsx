import { Trash2Icon } from "lucide-react"
import { useTranslation } from "react-i18next"
import { Button } from "@/components/ui/button"
import { Empty, EmptyHeader, EmptyTitle } from "@/components/ui/empty"
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from "@/components/ui/table"

type RuleTableProps = {
  rows: Array<{ id: number; subject: string; detail: string }>
  empty: string
  removeLabel: string
  removingId: number | null
  remove: (id: number) => void
}

export function RuleTable(props: RuleTableProps) {
  const { t } = useTranslation()
  if (props.rows.length === 0) {
    return <Empty><EmptyHeader><EmptyTitle>{props.empty}</EmptyTitle></EmptyHeader></Empty>
  }
  return (
    <div className="overflow-hidden rounded-md border bg-card">
      <Table>
        <TableHeader><TableRow>
          <TableHead>{t("access.subject")}</TableHead>
          <TableHead>{t("access.title")}</TableHead>
          <TableHead className="w-12"><span className="sr-only">{props.removeLabel}</span></TableHead>
        </TableRow></TableHeader>
        <TableBody>{props.rows.map((row) => (
          <TableRow key={row.id}>
            <TableCell className="font-mono text-xs">{row.subject}</TableCell>
            <TableCell className="font-mono text-xs">{row.detail}</TableCell>
            <TableCell>
              <Button size="icon-xs" variant="ghost" aria-label={`${props.removeLabel} ${row.subject}`} disabled={props.removingId != null} onClick={() => props.remove(row.id)}><Trash2Icon /></Button>
            </TableCell>
          </TableRow>
        ))}</TableBody>
      </Table>
    </div>
  )
}
