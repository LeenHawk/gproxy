import { useState } from "react"
import { useMutation } from "@tanstack/react-query"
import { useTranslation } from "react-i18next"
import { toast } from "sonner"
import { ListChecks, Trash2 } from "lucide-react"
import type { HistoryDeleted } from "@/generated/sdk"
import { Checkbox } from "@/components/ui/checkbox"
import { Button } from "@/components/ui/button"
import { Separator } from "@/components/ui/separator"
import { ConfirmButton } from "@/components/confirm"
import type { Column } from "@/components/data-table"

/** Batch selection over one page of history rows, plus deleting everything. */
export function useHistoryBatch<T extends { requestId: string }>({ context, rows, remove, clear, onDeleted }: { context: string; rows: T[]; remove: (ids: string[]) => Promise<HistoryDeleted>; clear: () => Promise<HistoryDeleted>; onDeleted: () => Promise<unknown> }) {
  const { t } = useTranslation()
  const ids = rows.map(row => row.requestId)
  const [active, setActive] = useState(false)
  const [selection, setSelection] = useState<{ context: string; ids: string[] }>({ context, ids: [] })
  const selected = selection.context === context ? selection.ids.filter(id => ids.includes(id)) : []
  const select = (next: string[]) => setSelection({ context, ids: next })
  const action = useMutation({ mutationFn: (target: string[] | null) => target ? remove(target) : clear(), onSuccess: async result => {
    select([])
    toast.success(t("observation.deleted", { count: result.deleted }))
    await onDeleted()
  }, onError: (error: Error) => toast.error(error.message) })
  const checkbox = ({ requestId: id }: T) => <Checkbox aria-label={`${t("management.select")}: ${id}`} checked={selected.includes(id)} disabled={action.isPending} onClick={event => event.stopPropagation()} onCheckedChange={checked => select(checked ? [...selected, id] : selected.filter(value => value !== id))} />
  const actions = <>
    <Button variant={active ? "secondary" : "outline"} size="sm" aria-pressed={active} disabled={action.isPending} onClick={() => { setActive(!active); select([]) }}><ListChecks data-icon="inline-start" />{t("management.selectPage")}</Button>
    <ConfirmButton variant="outline" disabled={action.isPending} title={t("observation.deleteAllConfirm")} confirmLabel={t("observation.deleteAll")} onConfirm={() => action.mutate(null)}><Trash2 data-icon="inline-start" />{t("observation.deleteAll")}</ConfirmButton>
  </>
  const toolbar = active ? <div role="group" aria-label={t("management.selectPage")} className="inline-flex w-fit max-w-full items-center gap-1 rounded-xl border bg-muted/30 p-1">
    <Checkbox aria-label={t("management.selectPage")} title={t("management.selectPage")} className="mx-1" checked={ids.length > 0 && ids.every(id => selected.includes(id)) ? true : selected.length ? "indeterminate" : false} disabled={action.isPending || !ids.length} onCheckedChange={checked => select(checked ? ids : [])} />
    <span className="px-1 text-sm text-muted-foreground">{t("management.selected", { count: selected.length })}</span>
    <Separator orientation="vertical" className="mx-1 self-stretch data-[orientation=vertical]:h-auto" />
    <ConfirmButton iconOnly variant="destructive" disabled={!selected.length || action.isPending} title={t("management.deleteSelected", { count: selected.length })} onConfirm={() => action.mutate(selected)}><Trash2 /></ConfirmButton>
  </div> : null
  const column: Array<Column<T>> = active ? [{ key: "selection", header: t("management.select"), cell: checkbox }] : []
  return { actions, toolbar, column }
}
