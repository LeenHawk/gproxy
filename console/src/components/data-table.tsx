import { Fragment, useDeferredValue, useMemo, useState, type KeyboardEvent, type MouseEvent, type ReactNode } from "react"
import { useTranslation } from "react-i18next"
import { DataTablePagination, type PageSize } from "@/components/data-table-pagination"
import { storePageSize, useColumnVisibility, usePageSize } from "@/components/data-table-state"
import { DataTableToolbar } from "@/components/data-table-toolbar"
import { Card, CardContent } from "@/components/ui/card"
import { Checkbox } from "@/components/ui/checkbox"
import { Empty, EmptyHeader, EmptyTitle } from "@/components/ui/empty"
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from "@/components/ui/table"
import { cn } from "@/lib/utils"

export type DataTableColumn<T> = {
  key: string
  label: string
  header: ReactNode
  cell: (row: T) => ReactNode
  className?: string
}

export type DataTableProps<T> = {
  columns: Array<DataTableColumn<T>>
  rows: Array<T>
  rowKey: (row: T) => string | number
  searchText: (row: T) => string
  renderCard: (row: T) => ReactNode
  renderExpandedRow?: (row: T) => ReactNode
  empty: ReactNode
  storageKey: string
  onRowClick?: (row: T) => void
  activeRowKey?: string | number | null
  selectable?: boolean
  batchActions?: (selectedRows: Array<T>, onApplied: () => void) => ReactNode
  createAction?: ReactNode
  pageSize?: PageSize
  onPageSizeChange?: (pageSize: PageSize) => void
  pagination?: {
    page: number
    pageSize: PageSize
    total: number
    onPage: (page: number) => void
    onPageSize: (size: PageSize) => void
  }
}

const INTERACTIVE = "button, a, input, select, textarea, [role=switch], [role=checkbox], [role=menuitem]"

function interactive(target: EventTarget | null) {
  return target instanceof Element && target.closest(INTERACTIVE) != null
}

