import { useState } from "react"
import { useMutation, useQueries, useQuery, useQueryClient } from "@tanstack/react-query"
import { useTranslation } from "react-i18next"
import { bindings, providerDefaultSetId, ruleSets, endpoints, effectiveRouting, saveRoutingMapping, resetRoutingMapping, applyDefaultRouting } from "@/api/routing-rules"
import { optionSource } from "@/api/options"
import { RulesEditor } from "@/pages/rules/editor"
import { CollectionPage } from "@/pages/identity/collection"
import { BoolCell } from "@/components/cells"
import { Switch } from "@/components/ui/switch"
import { Input } from "@/components/ui/input"
import { Button } from "@/components/ui/button"
import { Badge } from "@/components/ui/badge"
import { usePagination } from "@/lib/use-pagination"
import { DataTable, Pagination } from "@/components/data-table"
import { ConfirmButton } from "@/components/confirm"
import { toast } from "sonner"
import type { OperationRoutingDto, RoutingMappingWrite } from "@/generated/sdk"
import { EmptyNotice, ErrorNotice, QueryState } from "@/components/state"
import { operationChoices } from "@/pages/providers/operation-options"
import { dialects } from "@/pages/providers/config-schema"
import { RoutingRuleDialog } from "@/pages/providers/routing-rule-dialog"

export function ProviderRules({ providerId, providerName }: { providerId: string; providerName: string }) {
  const { t } = useTranslation()
  const { page, pageSize, setPage, setPageSize } = usePagination("provider-rule-sets", providerId)
  const request = { providerId, page, pageSize }
  const attached = useQuery({ queryKey: ["admin", "/provider-rule-sets", request], queryFn: () => bindings.list(request) })
  const rows = attached.data?.items ?? []
  const currentPage = Math.min(page, Math.max(1, Math.ceil((attached.data?.total ?? 0) / pageSize)))
  if (attached.data && currentPage !== page) setPage(currentPage)
  const sets = useQueries({ queries: rows.map(binding => ({ queryKey: ["admin", "/rule-sets", "detail", binding.ruleSetId], queryFn: () => ruleSets.get(binding.ruleSetId) })) })
  const available = sets.flatMap(set => set.data ? [set.data] : [])
  return <QueryState isPending={sets.some(set => set.isPending) || attached.isPending} error={sets.find(set => set.error)?.error ?? attached.error}>
    <div className="flex flex-col gap-6">
      <RulesEditor key={providerId} sets={available} attachments={rows} providerId={providerId} providerName={providerName} defaultSetId={providerDefaultSetId(providerId)} />
      <Pagination page={currentPage} pageSize={pageSize} total={attached.data?.total ?? 0} onPage={setPage} onPageSize={setPageSize} />
      <details><summary className="cursor-pointer text-sm text-muted-foreground">{t("rules.advancedBindings")}</summary>
        <div className="mt-4">
          <CollectionPage embedded id="provider-rule-sets" createLabel={t("rules.attach")} family={bindings} filter={{ providerId }} create={(body) => bindings.create({ ...body, providerId })} rowId={(r) => r.id} rowLabel={(r) => available.find((s) => s.id === r.ruleSetId)?.name ?? r.ruleSetId}
            columns={[{ key: "ruleSetId", cell: (r) => <RuleSetName id={r.ruleSetId} /> }, { key: "sortOrder", cell: (r) => r.sortOrder }, { key: "enabled", cell: (r) => <BoolCell value={r.enabled} /> }]}
            fields={[{ name: "ruleSetId", kind: "searchable", required: true, createOnly: true, source: optionSource(ruleSets, row => ({ value: row.id, label: row.name })) }, { name: "sortOrder", kind: "number" }, { name: "enabled", kind: "switch" }]}
          />
        </div>
      </details>
    </div>
  </QueryState>
}

