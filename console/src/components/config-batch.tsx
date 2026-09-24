import { useState } from "react"
import { useMutation, useQueryClient } from "@tanstack/react-query"
import { useTranslation } from "react-i18next"
import { toast } from "sonner"
import { ListChecks, Power, PowerOff, Trash2 } from "lucide-react"
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
  const [batchMode, setBatchMode] = useState(false)
  const active = Boolean(family.batch) && batchMode
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
  const checkbox = (id: string, name: string) => active ? <Checkbox aria-label={`${t("management.select")}: ${name}`} checked={selected.includes(id)} disabled={action.isPending} onClick={event => event.stopPropagation()} onCheckedChange={checked => select(checked ? [...selected, id] : selected.filter(value => value !== id))} /> : null
  const toolbar = family.batch ? (
    <div role="group" aria-label={t("management.selectPage")} className="inline-flex w-fit max-w-full items-center gap-1 rounded-xl border bg-muted/30 p-1">
      <Button
        variant={active ? "secondary" : "ghost"}
        size="sm"
        aria-label={t("management.selectPage")}
        aria-pressed={active}
        disabled={action.isPending}
        onClick={() => { setBatchMode(!active); select([]) }}
      >
        <ListChecks data-icon="inline-start" />
        {t("management.selectPage")}
      </Button>
      {active ? <>
        <Checkbox
          aria-label={t("management.selectPage")}
          title={t("management.selectPage")}
          className="mx-1"
          checked={rows.length > 0 && rows.every(row => selected.includes(row.id)) ? true : selected.length ? "indeterminate" : false}
          disabled={action.isPending || !rows.length}
          onCheckedChange={checked => select(checked ? rows.map(row => row.id) : [])}
        />
        {enableToggle || deletable ? <Separator orientation="vertical" className="mx-1 self-stretch data-[orientation=vertical]:h-auto" /> : null}
        {enableToggle ? (["enable", "disable"] as const).map(kind => {
          const Icon = kind === "enable" ? Power : PowerOff
          return <Button key={kind} size="icon-sm" variant="ghost" title={t(`management.${kind}`)} aria-label={t(`management.${kind}`)} disabled={!selected.length || action.isPending} onClick={() => action.mutate({ kind, ids: selected })}><Icon /></Button>
        }) : null}
        {deletable ? <ConfirmButton iconOnly variant="destructive" disabled={!selected.length || action.isPending} title={t("management.deleteSelected", { count: selected.length })} onConfirm={() => action.mutate({ kind: "delete", ids: selected })}><Trash2 /></ConfirmButton> : null}
      </> : null}
    </div>
  ) : null
  return { toolbar, checkbox, active, pending: action.isPending }
}
