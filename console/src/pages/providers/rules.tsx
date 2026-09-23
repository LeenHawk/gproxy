import { useQuery } from "@tanstack/react-query"
import { useTranslation } from "react-i18next"
import { bindings, ruleSetDirectory, operationRules, endpoints } from "@/api/routing-rules"
import { CollectionPage } from "@/pages/identity/collection"
import { BoolCell } from "@/components/cells"
import { QueryState } from "@/components/state"
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
  return <>
    <CollectionPage embedded id="operation-rules" family={operationRules} filter={{ providerId }} create={(body) => operationRules.create({ ...body, providerId, action: "dialects" })} rowId={(r) => r.id} rowLabel={(r) => r.operation}
      columns={[{ key: "operation", cell: (r) => t(`operation.${r.operation}`, { defaultValue: r.operation }) }, { key: "target", header: t("rules.dialects"), cell: (r) => Array.isArray(r.target) ? r.target.map((value: string) => t(`providerOption.${value}`, { defaultValue: value })).join(" · ") : JSON.stringify(r.target) }]}
      fields={[]}
      renderForm={(props) => <RoutingRuleDialog {...props} />}
    />
  </>
}
export function ProviderEndpoints({ providerId }: { providerId: string }) {
  const { t } = useTranslation()
  return <CollectionPage embedded id="operation-endpoints" family={endpoints} filter={{ providerId }} create={(body) => endpoints.create({ ...body, providerId })} rowId={(r) => r.id} rowLabel={(r) => r.operation}
    columns={[{ key: "operation", cell: (r) => t(`operation.${r.operation}`, { defaultValue: r.operation }) }, { key: "dialect", cell: (r) => t(`providerOption.${r.dialect}`, { defaultValue: r.dialect }) }, { key: "transport", cell: (r) => r.transport }, { key: "url", cell: (r) => r.url }, { key: "enabled", cell: (r) => <BoolCell value={r.enabled} /> }]}
    fields={[{ name: "operation", kind: "select", choices: operationChoices.map((choice) => ({ ...choice, label: t(`operation.${choice.value}`, { defaultValue: choice.value }) })), required: true }, { name: "dialect", kind: "select", choices: dialects.map((value) => ({ value, label: t(`providerOption.${value}`, { defaultValue: value }) })), required: true }, { name: "transport", kind: "select", options: ["http", "websocket"] }, { name: "url", kind: "text", required: true }, { name: "enabled", kind: "switch" }]}
  />
}
