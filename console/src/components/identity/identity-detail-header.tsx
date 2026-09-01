import { useTranslation } from "react-i18next"
import type { Entity } from "@/generated/Entity"
import { EntityDeleteButton } from "@/components/entity-delete-button"
import { Badge } from "@/components/ui/badge"

export function IdentityDetailHeader({ title, description, enabled, entity, id, queryKeys, onDeleted }: {
  title: string
  description: string
  enabled: boolean
  entity: Entity
  id: number
  queryKeys: Array<string>
  onDeleted: () => void
}) {
  const { t } = useTranslation()
  return (
    <header className="flex flex-wrap items-start justify-between gap-3">
      <div><h2 className="text-xl font-semibold">{title}</h2><p className="mt-1 text-sm text-muted-foreground">{description}</p></div>
      <div className="flex items-center gap-2"><Badge variant={enabled ? "success" : "outline"}>{t(`common.status.${enabled ? "enabled" : "disabled"}`)}</Badge><EntityDeleteButton entity={entity} id={id} label={title} queryKeys={queryKeys} onDeleted={onDeleted} /></div>
    </header>
  )
}
