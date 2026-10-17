import { ChevronRight } from "lucide-react"
import { Collapsible, CollapsibleContent, CollapsibleTrigger } from "@/components/ui/collapsible"
import { Badge } from "@/components/ui/badge"
import { invalidateConfiguration } from "@/api/invalidation"
import { usePagination } from "@/lib/use-pagination"
//! One page for twelve families.
//!
//! `gproxy-host-axum/src/admin/mod.rs` builds most of `/admin/api` from a
//! `family!` macro, because the families differ in their columns and not in
//! their shape: list, read, create, patch, delete. This component is the other
//! end of that macro. A family here is a declaration — a path, some columns,
//! some fields — and anything it does that the other eleven do not is passed
//! in as a row action.
//!
//! The one cast in this file is at the submit: a form built from a field list
//! produces a `Record<string, unknown>`, and no amount of typing proves that
//! it is the family's `Write`. The proof is the field list matching the DTO,
//! which is what the reviewer of a family declaration is actually checking.

import { useState, type ReactNode } from "react"
import { useMutation, useQueries, useQuery, useQueryClient } from "@tanstack/react-query"
import { useTranslation } from "react-i18next"
import { Pencil, Plus, Trash2 } from "lucide-react"
import { toast } from "sonner"
import { directory } from "@/api/models"
import { users, type Family, type ListFilter } from "@/api/admin"
import { ConfirmButton } from "@/components/confirm"
import { DataTable, Pagination, type Column } from "@/components/data-table"
import { Page, PageHeader } from "@/components/page"
import { RecordDialog, type FormField } from "@/components/record-form"
import { EmptyNotice, QueryState } from "@/components/state"
import { Button } from "@/components/ui/button"
import { useConfigBatch } from "@/components/config-batch"
import type { BatchItem } from "@/generated/sdk"
import { Input } from "@/components/ui/input"


/** One request per user group; the host clamps a page to this anyway. */
const GROUP_SIZE = 500

export type CollectionProps<D, W, P> = {
  /** The nav id: `nav.<id>` names it, `description.<id>` explains it. */
  id: string
  renderForm?: (props: { open: boolean; onOpenChange: (open: boolean) => void; original?: D; onSubmit: (body: Record<string, unknown>) => void; pending: boolean; error: unknown }) => ReactNode
  embedded?: boolean
  paginate?: boolean
  family: Family<D, W, P> & { batch?: (items: BatchItem<W, P>[]) => Promise<(D | null)[]> }
  columns: Array<Column<D>>
  fields: ReadonlyArray<FormField>
  rowId: (row: D) => string
  /** What the delete confirmation names. */
  rowLabel: (row: D) => string
  /** The family's `search` filter answers on a natural name column. */
  batchEnabled?: boolean
  createLabel?: string
  searchable?: boolean
  /** Paginate users, each a group holding its own rows under this filter. */
  groupByOwner?: "userId" | "ownerUserId"
  /** Filters held constant for this page, e.g. a parent id. */
  filter?: ListFilter
  /** Row actions this family has and the others do not. */
  rowActions?: (row: D) => ReactNode
  onOpen?: (row: D) => void
  onEdit?: (row: D) => void
  /** Some families retire rather than delete; some cannot be deleted at all. */
  rowDeletable?: boolean
  deletable?: boolean
  /** What the batch toolbar's removal does; see `useConfigBatch`. */
  batchRemoval?: "delete" | "retire"
  creatable?: boolean
  /** A family whose create answers with more than the row — a minted key. */
  onCreated?: (created: unknown) => void
  /** Replaces `family.create` when the mint returns a richer shape. */
  create?: (body: Record<string, unknown>) => Promise<unknown>
}

