import { useState } from "react"
import { useMutation, useQueryClient } from "@tanstack/react-query"
import { Trash2Icon } from "lucide-react"
import { useTranslation } from "react-i18next"
import { toast } from "sonner"
import { batch } from "@/api/control"
import { ConfirmDangerous } from "@/components/confirm-dangerous"
import { Button } from "@/components/ui/button"

export function UsageDeleteActions({ rows, onApplied, disabled = false }: {
  rows: Array<{ id: number }>
  onApplied: () => void
  disabled?: boolean
}) {
  const { t } = useTranslation()
  const client = useQueryClient()
  const [ids, setIds] = useState<Array<number> | null>(null)
  const remove = useMutation({
    mutationFn: (ids: Array<number>) => batch("usage", "delete", ids),
    onSuccess: async (result) => {
      const failed = result.outcomes.filter((outcome) => !outcome.applied)
      if (failed.length) {
        toast.error(t("common.batch.partial", { failed: failed.length, total: result.outcomes.length }))
      } else {
        toast.success(t("common.batch.deleted", { count: result.outcomes.length }))
      }
      setIds(null)
      onApplied()
      await Promise.all(["usage-records", "usage-summary", "usage", "portal-usage"].map((key) => client.invalidateQueries({ queryKey: [key] })))
    },
    onError: () => toast.error(t("common.batch.failed")),
  })
  return <>
    <Button size="sm" variant="destructive" disabled={disabled || remove.isPending || rows.length === 0} onClick={() => setIds(rows.map((row) => row.id))}>
      <Trash2Icon data-icon="inline-start" aria-hidden />{t("common.actions.delete")}
    </Button>
    <ConfirmDangerous open={ids !== null} onOpenChange={(open) => { if (!open && !remove.isPending) setIds(null) }}
      title={t("usage.delete.title", { count: ids?.length ?? 0 })} description={t("usage.delete.description")}
      confirmLabel={t("common.actions.delete")} pending={remove.isPending} onConfirm={() => { if (ids && !remove.isPending) remove.mutate(ids) }} />
  </>
}