export function DataTable<T>({
  columns,
  rows,
  rowKey,
  searchText,
  renderCard,
  renderExpandedRow,
  empty,
  storageKey,
  onRowClick,
  activeRowKey,
  selectable = false,
  batchActions,
  createAction,
  pageSize: initialPageSize = 10,
  onPageSizeChange,
  pagination,
}: DataTableProps<T>) {
  const { t } = useTranslation()
  const [query, setQuery] = useState("")
  const deferredQuery = useDeferredValue(query.trim().toLocaleLowerCase())
  const [page, setPage] = useState(1)
  const [pageSize, setPageSize] = usePageSize(storageKey, initialPageSize)
  const [batchMode, setBatchMode] = useState(false)
  const [selected, setSelected] = useState<Set<string | number>>(() => new Set())
  const { hidden, toggle } = useColumnVisibility(`gproxy.table.${storageKey}.columns`)
  const visibleColumns = columns.filter((column) => !hidden.has(column.key))
  const filtered = useMemo(() => !pagination && deferredQuery
    ? rows.filter((row) => searchText(row).toLocaleLowerCase().includes(deferredQuery))
    : rows, [deferredQuery, rows, searchText, pagination])
  const effectiveSize = pagination?.pageSize ?? pageSize
  const pages = Math.max(1, Math.ceil((pagination?.total ?? filtered.length) / effectiveSize))
  const currentPage = pagination?.page ?? Math.min(page, pages)
  const visibleRows = pagination ? rows : filtered.slice((currentPage - 1) * pageSize, currentPage * pageSize)
  const selectedRows = rows.filter((row) => selected.has(rowKey(row)))
  const selecting = selectable && batchMode
  const allSelected = filtered.length > 0 && filtered.every((row) => selected.has(rowKey(row)))
  const someSelected = filtered.some((row) => selected.has(rowKey(row)))

  const toggleRow = (id: string | number) => setSelected((current) => {
    const next = new Set(current)
    if (next.has(id)) next.delete(id)
    else next.add(id)
    return next
  })
  const toggleAll = () => setSelected((current) => {
    const next = new Set(current)
    for (const row of filtered) {
      const id = rowKey(row)
      if (allSelected) next.delete(id)
      else next.add(id)
    }
    return next
  })
  const exitBatch = () => {
    setBatchMode(false)
    setSelected(new Set())
  }
  const activate = (row: T) => (event: KeyboardEvent) => {
    if (event.key !== "Enter" && event.key !== " ") return
    if (interactive(event.target)) return
    event.preventDefault()
    if (selecting) toggleRow(rowKey(row))
    else onRowClick?.(row)
  }
  // A row that opens on click still holds switches and action buttons; a click that
  // landed on one of those was aimed at it, not at the row.
  const open = (row: T) => (event: MouseEvent) => {
    if (interactive(event.target)) return
    if (selecting) toggleRow(rowKey(row))
    else onRowClick?.(row)
  }

  return (
    <div className="flex min-w-0 flex-col gap-3">
      <DataTableToolbar
        searchable={!pagination}
        query={query}
        onQuery={(value) => { setQuery(value); setPage(1) }}
        columns={columns}
        hidden={hidden}
        onToggleColumn={toggle}
        batchMode={batchMode}
        onToggleBatch={selectable && batchActions ? () => batchMode ? exitBatch() : setBatchMode(true) : undefined}
        createAction={createAction}
      />
      {filtered.length === 0 ? (
        <Empty><EmptyHeader><EmptyTitle>{empty}</EmptyTitle></EmptyHeader></Empty>
      ) : (
        <>
          <div className="hidden overflow-hidden rounded-md border bg-card md:block">
            <Table>
              <TableHeader><TableRow>
                {selecting ? <TableHead className="w-10"><Checkbox checked={allSelected ? true : someSelected ? "indeterminate" : false} onCheckedChange={toggleAll} aria-label={t("common.dataTable.selectAll")} /></TableHead> : null}
                {visibleColumns.map((column) => <TableHead key={column.key} className={column.className}>{column.header}</TableHead>)}
              </TableRow></TableHeader>
              <TableBody>{visibleRows.map((row) => {
                const id = rowKey(row)
                const clickable = selecting || onRowClick != null
                const expanded = !selecting && id === activeRowKey && renderExpandedRow != null
                return <Fragment key={id}><TableRow data-state={selected.has(id) || id === activeRowKey ? "selected" : undefined} aria-expanded={renderExpandedRow && !selecting ? expanded : undefined} tabIndex={clickable ? 0 : undefined} onClick={open(row)} onKeyDown={activate(row)} className={cn(clickable && "cursor-pointer focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-inset")}>
                  {selecting ? <TableCell><Checkbox checked={selected.has(id)} onClick={(event) => event.stopPropagation()} onCheckedChange={() => toggleRow(id)} aria-label={t("common.dataTable.selectRow")} /></TableCell> : null}
                  {visibleColumns.map((column) => <TableCell key={column.key} className={column.className}>{column.cell(row)}</TableCell>)}
                </TableRow>
                  {expanded ? <TableRow><TableCell colSpan={Math.max(visibleColumns.length, 1)} className="whitespace-normal">{renderExpandedRow(row)}</TableCell></TableRow> : null}
                </Fragment>
              })}</TableBody>
            </Table>
          </div>
          <div className="grid gap-2 md:hidden">{visibleRows.map((row) => {
            const id = rowKey(row)
            const clickable = selecting || onRowClick != null
            const expanded = !selecting && id === activeRowKey && renderExpandedRow != null
            return <Fragment key={id}><Card role={clickable ? "button" : undefined} aria-expanded={renderExpandedRow && !selecting ? expanded : undefined} tabIndex={clickable ? 0 : undefined} onClick={open(row)} onKeyDown={activate(row)} className={cn(clickable && "cursor-pointer focus-visible:ring-2 focus-visible:ring-ring", (selected.has(id) || id === activeRowKey) && "ring-2 ring-ring")}>
              <CardContent className="flex items-start gap-3">
                {selecting ? <Checkbox checked={selected.has(id)} onClick={(event) => event.stopPropagation()} onCheckedChange={() => toggleRow(id)} aria-label={t("common.dataTable.selectRow")} /> : null}
                <div className="min-w-0 flex-1">{renderCard(row)}</div>
              </CardContent>
            </Card>
              {expanded ? renderExpandedRow(row) : null}
            </Fragment>
          })}</div>
        </>
      )}
      {filtered.length > 0 || pagination ? <DataTablePagination page={currentPage} pages={pages} pageSize={effectiveSize} onPage={pagination?.onPage ?? setPage} onPageSize={pagination ? (size) => { storePageSize(storageKey, size); pagination.onPageSize(size) } : (size) => { setPageSize(size); setPage(1); onPageSizeChange?.(size) }} /> : null}
      {selecting ? (
        <div className="sticky bottom-3 flex flex-wrap items-center gap-2 rounded-md border bg-background/95 p-2 shadow-sm backdrop-blur">
          <span className="px-2 text-sm text-muted-foreground">{t("common.dataTable.selected", { count: selectedRows.length })}</span>
          <div className="ml-auto flex flex-wrap items-center gap-2">{batchActions?.(selectedRows, exitBatch)}</div>
        </div>
      ) : null}
    </div>
  )
}
