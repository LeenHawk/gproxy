import { useMemo, useState } from "react"
import { PlusIcon } from "lucide-react"
import { useTranslation } from "react-i18next"
import { saveModelAlias } from "@/api/control"
import type { ModelAliasDto } from "@/generated/ModelAliasDto"
import type { RouteDto } from "@/generated/RouteDto"
import { Button } from "@/components/ui/button"
import { Card, CardAction, CardContent, CardHeader, CardTitle } from "@/components/ui/card"
import { Empty, EmptyContent, EmptyHeader, EmptyTitle } from "@/components/ui/empty"
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from "@/components/ui/table"
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

  return (
    <Card>
      <CardHeader>
        <CardTitle>{t("routes.aliases.title")}</CardTitle>
        <CardAction>
          <Button size="sm" disabled={routes.length === 0} onClick={(event) => openForm(null, event.currentTarget)}>
            <PlusIcon data-icon="inline-start" />
            {t("routes.aliases.add")}
          </Button>
        </CardAction>
      </CardHeader>
      <CardContent>
        {ordered.length === 0 ? (
          <Empty>
            <EmptyHeader><EmptyTitle>{t("routes.aliases.empty")}</EmptyTitle></EmptyHeader>
            <EmptyContent>
              <Button disabled={routes.length === 0} onClick={(event) => openForm(null, event.currentTarget)}>{t("routes.aliases.add")}</Button>
            </EmptyContent>
          </Empty>
        ) : (
          <Table>
            <TableHeader><TableRow>
              <TableHead>{t("routes.aliases.name")}</TableHead>
              <TableHead>{t("routes.aliases.route")}</TableHead>
              <TableHead>{t("routes.aliases.enabled")}</TableHead>
              <TableHead><span className="sr-only">{t("common.actions.edit")}</span></TableHead>
            </TableRow></TableHeader>
            <TableBody>{ordered.map((alias) => (
              <TableRow key={alias.id}>
                <TableCell className="font-mono text-xs">{alias.name}</TableCell>
                <TableCell>{routeById.get(alias.route_id)?.name ?? alias.route_id}</TableCell>
                <TableCell>
                  <EnabledSwitch
                    checked={alias.enabled}
                    label={`${alias.name}: ${t("routes.aliases.enabled")}`}
                    errorMessage={t("routes.aliases.saveError")}
                    onChange={(enabled) => saveModelAlias({
                      name: alias.name,
                      route_id: alias.route_id,
                      enabled,
                    }, alias.id)}
                    onChanged={onChanged}
                  />
                </TableCell>
                <TableCell className="text-right">
                  <Button size="sm" variant="outline" aria-label={`${t("common.actions.edit")}: ${alias.name}`} onClick={(event) => openForm(alias, event.currentTarget)}>{t("common.actions.edit")}</Button>
                </TableCell>
              </TableRow>
            ))}</TableBody>
          </Table>
        )}
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
