import { useMutation, useQueryClient } from "@tanstack/react-query"
import { PowerIcon, PowerOffIcon, Trash2Icon } from "lucide-react"
import type { ReactNode } from "react"
import { useTranslation } from "react-i18next"
import { toast } from "sonner"
import { batch } from "@/api/control"
import type { BatchActionDto } from "@/generated/BatchActionDto"
import type { Entity } from "@/generated/Entity"
import { Button } from "@/components/ui/button"

export function BatchActions<T extends { id: number }>({
  entity,
  rows,
  queryKeys,
  toggle = true,
  remove = true,
  onApplied,
  size = "sm",
}: {
  entity: Entity
  rows: Array<T>
  queryKeys: Array<string>
  toggle?: boolean
  remove?: boolean
  onApplied?: () => void
  size?: "xs" | "sm"
}) {
  const { t } = useTranslation()
  const client = useQueryClient()
  const mutation = useMutation({
    mutationFn: (action: BatchActionDto) => batch(entity, action, rows.map((row) => row.id)),
    onSuccess: async (result, action) => {
      await Promise.all(queryKeys.map((queryKey) => client.invalidateQueries({ queryKey: [queryKey] })))
      const failures = result.outcomes.filter((outcome) => !outcome.applied)
      if (failures.length) {
        toast.error(t("common.batch.partial", { failed: failures.length, total: result.outcomes.length }), {
          description: failures.map((failure) => `#${failure.id}: ${failure.error ?? t("common.errors.unknown")}`).join("\n"),
        })
      } else {
        toast.success(t(`common.batch.${action}d`, { count: result.outcomes.length }))
      }
      onApplied?.()
    },
    onError: () => toast.error(t("common.batch.failed")),
  })
  const action = (value: BatchActionDto, icon: ReactNode) => (
    <Button
      key={value}
      size={size}
      variant={value === "delete" ? "destructive" : "outline"}
      disabled={mutation.isPending || rows.length === 0}
      onClick={() => mutation.mutate(value)}
      aria-label={t(`common.batch.${value}`)}
    >
      {icon}{t(`common.actions.${value}`)}
    </Button>
  )
  return <>
    {toggle ? action("enable", <PowerIcon data-icon="inline-start" aria-hidden />) : null}
    {toggle ? action("disable", <PowerOffIcon data-icon="inline-start" aria-hidden />) : null}
    {remove ? action("delete", <Trash2Icon data-icon="inline-start" aria-hidden />) : null}
  </>
}