export function CollectionPage<D, W, P>({
  id, family, columns, fields, rowId, rowLabel, searchable, groupByOwner, filter, embedded = false,
  rowActions, onOpen, onEdit, deletable = true, batchRemoval, rowDeletable = deletable, creatable = true, onCreated, create, renderForm, createLabel, paginate = true, batchEnabled = fields.some(field => field.name === "enabled"),
}: CollectionProps<D, W, P>) {
  const { t } = useTranslation()
  const client = useQueryClient()
  const { page, pageSize, setPage, setPageSize } = usePagination(family.path)
  const [search, setSearch] = useState("")
  const [editing, setEditing] = useState<D | null>(null)
  const [creating, setCreating] = useState(false)
  const selectionKey = JSON.stringify([family.path, filter, search, page, pageSize])

  const request: ListFilter = { ...filter, page, pageSize, search: search.trim() || undefined }
  const key = ["admin", family.path, request, paginate] as const
  const list = useQuery({ queryKey: key, enabled: !groupByOwner, queryFn: async () => {
    if (paginate) return family.list(request)
    const items = await directory(family, request)
    return { items, total: items.length, offset: 0, limit: items.length }
  } })
  // Grouped, the pager walks users and the search names them; each user's
  // rows come whole from their own request, so a group never spans pages.
  const ownersRequest: ListFilter = { page, pageSize, search: search.trim() || undefined }
  const owners = useQuery({ queryKey: ["admin", users.path, ownersRequest], queryFn: () => users.list(ownersRequest), enabled: !!groupByOwner })
  const groups = useQueries({ queries: (groupByOwner ? owners.data?.items ?? [] : []).map(owner => {
    const groupRequest: ListFilter = { ...filter, [groupByOwner!]: owner.id, pageSize: GROUP_SIZE }
    return { queryKey: ["admin", family.path, groupRequest, true] as const, queryFn: () => family.list(groupRequest) }
  }) })
  const total = (groupByOwner ? owners.data?.total : list.data?.total) ?? 0
  if (paginate && (groupByOwner ? owners.data : list.data) && page > Math.max(1, Math.ceil(total / pageSize))) setPage(Math.max(1, Math.ceil(total / pageSize)))
  const loadedRows = groupByOwner ? groups.flatMap(group => group.data?.items ?? []) : list.data?.items ?? []
  const invalidate = () => invalidateConfiguration(client, family.path)

  const created = useMutation({
    mutationFn: (body: Record<string, unknown>) => (create ?? ((value) => family.create(value as W)))(body),
    onSuccess: (value) => {
      setCreating(false)
      void invalidate()
      toast.success(t("toast.created"))
      onCreated?.(value)
    },
  })

  const updated = useMutation({
    mutationFn: ({ id: rowKey, body }: { id: string; body: Record<string, unknown> }) =>
      family.update(rowKey, body as P),
    onSuccess: () => {
      setEditing(null)
      void invalidate()
      toast.success(t("toast.saved"))
    },
  })

  const removed = useMutation({
    mutationFn: (rowKey: string) => family.remove(rowKey),
    onSuccess: () => {
      void invalidate()
      toast.success(t("toast.deleted"))
    },
    onError: (error: Error) => toast.error(error.message),
  })

  const batch = useConfigBatch({ family, context: selectionKey, rows: loadedRows.map(row => ({ id: rowId(row) })), enableToggle: batchEnabled, deletable, removal: batchRemoval })
  const tableColumns = batch.active ? [{ key: "selection", header: t("management.select"), cell: (row: D) => batch.checkbox(rowId(row), rowLabel(row)) }, ...columns] : columns

  const add = (
    <Button size="sm" disabled={!creatable} onClick={() => setCreating(true)}>
      <Plus data-icon="inline-start" /> {createLabel ?? t("actions.new")}
    </Button>
  )
  const searchInput = searchable || groupByOwner ? (
    <Input
      className="max-w-xs"
      aria-label={t("actions.search")}
      placeholder={t("actions.search")}
      value={search}
      onChange={(event) => { setSearch(event.target.value); setPage(1) }}
    />
  ) : null

  const renderRows = (rows: D[]) => (<DataTable paginate={false}
            columns={tableColumns}
            rows={rows}
            rowKey={rowId}
            onRowClick={onOpen}
            empty={<EmptyNotice title={t("state.emptyTitle")} />}
            actions={(row) => (
              <>
                {onOpen ? <Button variant="ghost" size="sm" onClick={() => onOpen(row)}>{t("actions.details")}<span className="sr-only">: {rowLabel(row)}</span></Button> : null}
                {rowActions?.(row)}
                <Button variant="ghost" size="sm" onClick={() => onEdit ? onEdit(row) : setEditing(row)}><Pencil data-icon="inline-start" />{t("actions.edit")}</Button>
                {rowDeletable ? (
                  <ConfirmButton
                    title={t("confirm.deleteTitle", { name: rowLabel(row) })}
                    onConfirm={() => removed.mutate(rowId(row))}
                  >
                    <Trash2 data-icon="inline-start" />{t("actions.delete")}
                  </ConfirmButton>
                ) : null}
              </>
            )}
          />)

  return (
    <Page>
      {embedded ? (
        <div className="flex flex-wrap items-center justify-between gap-3">{searchInput}<div className="ml-auto flex items-center gap-2">{batch.trigger}{add}</div></div>
      ) : (
        <><PageHeader title={t(`nav.${id}`)} actions={<>{batch.trigger}{add}</>} />{searchInput}</>
      )}
      {batch.toolbar}
      <QueryState isPending={groupByOwner ? owners.isPending : list.isPending} error={groupByOwner ? owners.error : list.error}>
        <div className="flex flex-col gap-3">
          {groupByOwner ? (owners.data?.items.length ? <div className="flex flex-col gap-3">{owners.data.items.map((owner, index) => {
            const group = groups[index]
            return <Collapsible key={owner.id} defaultOpen={owners.data.items.length === 1} className="rounded-lg border">
              <CollapsibleTrigger asChild><Button variant="ghost" className="group h-auto w-full justify-start whitespace-normal p-3"><ChevronRight data-icon="inline-start" className="transition-transform group-data-[state=open]:rotate-90" /><span className="min-w-0 flex-1 break-words text-left">{owner.name}</span><Badge variant={group?.data?.total ? "secondary" : "outline"}>{group?.data?.total ?? "…"}</Badge></Button></CollapsibleTrigger>
              <CollapsibleContent className="flex flex-col gap-2 p-3 pt-0"><QueryState isPending={!group || group.isPending} error={group?.error}>
                {renderRows(group?.data?.items ?? [])}
                {group?.data && group.data.total > group.data.items.length ? <p className="text-xs text-muted-foreground">{t("management.groupTruncated", { shown: group.data.items.length, total: group.data.total })}</p> : null}
              </QueryState></CollapsibleContent>
            </Collapsible>
          })}</div> : <EmptyNotice title={t("state.emptyTitle")} />) : renderRows(list.data?.items ?? [])}
          {paginate ? <Pagination
            page={page}
            pageSize={pageSize}
            total={total}
            onPage={setPage} onPageSize={setPageSize}
          /> : null}
        </div>
      </QueryState>

      {creating ? (renderForm ? renderForm({ open: creating, onOpenChange: setCreating, onSubmit: (body) => created.mutate(body), pending: created.isPending, error: created.error }) : <RecordDialog
        open={creating}
        onOpenChange={setCreating}
        mode="create"
        title={createLabel ?? t(`create.${id}`)}
        fields={fields}
        onSubmit={(body) => created.mutate(body)}
        pending={created.isPending}
        error={created.error}
      />) : null}
      {editing !== null ? (renderForm ? renderForm({ open: editing !== null, onOpenChange: (open) => { if (!open) setEditing(null) }, original: editing ?? undefined, onSubmit: (body) => editing && updated.mutate({ id: rowId(editing), body }), pending: updated.isPending, error: updated.error }) : <RecordDialog
        open={editing !== null}
        onOpenChange={(open) => { if (!open) setEditing(null) }}
        mode="edit"
        title={t(`edit.${id}`)}
        fields={fields}
        original={editing as Record<string, unknown> | undefined}
        onSubmit={(body) => editing && updated.mutate({ id: rowId(editing), body })}
        pending={updated.isPending}
        error={updated.error}
      />) : null}
    </Page>
  )
}
