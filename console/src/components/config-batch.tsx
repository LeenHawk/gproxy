import { useState } from "react"
import { useMutation, useQueryClient } from "@tanstack/react-query"
import { useTranslation } from "react-i18next"
import { toast } from "sonner"
import { invalidateConfiguration } from "@/api/invalidation"
import type { Family } from "@/api/admin"
import type { ConfigFamily } from "@/api/config-family"
import { Checkbox } from "@/components/ui/checkbox"
import { Button } from "@/components/ui/button"
import { ConfirmButton } from "@/components/confirm"
export function useConfigBatch<D, W, P>({ family, context, rows, onSaved, afterDelete, enableToggle = true, deletable = true }: { family: Family<D, W, P> & Partial<Pick<ConfigFamily<D, W, P>, "batch">>; enableToggle?: boolean; deletable?: boolean; context: string; rows: { id: string }[]; onSaved?: () => Promise<unknown>; afterDelete?: (ids: string[]) => Promise<unknown> }) {
  const { t } = useTranslation()
  const client = useQueryClient()
  const [selection, setSelection] = useState<{ context: string; ids: string[] }>({ context, ids: [] })
  const selected = selection.context === context ? selection.ids : []
  const select = (ids: string[]) => setSelection({ context, ids })
  const action = useMutation({ mutationFn: ({ kind, ids }: { kind: "enable" | "disable" | "delete"; ids: string[] }) => family.batch!(ids.map(id => kind === "delete" ? { delete: id } : { update: { id, patch: { enabled: kind === "enable" } as P } })),
    onSuccess: async (_, { kind, ids }) => {
      select([])
      await invalidateConfiguration(client, family.path)
      if (kind === "delete" && afterDelete) {
        try { await afterDelete(ids) } catch (error) { toast.error(t("management.cleanupFailed", { message: error instanceof Error ? error.message : String(error) })) }
      }
      await onSaved?.()
    }, onError: (error: Error) => toast.error(error.message) })
  const checkbox = (id: string, name: string) => <Checkbox aria-label={`${t("management.select")}: ${name}`} checked={selected.includes(id)} disabled={action.isPending} onClick={event => event.stopPropagation()} onCheckedChange={checked => select(checked ? [...selected, id] : selected.filter(value => value !== id))} />
  const toolbar = family.batch ? <div className="flex flex-wrap items-center gap-2"><Button variant="outline" size="sm" disabled={action.isPending} onClick={() => select(selected.length ? [] : rows.map(row => row.id))}>{t("management.selectPage")}</Button><span className="text-sm">{t("management.selected", { count: selected.length })}</span>{enableToggle ? (["enable", "disable"] as const).map(kind => <Button key={kind} size="sm" variant="outline" disabled={!selected.length || action.isPending} onClick={() => action.mutate({ kind, ids: selected })}>{t(`management.${kind}`)}</Button>) : null}{deletable ? <ConfirmButton disabled={!selected.length || action.isPending} title={t("management.deleteSelected", { count: selected.length })} onConfirm={() => action.mutate({ kind: "delete", ids: selected })}>{t("actions.delete")}</ConfirmButton> : null}</div> : null
  return { toolbar, checkbox, pending: action.isPending }
}
