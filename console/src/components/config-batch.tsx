import { useState } from "react"
import { useMutation, useQueryClient } from "@tanstack/react-query"
import { useTranslation } from "react-i18next"
import { toast } from "sonner"
import { ListChecks, Power, PowerOff, Trash2 } from "lucide-react"
import { Badge } from "@/components/ui/badge"
import { Separator } from "@/components/ui/separator"
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
  const toolbar = family.batch ? (
    <div role="group" aria-label={t("management.selectPage")} className="inline-flex w-fit max-w-full items-center gap-1 rounded-xl border bg-muted/30 p-1">
      <Button
        variant={selected.length ? "secondary" : "ghost"}
        size="sm"
        aria-label={t("management.selectPage")}
        aria-pressed={selected.length > 0}
        disabled={action.isPending || !rows.length}
        onClick={() => select(selected.length ? [] : rows.map(row => row.id))}
      >
        <ListChecks data-icon="inline-start" />
        {t("management.selectPage")}
      </Button>
      <span role="status" aria-live="polite" aria-atomic="true" title={t("management.selected", { count: selected.length })}>
        <span className="sr-only">{t("management.selected", { count: selected.length })}</span>
        <Badge aria-hidden="true" variant={selected.length ? "default" : "secondary"} className="min-w-6 tabular-nums">{selected.length}</Badge>
      </span>
      {enableToggle || deletable ? <Separator orientation="vertical" className="mx-1 self-stretch data-[orientation=vertical]:h-auto" /> : null}
      {enableToggle ? (["enable", "disable"] as const).map(kind => {
        const Icon = kind === "enable" ? Power : PowerOff
        return <Button key={kind} size="icon-sm" variant="ghost" title={t(`management.${kind}`)} aria-label={t(`management.${kind}`)} disabled={!selected.length || action.isPending} onClick={() => action.mutate({ kind, ids: selected })}><Icon /></Button>
      }) : null}
      {deletable ? <ConfirmButton iconOnly variant="destructive" disabled={!selected.length || action.isPending} title={t("management.deleteSelected", { count: selected.length })} onConfirm={() => action.mutate({ kind: "delete", ids: selected })}><Trash2 /></ConfirmButton> : null}
    </div>
  ) : null
  return { toolbar, checkbox, pending: action.isPending }
}
