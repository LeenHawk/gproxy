import { useMemo, useState } from "react"
import { useTranslation } from "react-i18next"
import { saveAlias } from "@/api/control"
import type { AliasDto } from "@/generated/AliasDto"
import type { ProviderDto } from "@/generated/ProviderDto"
import { DataTable, type DataTableColumn } from "@/components/data-table"
import { BatchActions } from "@/components/batch-actions"
import { EntityDeleteButton } from "@/components/entity-delete-button"
import { Button } from "@/components/ui/button"
import { Card, CardAction, CardContent, CardHeader, CardTitle } from "@/components/ui/card"
import { EnabledSwitch } from "@/components/routes/enabled-switch"
import { RoutingAliasForm } from "@/components/routes/routing-alias-form"

export function RoutingAliases({
  aliases,
  providers,
  onChanged,
  scopeProviderId,
}: {
  aliases: Array<AliasDto>
  providers: Array<ProviderDto>
  onChanged: () => void
  scopeProviderId?: number
}) {
  const { t } = useTranslation()
  const [form, setForm] = useState<{ alias: AliasDto | null; opener: HTMLElement } | null>(null)
  const providerById = useMemo(() => new Map(providers.map((provider) => [provider.id, provider])), [providers])
  const ordered = useMemo(
    () => aliases.filter((alias) => scopeProviderId === undefined || alias.provider_id === scopeProviderId).sort((a, b) => a.priority - b.priority || a.id - b.id),
    [aliases, scopeProviderId],
  )

  function openForm(value: AliasDto | null, element: HTMLElement) {
    setForm({ alias: value, opener: element })
  }
  const providerLabel = (alias: AliasDto) => alias.provider_id == null ? t("routes.routingAliases.anyProvider") : providerById.get(alias.provider_id)?.name ?? alias.provider_id
  const actions = (alias: AliasDto) => <div className="flex items-center justify-end gap-2" onClick={(event) => event.stopPropagation()}>
    <EnabledSwitch checked={alias.enabled} label={`${alias.alias}: ${t("routes.routingAliases.enabled")}`} errorMessage={t("routes.routingAliases.saveError")} onChange={(enabled) => saveAlias({ alias: alias.alias, target: alias.target, provider_id: alias.provider_id, priority: alias.priority, enabled }, alias.id)} onChanged={onChanged} />
    <Button size="sm" variant="outline" aria-label={`${t("common.actions.edit")}: ${alias.alias}`} onClick={(event) => openForm(alias, event.currentTarget)}>{t("common.actions.edit")}</Button>
    <EntityDeleteButton entity="aliases" id={alias.id} label={alias.alias} queryKeys={["aliases"]} />
  </div>
  const columns: Array<DataTableColumn<AliasDto>> = [
    { key: "alias", label: t("routes.routingAliases.alias"), header: t("routes.routingAliases.alias"), cell: (alias) => <span className="font-mono text-xs">{alias.alias}</span> },
    { key: "target", label: t("routes.routingAliases.target"), header: t("routes.routingAliases.target"), cell: (alias) => <span className="font-mono text-xs">{alias.target}</span> },
    { key: "provider", label: t("routes.routingAliases.provider"), header: t("routes.routingAliases.provider"), cell: providerLabel },
    { key: "priority", label: t("routes.routingAliases.priority"), header: t("routes.routingAliases.priority"), cell: (alias) => <span className="font-mono text-xs">{alias.priority}</span> },
    { key: "actions", label: t("common.actions.edit"), header: <span className="sr-only">{t("common.actions.edit")}</span>, cell: actions },
  ]

  return (
    <Card>
      <CardHeader>
        <CardTitle>{t(scopeProviderId === undefined ? "routes.routingAliases.title" : "routes.routingAliases.providerTitle")}</CardTitle>
        <CardAction>
          <Button size="sm" onClick={(event) => openForm(null, event.currentTarget)}>
            {t(scopeProviderId === undefined ? "routes.routingAliases.add" : "routes.routingAliases.modelAdd")}
          </Button>
        </CardAction>
      </CardHeader>
      <CardContent>
        <DataTable columns={columns} rows={ordered} rowKey={(alias) => alias.id} searchText={(alias) => `${alias.alias} ${alias.target} ${providerLabel(alias)}`} renderCard={(alias) => <div className="flex flex-col gap-3"><div><p className="font-mono text-xs">{alias.alias} → {alias.target}</p><p className="text-xs text-muted-foreground">{providerLabel(alias)} · {t("routes.routingAliases.priority")}: {alias.priority}</p></div>{actions(alias)}</div>} empty={t(scopeProviderId === undefined ? "routes.routingAliases.empty" : "routes.routingAliases.modelEmpty")} storageKey={scopeProviderId === undefined ? "routing-aliases" : `provider-${scopeProviderId}-aliases`} selectable batchActions={(rows, onApplied) => <BatchActions entity="aliases" rows={rows} queryKeys={["aliases"]} onApplied={onApplied} />} />
      </CardContent>
      {form ? (
        <RoutingAliasForm
          key={form.alias?.id ?? "new"}
          alias={form.alias}
          providers={providers}
          opener={form.opener}
          onOpenChange={(open) => { if (!open) setForm(null) }}
          onChanged={onChanged}
          fixedProviderId={scopeProviderId}
        />
      ) : null}
    </Card>
  )
}
