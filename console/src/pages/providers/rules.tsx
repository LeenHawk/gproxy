import { useQuery } from "@tanstack/react-query"
import { useTranslation } from "react-i18next"
import { bindings, ruleSetDirectory, operationRules, endpoints } from "@/api/routing-rules"
import { CollectionPage } from "@/pages/identity/collection"
import { BoolCell } from "@/components/cells"
import { QueryState } from "@/components/state"
import { Link } from "@/lib/router"
import { operationChoices } from "@/pages/providers/operation-options"
import { dialects } from "@/pages/providers/config-schema"
import { RecordDialog } from "@/components/record-form"

export function ProviderRules({ providerId }: { providerId: string }) {
  const { t } = useTranslation()
  const sets = useQuery({ queryKey: ["admin", "/rule-sets", "directory"], queryFn: ruleSetDirectory })
  return <QueryState isPending={sets.isPending} error={sets.error}>
    <p className="mb-4 text-sm">{t("rules.bindingHelp")} <Link to="/rule-sets" className="underline">{t("nav.rule-sets")}</Link></p>
    <CollectionPage embedded id="provider-rule-sets" family={bindings} filter={{ providerId }} create={(body) => bindings.create({ ...body, providerId })} rowId={(r) => r.id} rowLabel={(r) => sets.data?.find((s) => s.id === r.ruleSetId)?.name ?? r.ruleSetId}
      columns={[{ key: "ruleSetId", cell: (r) => sets.data?.find((s) => s.id === r.ruleSetId)?.name ?? r.ruleSetId }, { key: "sortOrder", cell: (r) => r.sortOrder }, { key: "enabled", cell: (r) => <BoolCell value={r.enabled} /> }]}
      fields={[{ name: "ruleSetId", kind: "select", required: true, createOnly: true, choices: sets.data?.map((s) => ({ value: s.id, label: s.name })) ?? [] }, { name: "sortOrder", kind: "number" }, { name: "enabled", kind: "switch" }]}
    />
  </QueryState>
}
export function ProviderOperations({ providerId }: { providerId: string }) {
  const { t } = useTranslation()
  return <>
    <p className="mb-4 text-sm text-muted-foreground">{t("rules.operationHelp")}</p>
    <CollectionPage embedded id="operation-rules" family={operationRules} filter={{ providerId }} create={(body) => operationRules.create({ ...body, providerId, action: "dialects" })} rowId={(r) => r.id} rowLabel={(r) => r.operation}
      columns={[{ key: "operation", cell: (r) => r.operation }, { key: "target", header: t("rules.dialects"), cell: (r) => Array.isArray(r.target) ? r.target.join(" → ") : JSON.stringify(r.target) }]}
      fields={[]}
      renderForm={({ original, ...props }) => <RecordDialog {...props} mode={original ? "edit" : "create"} original={original ? { ...original } : undefined} title={original ? t("edit.operation-rules") : t("create.operation-rules")} fields={[{ name: "operation", kind: "select", choices: operationChoices, required: true }, { name: "target", label: t("rules.dialects"), kind: "lines", required: true }]} extra={<p className="text-sm text-muted-foreground">{t("rules.operationHelp")}</p>} />}
    />
  </>
}
export function ProviderEndpoints({ providerId }: { providerId: string }) {
  const { t } = useTranslation()
  return <><p className="mb-4 text-sm text-muted-foreground">{t("rules.endpointHelp")}</p><CollectionPage embedded id="operation-endpoints" family={endpoints} filter={{ providerId }} create={(body) => endpoints.create({ ...body, providerId })} rowId={(r) => r.id} rowLabel={(r) => r.operation}
    columns={[{ key: "operation", cell: (r) => r.operation }, { key: "dialect", cell: (r) => r.dialect }, { key: "transport", cell: (r) => r.transport }, { key: "url", cell: (r) => r.url }, { key: "enabled", cell: (r) => <BoolCell value={r.enabled} /> }]}
    fields={[{ name: "operation", kind: "select", choices: operationChoices, required: true }, { name: "dialect", kind: "select", choices: dialects.map((value) => ({ value, label: value })), required: true }, { name: "transport", kind: "select", options: ["http", "websocket"] }, { name: "url", kind: "text", required: true }, { name: "enabled", kind: "switch" }]}
  /></>
}
