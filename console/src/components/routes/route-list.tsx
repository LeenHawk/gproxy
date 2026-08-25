import { useState } from "react"
import { PlusIcon } from "lucide-react"
import { useTranslation } from "react-i18next"
import { saveRoute } from "@/api/control"
import type { RouteDto } from "@/generated/RouteDto"
import { Button } from "@/components/ui/button"
import { Card, CardAction, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card"
import { Empty, EmptyContent, EmptyHeader, EmptyTitle } from "@/components/ui/empty"
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from "@/components/ui/table"
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
        {routes.length === 0 ? (
          <Empty>
            <EmptyHeader><EmptyTitle>{t("routes.empty")}</EmptyTitle></EmptyHeader>
            <EmptyContent><Button onClick={(event) => openForm(null, event.currentTarget)}>{t("routes.add")}</Button></EmptyContent>
          </Empty>
        ) : (
          <Table>
            <TableHeader>
              <TableRow>
                <TableHead>{t("routes.fields.name")}</TableHead>
                <TableHead>{t("routes.fields.maxAttempts")}</TableHead>
                <TableHead>{t("routes.fields.enabled")}</TableHead>
                <TableHead><span className="sr-only">{t("common.actions.edit")}</span></TableHead>
              </TableRow>
            </TableHeader>
            <TableBody>
              {routes.map((route) => (
                <TableRow key={route.id} data-state={route.id === selectedId ? "selected" : undefined}>
                  <TableCell>
                    <Button
                      size="sm"
                      variant="ghost"
                      aria-pressed={route.id === selectedId}
                      onClick={() => onSelect(route.id)}
                    >
                      {route.name}
                    </Button>
                  </TableCell>
                  <TableCell className="font-mono tabular-nums">{route.max_attempts}</TableCell>
                  <TableCell>
                    <EnabledSwitch
                      checked={route.enabled}
                      label={`${route.name}: ${t("routes.fields.enabled")}`}
                      errorMessage={t("routes.form.updateError")}
                      onChange={(enabled) => saveRoute({
                        name: route.name,
                        max_attempts: route.max_attempts,
                        enabled,
                      }, route.id)}
                      onChanged={onChanged}
                    />
                  </TableCell>
                  <TableCell className="text-right">
                    <Button size="sm" variant="outline" aria-label={`${t("common.actions.edit")}: ${route.name}`} onClick={(event) => openForm(route, event.currentTarget)}>
                      {t("common.actions.edit")}
                    </Button>
                  </TableCell>
                </TableRow>
              ))}
            </TableBody>
          </Table>
        )}
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