export function ProviderOperations({ providerId }: { providerId: string }) {
  const { t } = useTranslation()
  const client = useQueryClient()
  const [editing, setEditing] = useState<OperationRoutingDto | "new" | null>(null)
  const list = useQuery({ queryKey: ["admin", "/providers", "routing", providerId], queryFn: () => effectiveRouting(providerId) })
  const current = editing && editing !== "new" ? editing : undefined
  const [showUnsupported, setShowUnsupported] = useState(false)
  const [search, setSearch] = useState("")
  const rows = (list.data ?? []).filter((r) => (showUnsupported || r.mapping.implementation !== "unsupported" || r.custom) && `${t(`operation.${r.operation}`, { defaultValue: r.operation })} ${t(`rules.protocols.${r.dialect}`)}`.toLowerCase().includes(search.toLowerCase()))
  const invalidate = () => client.invalidateQueries({ queryKey: ["admin", "/providers", "routing", providerId] })
  const saved = useMutation({
    mutationFn: ({ row, write }: { row: OperationRoutingDto; write: RoutingMappingWrite }) => saveRoutingMapping(providerId, row.operation, row.dialect, write),
    onSuccess: async () => { setEditing(null); await invalidate(); toast.success(t("toast.saved")) },
  })
  const reset = useMutation({ mutationFn: (row: OperationRoutingDto) => resetRoutingMapping(providerId, row.operation, row.dialect), onSuccess: async () => { await invalidate(); toast.success(t("toast.saved")) } })
  const defaults = useMutation({
    mutationFn: () => applyDefaultRouting(providerId),
    onSuccess: async () => { await Promise.all([invalidate(), client.invalidateQueries({ queryKey: ["admin", "/operation-endpoints"] })]); toast.success(t("toast.saved")) },
  })
  const pending = reset.isPending || defaults.isPending || saved.isPending
  return <>
    <div className="mb-4 flex flex-wrap items-center gap-3"><Input className="max-w-xs" aria-label={t("actions.search")} placeholder={t("actions.search")} value={search} onChange={(e) => setSearch(e.target.value)} /><label className="flex items-center gap-2 text-sm"><Switch checked={showUnsupported} onCheckedChange={setShowUnsupported} />{t("rules.showUnsupported")}</label><Button size="sm" disabled={pending || !list.data} onClick={() => { saved.reset(); setEditing("new") }}>{t("create.operation-rules")}</Button><Button variant="outline" size="sm" disabled={pending || !list.data} onClick={() => defaults.mutate()}>{t("rules.applyDefaults")}</Button></div>
    {reset.error || defaults.error ? <ErrorNotice error={reset.error || defaults.error} /> : null}
    <QueryState isPending={list.isPending} error={list.error}>
      <DataTable rows={rows} rowKey={(r) => `${r.operation}:${r.dialect}`} empty={<EmptyNotice title={t("rules.noRoutes")} />}
        columns={[
          { key: "operation", cell: (r) => t(`operation.${r.operation}`, { defaultValue: r.operation }) },
          { key: "dialect", header: t("rules.incoming"), cell: (r) => t(`rules.protocols.${r.dialect}`) },
          { key: "implementation", header: t("rules.behavior"), cell: (r) => <span className="inline-flex flex-wrap items-center gap-2"><Badge variant={r.mapping.implementation === "unsupported" ? "destructive" : "secondary"}>{t(`rules.implementations.${r.mapping.implementation}`)}</Badge>{r.mapping.target ? <span>{t(`operation.${r.mapping.target.operation}`, { defaultValue: r.mapping.target.operation })} · {t(`rules.protocols.${r.mapping.target.dialect}`)}</span> : null}</span> },
        ]}
        actions={(row) => <>
          <Button variant="ghost" size="sm" disabled={pending} onClick={() => { saved.reset(); setEditing(row) }}>{t("actions.edit")}</Button>
          {row.custom ? <ConfirmButton disabled={pending} title={t("rules.resetConfirm")} confirmLabel={t("rules.reset")} onConfirm={() => reset.mutate(row)}>{t("rules.reset")}</ConfirmButton> : null}
        </>}
      />
    </QueryState>
    <RoutingRuleDialog open={editing !== null} onOpenChange={(open) => { if (!open && !saved.isPending) setEditing(null) }} row={current} rows={list.data ?? []} onSubmit={(row, write) => saved.mutate({ row, write })} pending={saved.isPending} error={saved.error} />
  </>
}
export function ProviderEndpoints({ providerId }: { providerId: string }) {
  const { t } = useTranslation()
  return <CollectionPage paginate={false} embedded id="operation-endpoints" family={endpoints} filter={{ providerId }} create={(body) => endpoints.create({ ...body, providerId })} rowId={(r) => r.id} rowLabel={(r) => r.operation}
    columns={[{ key: "operation", cell: (r) => t(`operation.${r.operation}`, { defaultValue: r.operation }) }, { key: "dialect", cell: (r) => t(`providerOption.${r.dialect}`, { defaultValue: r.dialect }) }, { key: "transport", cell: (r) => r.transport }, { key: "url", cell: (r) => r.url }, { key: "enabled", cell: (r) => <BoolCell value={r.enabled} /> }]}
    fields={[{ name: "operation", kind: "select", choices: operationChoices.map((choice) => ({ ...choice, label: t(`operation.${choice.value}`, { defaultValue: choice.value }) })), required: true }, { name: "dialect", kind: "select", choices: dialects.map((value) => ({ value, label: t(`providerOption.${value}`, { defaultValue: value }) })), required: true }, { name: "transport", kind: "select", options: ["http", "websocket"] }, { name: "url", kind: "text", required: true }, { name: "enabled", kind: "switch" }]}
  />
}

function RuleSetName({ id }: { id: string }) {
  const set = useQuery({ queryKey: ["admin", "/rule-sets", "detail", id], queryFn: () => ruleSets.get(id) })
  return set.data?.name ?? id
}
