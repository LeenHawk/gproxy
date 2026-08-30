import { useMemo, useState } from "react"
import { useTranslation } from "react-i18next"
import { saveModelAlias } from "@/api/control"
import type { ModelAliasDto } from "@/generated/ModelAliasDto"
import type { ModelAliasWriteRequest } from "@/generated/ModelAliasWriteRequest"
import type { RouteDto } from "@/generated/RouteDto"
import { DataTable, type DataTableColumn } from "@/components/data-table"
import { BatchActions } from "@/components/batch-actions"
import { EntityDeleteButton } from "@/components/entity-delete-button"
import { Button } from "@/components/ui/button"
import { Card, CardAction, CardContent, CardHeader, CardTitle } from "@/components/ui/card"
import { EnabledSwitch } from "@/components/routes/enabled-switch"
import { ModelAliasForm } from "@/components/routes/model-alias-form"

export function ModelAliases({
  aliases,
  routes,
  onChanged,
}: {
  aliases: Array<ModelAliasDto>
  routes: Array<RouteDto>
  onChanged: () => void
}) {
  const { t } = useTranslation()
  const [form, setForm] = useState<{ alias: ModelAliasDto | null; opener: HTMLElement } | null>(null)
  const routeById = useMemo(() => new Map(routes.map((route) => [route.id, route])), [routes])
  const ordered = useMemo(() => [...aliases].sort((a, b) => a.name.localeCompare(b.name)), [aliases])

  function openForm(value: ModelAliasDto | null, element: HTMLElement) {
    setForm({ alias: value, opener: element })
  }
  const write = (alias: ModelAliasDto, enabled: boolean): ModelAliasWriteRequest => ({
    name: alias.name,
    route_id: alias.route_id,
    enabled,
  })
  const actions = (alias: ModelAliasDto) => <div className="flex items-center justify-end gap-2" onClick={(event) => event.stopPropagation()}>
    <EnabledSwitch checked={alias.enabled} label={`${alias.name}: ${t("routes.aliases.enabled")}`} errorMessage={t("routes.aliases.saveError")} onChange={(enabled) => saveModelAlias(write(alias, enabled), alias.id)} onChanged={onChanged} />
    <Button size="sm" variant="outline" aria-label={`${t("common.actions.edit")}: ${alias.name}`} onClick={(event) => openForm(alias, event.currentTarget)}>{t("common.actions.edit")}</Button>
    <EntityDeleteButton entity="model-aliases" id={alias.id} label={alias.name} queryKeys={["model-aliases"]} />
  </div>
  const columns: Array<DataTableColumn<ModelAliasDto>> = [
    { key: "name", label: t("routes.aliases.name"), header: t("routes.aliases.name"), cell: (alias) => <span className="font-mono text-xs">{alias.name}</span> },
    { key: "route", label: t("routes.aliases.route"), header: t("routes.aliases.route"), cell: (alias) => routeById.get(alias.route_id)?.name ?? alias.route_id },
    { key: "enabled", label: t("routes.aliases.enabled"), header: t("routes.aliases.enabled"), cell: (alias) => t(`common.status.${alias.enabled ? "enabled" : "disabled"}`) },
    { key: "actions", label: t("common.actions.edit"), header: <span className="sr-only">{t("common.actions.edit")}</span>, cell: actions },
  ]

  return (
    <Card>
      <CardHeader>
        <CardTitle>{t("routes.aliases.title")}</CardTitle>
        <CardAction>
          <Button size="sm" disabled={routes.length === 0} onClick={(event) => openForm(null, event.currentTarget)}>
            {t("routes.aliases.add")}
          </Button>
        </CardAction>
      </CardHeader>
      <CardContent>
        <DataTable columns={columns} rows={ordered} rowKey={(alias) => alias.id} searchText={(alias) => `${alias.name} ${routeById.get(alias.route_id)?.name ?? alias.route_id}`} renderCard={(alias) => <div className="flex flex-col gap-3"><div><p className="font-mono text-xs">{alias.name}</p><p className="text-xs text-muted-foreground">{routeById.get(alias.route_id)?.name ?? alias.route_id}</p></div>{actions(alias)}</div>} empty={t("routes.aliases.empty")} storageKey="model-aliases" selectable batchActions={(rows) => <BatchActions entity="model-aliases" rows={rows} queryKeys={["model-aliases"]} />} />
      </CardContent>
      {form ? (
        <ModelAliasForm
          key={form.alias?.id ?? "new"}
          alias={form.alias}
          routes={routes}
          opener={form.opener}
          onOpenChange={(open) => { if (!open) setForm(null) }}
          onChanged={onChanged}
        />
      ) : null}
    </Card>
  )
}
