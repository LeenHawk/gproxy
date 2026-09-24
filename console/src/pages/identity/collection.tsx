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
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query"
import { useTranslation } from "react-i18next"
import { Pencil, Plus, Trash2 } from "lucide-react"
import { toast } from "sonner"
import { directory } from "@/api/models"
import type { Family, ListFilter } from "@/api/admin"
import { ConfirmButton } from "@/components/confirm"
import { DataTable, Pagination, type Column } from "@/components/data-table"
import { Page, PageHeader } from "@/components/page"
import { RecordDialog, type FormField } from "@/components/record-form"
import { EmptyNotice, QueryState } from "@/components/state"
import { Button } from "@/components/ui/button"
import { useConfigBatch } from "@/components/config-batch"
import type { BatchItem } from "@/generated/sdk"
import { Input } from "@/components/ui/input"


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
  /** Filters held constant for this page, e.g. a parent id. */
  filter?: ListFilter
  /** Row actions this family has and the others do not. */
  rowActions?: (row: D) => ReactNode
  onOpen?: (row: D) => void
  onEdit?: (row: D) => void
  /** Some families retire rather than delete; some cannot be deleted at all. */
  deletable?: boolean
  creatable?: boolean
  /** A family whose create answers with more than the row — a minted key. */
  onCreated?: (created: unknown) => void
  /** Replaces `family.create` when the mint returns a richer shape. */
  create?: (body: Record<string, unknown>) => Promise<unknown>
}

export function CollectionPage<D, W, P>({
  id, family, columns, fields, rowId, rowLabel, searchable, filter, embedded = false,
  rowActions, onOpen, onEdit, deletable = true, creatable = true, onCreated, create, renderForm, createLabel, paginate = true, batchEnabled = fields.some(field => field.name === "enabled"),
}: CollectionProps<D, W, P>) {
  const { t } = useTranslation()
  const client = useQueryClient()
  const { page, pageSize, setPage, setPageSize } = usePagination()
  const [search, setSearch] = useState("")
  const [editing, setEditing] = useState<D | null>(null)
  const [creating, setCreating] = useState(false)
  const selectionKey = JSON.stringify([family.path, filter, search, page, pageSize])

  const request: ListFilter = { ...filter, page, pageSize, search: search.trim() || undefined }
  const key = ["admin", family.path, request, paginate] as const
  const list = useQuery({ queryKey: key, queryFn: async () => {
    if (paginate) return family.list(request)
    const items = await directory(family, request)
    return { items, total: items.length, offset: 0, limit: items.length }
  } })
  if (paginate && list.data && page > Math.max(1, Math.ceil(list.data.total / pageSize))) setPage(Math.max(1, Math.ceil(list.data.total / pageSize)))

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

  const batch = useConfigBatch({ family, context: selectionKey, rows: (list.data?.items ?? []).map(row => ({ id: rowId(row) })), enableToggle: batchEnabled, deletable })
  const tableColumns = family.batch ? [{ key: "selection", header: t("management.select"), cell: (row: D) => batch.checkbox(rowId(row), rowLabel(row)) }, ...columns] : columns

  const add = (
    <Button size="sm" disabled={!creatable} onClick={() => setCreating(true)}>
      <Plus data-icon="inline-start" /> {createLabel ?? t("actions.new")}
    </Button>
  )
  const searchInput = searchable ? (
    <Input
      className="max-w-xs"
      aria-label={t("actions.search")}
      placeholder={t("actions.search")}
      value={search}
      onChange={(event) => { setSearch(event.target.value); setPage(1) }}
    />
  ) : null

  return (
    <Page>
      {embedded ? (
        <div className="flex items-center justify-between gap-3">{searchInput}<div className="ml-auto">{add}</div></div>
      ) : (
        <><PageHeader title={t(`nav.${id}`)} actions={add} />{searchInput}</>
      )}
      {batch.toolbar}
      <QueryState isPending={list.isPending} error={list.error}>
        <div className="space-y-3">
          <DataTable paginate={false}
            columns={tableColumns}
            rows={list.data?.items ?? []}
            rowKey={rowId}
            onRowClick={onOpen}
            empty={<EmptyNotice title={t("state.emptyTitle")} />}
            actions={(row) => (
              <>
                {rowActions?.(row)}
                <Button variant="ghost" size="sm" onClick={() => onEdit ? onEdit(row) : setEditing(row)}><Pencil data-icon="inline-start" />{t("actions.edit")}</Button>
                {deletable ? (
                  <ConfirmButton
                    title={t("confirm.deleteTitle", { name: rowLabel(row) })}
                    onConfirm={() => removed.mutate(rowId(row))}
                  >
                    <Trash2 data-icon="inline-start" />{t("actions.delete")}
                  </ConfirmButton>
                ) : null}
              </>
            )}
          />
          {paginate ? <Pagination
            page={page}
            pageSize={pageSize}
            total={list.data?.total ?? 0}
            onPage={setPage} onPageSize={setPageSize}
          /> : null}
        </div>
      </QueryState>

      {renderForm ? renderForm({ open: creating, onOpenChange: setCreating, onSubmit: (body) => created.mutate(body), pending: created.isPending, error: created.error }) : <RecordDialog
        open={creating}
        onOpenChange={setCreating}
        mode="create"
        title={createLabel ?? t(`create.${id}`)}
        fields={fields}
        onSubmit={(body) => created.mutate(body)}
        pending={created.isPending}
        error={created.error}
      />}
      {renderForm ? renderForm({ open: editing !== null, onOpenChange: (open) => { if (!open) setEditing(null) }, original: editing ?? undefined, onSubmit: (body) => editing && updated.mutate({ id: rowId(editing), body }), pending: updated.isPending, error: updated.error }) : <RecordDialog
        open={editing !== null}
        onOpenChange={(open) => { if (!open) setEditing(null) }}
        mode="edit"
        title={t(`edit.${id}`)}
        fields={fields}
        original={editing as Record<string, unknown> | undefined}
        onSubmit={(body) => editing && updated.mutate({ id: rowId(editing), body })}
        pending={updated.isPending}
        error={updated.error}
      />}
    </Page>
  )
}
