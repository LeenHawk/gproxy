//! One table, one pager. Every list in the console renders through these.

import { useId, type ReactNode } from "react"
import { ChevronLeft, ChevronRight } from "lucide-react"
import { PAGE_SIZES, usePagination, type PageSize } from "@/lib/use-pagination"
import { Field, FieldLabel } from "@/components/ui/field"
import { Select, SelectContent, SelectGroup, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select"
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

/**
 * The actions column is pinned to the right edge of the scroller.
 *
 * A twelve-column family is 1400px wide and a phone is 390, so on every list
 * in this console the row's own verbs — edit, delete, rotate, revoke — sat a
 * thousand pixels past the edge behind a horizontal scroll with nothing to
 * say they were there. Pinned, they are where a row's actions always are, at
 * whatever width, and the columns scroll underneath them.
 *
 * It needs an opaque background for that scrolled content to pass behind, and
 * a left border so the seam reads as an edge rather than a column boundary.
 * At a width where the table already fits there is nothing to stick to and
 * the rule costs nothing, which is why it is not behind a breakpoint.
 */
const STUCK = "sticky right-0 z-10 border-l border-border bg-background"

/**
 * No single column may exceed this.
 *
 * `ui/table` sets `whitespace-nowrap` on every cell, so one untruncated value
 * sets its column's width: an account named
 * `carol-with-a-deliberately-long-account-name-for-layout` or a pair of
 * redirect URIs took 570px of a 390px viewport and pushed every column after
 * it that much further out of reach. Capped and ellipsised, a long value
 * costs its own column and not the whole table; the row's edit dialog is
 * where the untruncated value lives.
 */
const CELL_CAP = "max-w-[18rem] truncate"

/**
 * Row actions, laid out by the table rather than by each caller.
 *
 * A family with four verbs — reveal, rotate, edit, delete — is 260px of
 * buttons, and pinned against a 356px scroller that leaves the row itself 96.
 * Capped and allowed to wrap, four verbs become two rows of two and give half
 * of that back; a family with two stays on one line because it already fits
 * under the cap, so only the crowded families pay the extra row. Above `sm`
 * there is room for the single row and the cap lifts.
 *
 * It is a span inside the cell and not the cell itself: `display: flex` on a
 * `<td>` would stop it being a table cell. And it is `w-max` under the cap
 * because a wrappable cell has a min-content width of one button, which is
 * what the table's column algorithm reaches for when space is tight — every
 * family would then stack its verbs one per line, including the ones that
 * fit. `max-content` asks for the unwrapped row and the cap is what refuses
 * it.
 *
 * The vertical gap is the wider one: the two lines are stacked verbs a thumb
 * has to tell apart, and `Edit` 4px above `Delete` is a mis-tap waiting to
 * happen. Along a line the labels already separate them.
 */
const ACTION_ROW =
  "flex w-max max-w-40 flex-wrap items-center justify-end gap-x-1 gap-y-2 sm:max-w-none sm:flex-nowrap"

type TableProps<T> = {
  columns: Array<Column<T>>
  rows: Array<T>
  rowKey: (row: T) => string
  empty: ReactNode
  /** Rendered in a last, right-aligned column when given. */
  actions?: (row: T) => ReactNode
  onRowClick?: (row: T) => void
  renderCard?: (row: T) => ReactNode
}

// Record lists opt into paging; configuration and reference tables remain whole.
export function DataTable<T>({ paginate = false, resetPageKey = "", storageKey = "", ...props }: TableProps<T> & { paginate?: boolean; resetPageKey?: string; storageKey?: string }) {
  const { page, pageSize, setPage, setPageSize } = usePagination(storageKey, resetPageKey)
  const currentPage = Math.min(page, Math.max(1, Math.ceil(props.rows.length / pageSize)))
  if (paginate && page > currentPage) setPage(currentPage)
  const rows = paginate ? props.rows.slice((currentPage - 1) * pageSize, currentPage * pageSize) : props.rows
  return <div className="flex flex-col gap-3"><TableContent {...props} rows={rows} />{paginate ? <Pagination page={currentPage} pageSize={pageSize} total={props.rows.length} onPage={setPage} onPageSize={setPageSize} /> : null}</div>
}

function TableContent<T>({ columns, rows, rowKey, empty, actions, onRowClick, renderCard }: TableProps<T>) {
  const { t } = useTranslation()
  if (rows.length === 0) return <>{empty}</>
  return (
    // `ui/table` already wraps the table in its own `overflow-x-auto`, and
    // that inner container is the one that scrolls. This wrapper only draws
    // the frame and clips the table's square corners to it — a second
    // `overflow-x-auto` here would be a scroll container that never scrolls
    // and would take the sticky actions column out of the scroller it needs
    // to stick against.
    <>
    {renderCard ? <div className="grid gap-3 md:hidden">{rows.map(row => <div key={rowKey(row)}>{renderCard(row)}</div>)}</div> : null}
    <div className={cn("overflow-hidden rounded-lg border border-border", renderCard && "hidden md:block")}>
      <Table>
        <TableHeader>
          <TableRow>
            {columns.map((column) => (
              <TableHead key={column.key} className={cn(CELL_CAP, column.className)}>
                {column.header ?? t(`fields.${column.key}`)}
              </TableHead>
            ))}
            {actions ? <TableHead className={cn(STUCK, "text-right")}>{t("fields.actions")}</TableHead> : null}
          </TableRow>
        </TableHeader>
        <TableBody>
          {rows.map((row) => (
            <TableRow key={rowKey(row)} className={onRowClick ? "cursor-pointer" : undefined} onClick={onRowClick ? (event) => {
              if (!event.currentTarget.contains(event.target as Node)) return
              if (!(event.target as HTMLElement).closest("a, button, input, select, textarea, [role='switch']")) onRowClick(row)
            } : undefined}>
              {columns.map((column) => (
                <TableCell key={column.key} className={cn(CELL_CAP, column.className)}>{column.cell(row)}</TableCell>
              ))}
              {actions ? (
                <TableCell className={cn(STUCK, "text-right whitespace-nowrap")}>
                  <span className={ACTION_ROW}>{actions(row)}</span>
                </TableCell>
              ) : null}
            </TableRow>
          ))}
        </TableBody>
      </Table>
    </div>
    </>
  )
}

/**
 * A page counter and two steps. Deliberately not a numbered pager: `total`
 * comes back with every page, so "3 of 120" is honest and cheap, while a row
 * of page numbers over a 500-row cap is neither.
 */
export function Pagination({ page, pageSize, total, onPage, onPageSize }: {
  page: number
  pageSize: number
  total: number
  onPage: (page: number) => void
  onPageSize: (pageSize: PageSize) => void
}) {
  const { t } = useTranslation()
  const id = useId()
  const pages = Math.max(1, Math.ceil(total / pageSize))
  return (
    <nav className="flex flex-wrap items-center justify-between gap-3 text-sm" aria-label={t("pagination.position", { page, pages, total })}>
      <Field orientation="horizontal" className="w-auto gap-2">
        <FieldLabel htmlFor={id} className="whitespace-nowrap">{t("pagination.pageSize")}</FieldLabel>
        <Select value={String(pageSize)} onValueChange={value => onPageSize(Number(value) as PageSize)}>
          <SelectTrigger id={id} size="sm"><SelectValue /></SelectTrigger>
          <SelectContent><SelectGroup>{PAGE_SIZES.map(size => <SelectItem key={size} value={String(size)}>{size}</SelectItem>)}</SelectGroup></SelectContent>
        </Select>
      </Field>
      <div className="flex items-center gap-2"><span>{t("pagination.position", { page, pages, total })}</span>
        <Button variant="outline" size="icon-sm" aria-label={t("pagination.previous")} disabled={page <= 1} onClick={() => onPage(page - 1)}><ChevronLeft /></Button>
        <Button variant="outline" size="icon-sm" aria-label={t("pagination.next")} disabled={page >= pages} onClick={() => onPage(page + 1)}><ChevronRight /></Button>
      </div>
    </nav>
  )
}

/**
 * A monospaced id, truncated in the middle of a table without wrapping.
 *
 * The truncation is the point and was missing: a `title` keeps the full value
 * one hover or long-press away, which an opaque id needs more than a name
 * does, since there is nothing to infer it from.
 */
export function IdCell({ value, className }: { value: string; className?: string }) {
  return (
    <code
      title={value}
      className={cn("block max-w-[14rem] truncate font-mono text-xs text-muted-foreground", className)}
    >
      {value}
    </code>
  )
}
