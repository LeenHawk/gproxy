import { useState } from "react"
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query"
import { useTranslation } from "react-i18next"
import { bindings, ruleSetDirectory, operationRules, endpoints, effectiveRouting } from "@/api/routing-rules"
import { CollectionPage } from "@/pages/identity/collection"
import { BoolCell } from "@/components/cells"
import { Button } from "@/components/ui/button"
import { Badge } from "@/components/ui/badge"
import { DataTable } from "@/components/data-table"
import { ConfirmButton } from "@/components/confirm"
import { toast } from "sonner"
import type { OperationRoutingDto } from "@/generated/sdk"
import { EmptyNotice, ErrorNotice, QueryState } from "@/components/state"
import { operationChoices } from "@/pages/providers/operation-options"
import { dialects } from "@/pages/providers/config-schema"
import { RoutingRuleDialog } from "@/pages/providers/routing-rule-dialog"

export function ProviderRules({ providerId }: { providerId: string }) {
  const { t } = useTranslation()
  const sets = useQuery({ queryKey: ["admin", "/rule-sets", "directory"], queryFn: ruleSetDirectory })
  return <QueryState isPending={sets.isPending} error={sets.error}>
    <CollectionPage embedded id="provider-rule-sets" createLabel={t("rules.attach")} family={bindings} filter={{ providerId }} create={(body) => bindings.create({ ...body, providerId })} rowId={(r) => r.id} rowLabel={(r) => sets.data?.find((s) => s.id === r.ruleSetId)?.name ?? r.ruleSetId}
      columns={[{ key: "ruleSetId", cell: (r) => sets.data?.find((s) => s.id === r.ruleSetId)?.name ?? r.ruleSetId }, { key: "sortOrder", cell: (r) => r.sortOrder }, { key: "enabled", cell: (r) => <BoolCell value={r.enabled} /> }]}
      fields={[{ name: "ruleSetId", kind: "select", required: true, createOnly: true, choices: sets.data?.map((s) => ({ value: s.id, label: s.name })) ?? [] }, { name: "sortOrder", kind: "number" }, { name: "enabled", kind: "switch" }]}
    />
  </QueryState>
}
export function ProviderOperations({ providerId }: { providerId: string }) {
  const { t } = useTranslation()
  const client = useQueryClient()
  const [editing, setEditing] = useState<OperationRoutingDto | "new" | null>(null)
  const list = useQuery({ queryKey: ["admin", "/providers", "routing", providerId], queryFn: () => effectiveRouting(providerId) })
  const current = editing && editing !== "new" ? editing : undefined
  const invalidate = () => client.invalidateQueries({ queryKey: ["admin", "/providers", "routing", providerId] })
  const saved = useMutation({
    mutationFn: (body: Record<string, unknown>) => current?.rule ? operationRules.update(current.rule.id, body) : operationRules.create({ ...body, providerId, action: "dialects" }),
    onSuccess: async () => { setEditing(null); await invalidate(); toast.success(t("toast.saved")) },
  })
  const reset = useMutation({ mutationFn: (id: string) => operationRules.remove(id), onSuccess: async () => { await invalidate(); toast.success(t("toast.saved")) } })
  return <>
    <div className="mb-4 flex justify-end"><Button size="sm" onClick={() => { saved.reset(); setEditing("new") }}>{t("create.operation-rules")}</Button></div>
    {reset.error ? <ErrorNotice error={reset.error} /> : null}
    <QueryState isPending={list.isPending} error={list.error}>
      <DataTable rows={list.data ?? []} rowKey={(r) => r.operation} empty={<EmptyNotice title={t("rules.noRoutes")} />}
        columns={[
          { key: "operation", cell: (r) => t(`operation.${r.operation}`, { defaultValue: r.operation }) },
          { key: "target", header: t("rules.dialects"), cell: (r) => r.dialects.map((value) => t(`providerOption.${value}`, { defaultValue: value })).join(" · ") || "—" },
          { key: "source", header: t("rules.source"), cell: (r) => <Badge variant={r.rule ? "secondary" : "outline"}>{t(r.rule ? "rules.custom" : "rules.default")}</Badge> },
        ]}
        actions={(row) => <>
          <Button variant="ghost" size="sm" disabled={reset.isPending} onClick={() => { saved.reset(); setEditing(row) }}>{t("actions.edit")}</Button>
          {row.rule ? <ConfirmButton disabled={reset.isPending} title={t("rules.resetConfirm")} confirmLabel={t("rules.reset")} onConfirm={() => reset.mutate(row.rule!.id)}>{t("rules.reset")}</ConfirmButton> : null}
        </>}
      />
    </QueryState>
    <RoutingRuleDialog open={editing !== null} onOpenChange={(open) => { if (!open && !saved.isPending) setEditing(null) }} original={current?.rule ?? undefined} defaults={current ? { operation: current.operation, dialects: current.dialects } : undefined} onSubmit={(body) => saved.mutate(body)} pending={saved.isPending} error={saved.error} />
  </>
}
export function ProviderEndpoints({ providerId }: { providerId: string }) {
  const { t } = useTranslation()
  return <CollectionPage embedded id="operation-endpoints" family={endpoints} filter={{ providerId }} create={(body) => endpoints.create({ ...body, providerId })} rowId={(r) => r.id} rowLabel={(r) => r.operation}
    columns={[{ key: "operation", cell: (r) => t(`operation.${r.operation}`, { defaultValue: r.operation }) }, { key: "dialect", cell: (r) => t(`providerOption.${r.dialect}`, { defaultValue: r.dialect }) }, { key: "transport", cell: (r) => r.transport }, { key: "url", cell: (r) => r.url }, { key: "enabled", cell: (r) => <BoolCell value={r.enabled} /> }]}
    fields={[{ name: "operation", kind: "select", choices: operationChoices.map((choice) => ({ ...choice, label: t(`operation.${choice.value}`, { defaultValue: choice.value }) })), required: true }, { name: "dialect", kind: "select", choices: dialects.map((value) => ({ value, label: t(`providerOption.${value}`, { defaultValue: value }) })), required: true }, { name: "transport", kind: "select", options: ["http", "websocket"] }, { name: "url", kind: "text", required: true }, { name: "enabled", kind: "switch" }]}
  />
}
