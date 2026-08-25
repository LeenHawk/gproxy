import { useMemo, useState } from "react"
import { PlusIcon } from "lucide-react"
import { useTranslation } from "react-i18next"
import { saveAlias } from "@/api/control"
import type { AliasDto } from "@/generated/AliasDto"
import type { ProviderDto } from "@/generated/ProviderDto"
import { Button } from "@/components/ui/button"
import { Card, CardAction, CardContent, CardHeader, CardTitle } from "@/components/ui/card"
import { Empty, EmptyContent, EmptyHeader, EmptyTitle } from "@/components/ui/empty"
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from "@/components/ui/table"
import { EnabledSwitch } from "@/components/routes/enabled-switch"
import { RoutingAliasForm } from "@/components/routes/routing-alias-form"

export function RoutingAliases({
  aliases,
  providers,
  onChanged,
}: {
  aliases: Array<AliasDto>
  providers: Array<ProviderDto>
  onChanged: () => void
}) {
  const { t } = useTranslation()
  const [form, setForm] = useState<{ alias: AliasDto | null; opener: HTMLElement } | null>(null)
  const providerById = useMemo(() => new Map(providers.map((provider) => [provider.id, provider])), [providers])
  const ordered = useMemo(
    () => [...aliases].sort((a, b) => a.priority - b.priority || a.id - b.id),
    [aliases],
  )

  function openForm(value: AliasDto | null, element: HTMLElement) {
    setForm({ alias: value, opener: element })
  }

  return (
    <Card>
      <CardHeader>
        <CardTitle>{t("routes.routingAliases.title")}</CardTitle>
        <CardAction>
          <Button size="sm" onClick={(event) => openForm(null, event.currentTarget)}>
            <PlusIcon data-icon="inline-start" />
            {t("routes.routingAliases.add")}
          </Button>
        </CardAction>
      </CardHeader>
      <CardContent>
        {ordered.length === 0 ? (
          <Empty>
            <EmptyHeader><EmptyTitle>{t("routes.routingAliases.empty")}</EmptyTitle></EmptyHeader>
            <EmptyContent><Button onClick={(event) => openForm(null, event.currentTarget)}>{t("routes.routingAliases.add")}</Button></EmptyContent>
          </Empty>
        ) : (
          <Table>
            <TableHeader><TableRow>
              <TableHead>{t("routes.routingAliases.alias")}</TableHead>
              <TableHead>{t("routes.routingAliases.target")}</TableHead>
              <TableHead>{t("routes.routingAliases.provider")}</TableHead>
              <TableHead>{t("routes.routingAliases.priority")}</TableHead>
              <TableHead>{t("routes.routingAliases.enabled")}</TableHead>
              <TableHead><span className="sr-only">{t("common.actions.edit")}</span></TableHead>
            </TableRow></TableHeader>
            <TableBody>{ordered.map((alias) => (
              <TableRow key={alias.id}>
                <TableCell className="font-mono text-xs">{alias.alias}</TableCell>
                <TableCell className="font-mono text-xs">{alias.target}</TableCell>
                <TableCell>
                  {alias.provider_id == null
                    ? t("routes.routingAliases.anyProvider")
                    : providerById.get(alias.provider_id)?.name ?? alias.provider_id}
                </TableCell>
                <TableCell className="font-mono tabular-nums">{alias.priority}</TableCell>
                <TableCell>
                  <EnabledSwitch
                    checked={alias.enabled}
                    label={`${alias.alias}: ${t("routes.routingAliases.enabled")}`}
                    errorMessage={t("routes.routingAliases.saveError")}
                    onChange={(enabled) => saveAlias({
                      alias: alias.alias,
                      target: alias.target,
                      provider_id: alias.provider_id,
                      priority: alias.priority,
                      enabled,
                    }, alias.id)}
                    onChanged={onChanged}
                  />
                </TableCell>
                <TableCell className="text-right">
                  <Button size="sm" variant="outline" aria-label={`${t("common.actions.edit")}: ${alias.alias}`} onClick={(event) => openForm(alias, event.currentTarget)}>{t("common.actions.edit")}</Button>
                </TableCell>
              </TableRow>
            ))}</TableBody>
          </Table>
        )}
      </CardContent>
      {form ? (
        <RoutingAliasForm
          key={form.alias?.id ?? "new"}
          alias={form.alias}
          providers={providers}
          opener={form.opener}
          onOpenChange={(open) => { if (!open) setForm(null) }}
          onChanged={onChanged}
        />
      ) : null}
    </Card>
  )
}
