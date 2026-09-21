//! One table, one pager. Every list in the console renders through these.

import type { ReactNode } from "react"
import { useTranslation } from "react-i18next"
import { Button } from "@/components/ui/button"
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from "@/components/ui/table"
import { cn } from "@/lib/utils"

export type Column<T> = {
  /** Also the i18n key under `fields.` for the header. */
  key: string
  /** An explicit header, when `fields.<key>` is not the right word. */
  header?: string
  cell: (row: T) => ReactNode
  className?: string
}

export function DataTable<T>({ columns, rows, rowKey, empty, actions }: {
  columns: Array<Column<T>>
  rows: Array<T>
  rowKey: (row: T) => string
  empty: ReactNode
  /** Rendered in a last, right-aligned column when given. */
  actions?: (row: T) => ReactNode
}) {
  const { t } = useTranslation()
  if (rows.length === 0) return <>{empty}</>
  return (
    <div className="overflow-x-auto rounded-lg border border-border">
      <Table>
        <TableHeader>
          <TableRow>
            {columns.map((column) => (
              <TableHead key={column.key} className={column.className}>
                {column.header ?? t(`fields.${column.key}`)}
              </TableHead>
            ))}
            {actions ? <TableHead className="text-right">{t("fields.actions")}</TableHead> : null}
          </TableRow>
        </TableHeader>
        <TableBody>
          {rows.map((row) => (
            <TableRow key={rowKey(row)}>
              {columns.map((column) => (
                <TableCell key={column.key} className={column.className}>{column.cell(row)}</TableCell>
              ))}
              {actions ? <TableCell className="text-right whitespace-nowrap">{actions(row)}</TableCell> : null}
            </TableRow>
          ))}
        </TableBody>
      </Table>
    </div>
  )
}

/**
 * A page counter and two steps. Deliberately not a numbered pager: `total`
 * comes back with every page, so "3 of 120" is honest and cheap, while a row
 * of page numbers over a 500-row cap is neither.
 */
export function Pagination({ page, pageSize, total, onPage }: {
  page: number
  pageSize: number
  total: number
  onPage: (page: number) => void
}) {
  const { t } = useTranslation()
  const pages = Math.max(1, Math.ceil(total / pageSize))
  if (total <= pageSize) return null
  return (
    <div className="flex items-center justify-between gap-3 text-sm text-muted-foreground">
      <span>{t("pagination.position", { page, pages, total })}</span>
      <div className="flex gap-2">
        <Button variant="outline" size="sm" disabled={page <= 1} onClick={() => onPage(page - 1)}>
          {t("pagination.previous")}
        </Button>
        <Button variant="outline" size="sm" disabled={page >= pages} onClick={() => onPage(page + 1)}>
          {t("pagination.next")}
        </Button>
      </div>
    </div>
  )
}

/** A monospaced id, truncated in the middle of a table without wrapping. */
export function IdCell({ value, className }: { value: string; className?: string }) {
  return <code className={cn("font-mono text-xs text-muted-foreground", className)}>{value}</code>
}
