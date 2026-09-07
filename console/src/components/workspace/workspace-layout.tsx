import { useState, type CSSProperties, type ReactNode } from "react"
import { useTranslation } from "react-i18next"
import { DataTablePagination } from "@/components/data-table-pagination"
import { usePageSize } from "@/components/data-table-state"
import { Button } from "@/components/ui/button"
import { Checkbox } from "@/components/ui/checkbox"
import { Input } from "@/components/ui/input"
import { useWorkspacePaneWidth } from "@/components/workspace/use-workspace-pane-width"
import { WorkspaceResizeHandle } from "@/components/workspace/workspace-resize-handle"
import { cn } from "@/lib/utils"

type Props<T extends { id: number }> = {
  storageKey: string
  title: string
  items: Array<T>
  selectedId: number | null
  detailOpen?: boolean
  getSearchText: (item: T) => string
  renderTitle: (item: T) => ReactNode
  renderSummary: (item: T) => ReactNode
  renderAction?: (item: T) => ReactNode
  onSelect: (item: T) => void
  onBack: () => void
  searchPlaceholder: string
  emptyLabel: string
  resizeLabel: string
  selectAllLabel: string
  selectRowLabel: (item: T) => string
  selectedLabel: (count: number) => string
  mobileBackLabel: string
  createAction?: ReactNode
  emptyState: ReactNode
  batchActions?: (items: Array<T>, onApplied: () => void) => ReactNode
  children: ReactNode
}

export function WorkspaceLayout<T extends { id: number }>(props: Props<T>) {
  const { t } = useTranslation()
  const [query, setQuery] = useState("")
  const [page, setPage] = useState(1)
  const [pageSize, setPageSize] = usePageSize(props.storageKey.replace(/^gproxy\./, "").replace(/\.width$/, ""), 10)
  const [batchMode, setBatchMode] = useState(false)
  const [selectedIds, setSelectedIds] = useState<Set<number>>(() => new Set())
  const pane = useWorkspacePaneWidth(props.storageKey)
  const needle = query.trim().toLocaleLowerCase()
  const filtered = needle
    ? props.items.filter((item) => props.getSearchText(item).toLocaleLowerCase().includes(needle))
    : props.items
  const pages = Math.max(1, Math.ceil(filtered.length / pageSize))
  const currentPage = Math.min(page, pages)
  const visibleItems = filtered.slice((currentPage - 1) * pageSize, currentPage * pageSize)
  const selectedItems = props.items.filter((item) => selectedIds.has(item.id))
  const allSelected = filtered.length > 0 && filtered.every((item) => selectedIds.has(item.id))

  const toggle = (id: number, checked: boolean) => setSelectedIds((current) => {
    const next = new Set(current)
    if (checked) next.add(id)
    else next.delete(id)
    return next
  })
  const toggleAll = (checked: boolean) => setSelectedIds((current) => {
    const next = new Set(current)
    for (const item of filtered) {
      if (checked) next.add(item.id)
      else next.delete(item.id)
    }
    return next
  })
  const exitBatch = () => {
    setBatchMode(false)
    setSelectedIds(new Set())
  }
  const hasDetail = props.detailOpen ?? props.selectedId != null

  return (
    <div className="flex min-h-[calc(100svh-10rem)] overflow-hidden rounded-xl border bg-background">
      <aside
        style={{ "--workspace-pane-width": `${pane.width}px` } as CSSProperties}
        className={cn("w-full flex-col border-r md:flex md:w-[var(--workspace-pane-width)] md:shrink-0", hasDetail ? "hidden md:flex" : "flex")}
      >
        <div className="grid gap-3 border-b p-3">
          <div className="flex items-center justify-between gap-2">
            <h1 className="truncate text-lg font-semibold">{props.title}</h1>
            <div className="flex items-center gap-1">
              {props.batchActions ? <Button size="sm" variant="outline" onClick={() => batchMode ? exitBatch() : setBatchMode(true)}>{t(`common.batch.${batchMode ? "cancel" : "select"}`)}</Button> : null}
              {!batchMode ? props.createAction : null}
            </div>
          </div>
          <Input value={query} onChange={(event) => { setQuery(event.target.value); setPage(1) }} placeholder={props.searchPlaceholder} aria-label={props.searchPlaceholder} />
          {batchMode ? (
            <label className="flex items-center gap-2 text-xs text-muted-foreground">
              <Checkbox checked={allSelected} onCheckedChange={(checked) => toggleAll(checked === true)} aria-label={props.selectAllLabel} />
              {props.selectAllLabel}
            </label>
          ) : null}
        </div>
        <div className="flex-1 overflow-x-hidden overflow-y-auto p-2">
          {filtered.length === 0 ? <p className="p-3 text-sm text-muted-foreground">{props.emptyLabel}</p> : null}
          <ul className="grid gap-1">
            {visibleItems.map((item) => (
              <li key={item.id} className={cn("flex min-h-14 items-center gap-2 rounded-md border border-transparent", item.id === props.selectedId ? "bg-accent text-accent-foreground" : "hover:bg-muted/60")}>
                {batchMode ? <Checkbox className="ml-3" checked={selectedIds.has(item.id)} onCheckedChange={(checked) => toggle(item.id, checked === true)} aria-label={props.selectRowLabel(item)} /> : null}
                <button type="button" className="min-w-0 flex-1 px-3 py-2 text-left" onClick={() => batchMode ? toggle(item.id, !selectedIds.has(item.id)) : props.onSelect(item)}>
                  <span className="block truncate text-sm font-medium">{props.renderTitle(item)}</span>
                  <span className="mt-0.5 flex min-w-0 items-center gap-1.5 text-xs text-muted-foreground">{props.renderSummary(item)}</span>
                </button>
                {!batchMode && props.renderAction ? <div className="shrink-0 pr-3">{props.renderAction(item)}</div> : null}
              </li>
            ))}
          </ul>
        </div>
        {filtered.length > 0 ? <div className="border-t p-2"><DataTablePagination page={currentPage} pages={pages} pageSize={pageSize} onPage={setPage} onPageSize={(size) => { setPageSize(size); setPage(1) }} /></div> : null}
        {batchMode && props.batchActions ? (
          <div className="flex items-center justify-between gap-2 border-t p-2">
            <span className="text-xs text-muted-foreground">{props.selectedLabel(selectedItems.length)}</span>
            <div className="flex items-center gap-1">{props.batchActions(selectedItems, exitBatch)}</div>
          </div>
        ) : null}
      </aside>
      <WorkspaceResizeHandle label={props.resizeLabel} width={pane.width} minWidth={pane.minWidth} maxWidth={pane.maxWidth} onWidthChange={pane.setWidth} onReset={pane.resetWidth} />
      <section className={cn("min-w-0 flex-1 p-4", hasDetail ? "block" : "hidden md:block")}>
        {hasDetail ? <><Button className="mb-3 md:hidden" variant="ghost" onClick={props.onBack}>{props.mobileBackLabel}</Button>{props.children}</> : props.emptyState}
      </section>
    </div>
  )
}
