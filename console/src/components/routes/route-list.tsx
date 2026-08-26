import { useState } from "react"
import { PlusIcon, RouteIcon } from "lucide-react"
import { useTranslation } from "react-i18next"
import { saveRoute } from "@/api/control"
import type { RouteDto } from "@/generated/RouteDto"
import { DataTable, type DataTableColumn } from "@/components/data-table"
import { Button } from "@/components/ui/button"
import { Card, CardAction, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card"
import { EnabledSwitch } from "@/components/routes/enabled-switch"
import { RouteForm } from "@/components/routes/route-form"

export function RouteList({
  routes,
  selectedId,
  onSelect,
  onChanged,
}: {
  routes: Array<RouteDto>
  selectedId: number | null
  onSelect: (id: number) => void
  onChanged: () => void
}) {
  const { t } = useTranslation()
  const [form, setForm] = useState<{ route: RouteDto | null; opener: HTMLElement } | null>(null)

  function openForm(value: RouteDto | null, element: HTMLElement) {
    setForm({ route: value, opener: element })
  }
  const actions = (route: RouteDto) => <div className="flex items-center justify-end gap-2" onClick={(event) => event.stopPropagation()}>
    <EnabledSwitch
      checked={route.enabled}
      label={`${route.name}: ${t("routes.fields.enabled")}`}
      errorMessage={t("routes.form.updateError")}
      onChange={(enabled) => saveRoute({ name: route.name, max_attempts: route.max_attempts, enabled }, route.id)}
      onChanged={onChanged}
    />
    <Button size="sm" variant="outline" aria-label={`${t("common.actions.edit")}: ${route.name}`} onClick={(event) => openForm(route, event.currentTarget)}>{t("common.actions.edit")}</Button>
  </div>
  const columns: Array<DataTableColumn<RouteDto>> = [
    { key: "name", label: t("routes.fields.name"), header: t("routes.fields.name"), cell: (route) => <span className="flex items-center gap-2 font-medium"><RouteIcon aria-hidden />{route.name}</span> },
    { key: "attempts", label: t("routes.fields.maxAttempts"), header: t("routes.fields.maxAttempts"), cell: (route) => <span className="font-mono text-xs">{route.max_attempts}</span> },
    { key: "enabled", label: t("routes.fields.enabled"), header: t("routes.fields.enabled"), cell: (route) => t(`common.status.${route.enabled ? "enabled" : "disabled"}`) },
    { key: "actions", label: t("common.actions.edit"), header: <span className="sr-only">{t("common.actions.edit")}</span>, cell: actions, className: "text-right" },
  ]

  return (
    <Card>
      <CardHeader>
        <CardTitle>{t("routes.title")}</CardTitle>
        <CardDescription>{t("routes.subtitle")}</CardDescription>
        <CardAction>
          <Button size="sm" onClick={(event) => openForm(null, event.currentTarget)}>
            <PlusIcon data-icon="inline-start" />
            {t("routes.add")}
          </Button>
        </CardAction>
      </CardHeader>
      <CardContent>
        <DataTable
          columns={columns}
          rows={routes}
          rowKey={(route) => route.id}
          searchText={(route) => route.name}
          renderCard={(route) => <div className="flex flex-col gap-3"><div className="flex items-center justify-between gap-3"><span className="flex items-center gap-2 font-medium"><RouteIcon aria-hidden />{route.name}</span><span className="font-mono text-xs text-muted-foreground">{t("routes.fields.maxAttempts")}: {route.max_attempts}</span></div>{actions(route)}</div>}
          empty={t("routes.empty")}
          storageKey="routes"
          activeRowKey={selectedId}
          selectable
          onRowClick={(route) => onSelect(route.id)}
        />
      </CardContent>
      {form ? (
        <RouteForm
          key={form.route?.id ?? "new"}
          route={form.route}
          opener={form.opener}
          onOpenChange={(open) => { if (!open) setForm(null) }}
          onChanged={onChanged}
        />
      ) : null}
    </Card>
  )
}
