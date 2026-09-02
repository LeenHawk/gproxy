import { useMutation, useQueryClient } from "@tanstack/react-query"
import { Trash2Icon } from "lucide-react"
import { useTranslation } from "react-i18next"
import { toast } from "sonner"
import { deleteEntity } from "@/api/control"
import type { Entity } from "@/generated/Entity"
import { Button } from "@/components/ui/button"

export function EntityDeleteButton({ entity, id, label, queryKeys, disabled, onDeleted }: {
  entity: Entity
  id: number
  label: string
  queryKeys: Array<string>
  disabled?: boolean
  onDeleted?: () => void
}) {
  const { t } = useTranslation()
  const client = useQueryClient()
  const mutation = useMutation({
    mutationFn: () => deleteEntity(entity, id),
    onSuccess: async () => {
      await Promise.all(queryKeys.map((key) => client.invalidateQueries({ queryKey: [key] })))
      toast.success(t("common.batch.deleted", { count: 1 }))
      onDeleted?.()
    },
    onError: () => toast.error(t("common.batch.failed")),
  })
  return (
    <Button size="icon-sm" variant="ghost" disabled={disabled || mutation.isPending} onClick={() => mutation.mutate()} aria-label={`${t("common.actions.delete")}: ${label}`}>
      <Trash2Icon aria-hidden />
    </Button>
  )
}
