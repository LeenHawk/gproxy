import { useConfigBatch } from "@/components/config-batch"
import { CredentialPicker } from "@/components/credential-picker"
import { usePagination } from "@/lib/use-pagination"
import { useState } from "react"
import { BadgeDollarSign, Download, Pencil, Play, Plus, Search, Trash2 } from "lucide-react"
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query"
import { useTranslation } from "react-i18next"
import { toast } from "sonner"
import { providerModels } from "@/api/configuration"
import { directory, priceRules, testModel } from "@/api/models"
import { ApiError } from "@/api/client"
import type { ProviderDto, ProviderModelDto } from "@/generated/sdk"
import { object } from "@/components/providers/provider-model-state"
import { readVariants, saveVariantRules, variantRules, variantSetId } from "@/components/providers/provider-model-variant-rules"
import { ruleSets } from "@/api/routing-rules"
import { DataTable, Pagination } from "@/components/data-table"
import { ConfirmButton } from "@/components/confirm"
import { EmptyNotice, ErrorNotice, QueryState } from "@/components/state"
import { Button } from "@/components/ui/button"
import { InputGroup, InputGroupAddon, InputGroupInput } from "@/components/ui/input-group"
import { ModelCapabilities, ModelIdentity, ModelLimits, ModelSummaryCard } from "@/components/providers/model-summary"
import { Switch } from "@/components/ui/switch"
import { Badge } from "@/components/ui/badge"
import { ProviderModelDialog, type ModelSave } from "./model-form"
import { ModelImportDialog } from "./model-import"
import { ModelPricingDialog } from "./model-pricing"
export function ProviderModels({ provider }: { provider: ProviderDto }) {
  const { t } = useTranslation(), client = useQueryClient()
  const key = ["admin", "/provider-models", provider.id]
  const list = useQuery({ queryKey: key, queryFn: () => directory(providerModels, { providerId: provider.id }) })
  const prices = useQuery({ queryKey: ["admin", "/price-rules", provider.id], queryFn: () => directory(priceRules, { providerId: provider.id }) })
  const [editing, setEditing] = useState<{ id: string; row?: ProviderModelDto } | null>(null)
  const [pricing, setPricing] = useState<ProviderModelDto | null>(null)
  const [importing, setImporting] = useState<"models" | "metadata" | null>(null)
  const [credentialId, setCredentialId] = useState<string | null>(null)
  const [search, setSearch] = useState("")
  const { page, pageSize, setPage, setPageSize } = usePagination()
  const behavior = useQuery({ queryKey: ["model-variant-rules", editing?.id], queryFn: () => variantRules(editing!.id), enabled: !!editing })
  const refresh = () => Promise.all([client.invalidateQueries({ queryKey: ["admin", "/provider-models"] }), client.invalidateQueries({ queryKey: ["admin", "/price-rules"] }), client.invalidateQueries({ queryKey: ["admin", "/rules"] }), client.invalidateQueries({ queryKey: ["admin", "/rule-sets"] }), client.invalidateQueries({ queryKey: ["model-variant-rules"] })])
  const save = useMutation({ mutationFn: async (value: ModelSave) => {
    const { variants, ...body } = value, id = editing!.id
    let exists = !!editing!.row
    if (!exists) { try { await providerModels.get(id); exists = true } catch (e) { if (!(e instanceof ApiError && e.status === 404)) throw e } }
    if (exists) await providerModels.update(id, body)
    else await providerModels.create({ id, providerId: provider.id, ...body })
    await saveVariantRules(provider.id, provider.name, id, body.upstreamName, variants)
  }, onSuccess: async () => { setEditing(null); await refresh(); toast.success(t("toast.saved")) }, onError: () => { void refresh() } })
  const toggle = useMutation({ mutationFn: ({ id, enabled }: { id: string; enabled: boolean }) => providerModels.update(id, { enabled }), onSuccess: refresh })
  const cleanupVariants = async (ids: string[]) => {
    for (const id of ids) try { await ruleSets.remove(variantSetId(id)) } catch (error) { if (!(error instanceof ApiError && error.status === 404)) throw error }
  }
  const remove = useMutation({ mutationFn: async (id: string) => { await providerModels.remove(id); await cleanupVariants([id]) }, onSuccess: refresh })
  const probe = useMutation({ mutationFn: (model: string) => testModel(provider.id, model, credentialId) })
  const rows = (list.data ?? []).filter(r => `${r.upstreamName} ${object(r.metadata).display_name ?? ""}`.toLowerCase().includes(search.toLowerCase()))
  const visiblePage = Math.min(page, Math.max(1, Math.ceil(rows.length / pageSize)))
  const batch = useConfigBatch({ family: providerModels, context: JSON.stringify([provider.id, search, visiblePage, pageSize]), rows: rows.slice((visiblePage - 1) * pageSize, visiblePage * pageSize), onSaved: refresh, afterDelete: cleanupVariants })
  const priced = (row: ProviderModelDto) => prices.data?.some(price => price.modelPattern === row.upstreamName && price.enabled)
  const enabled = (row: ProviderModelDto) => <div className="flex items-center gap-2">{batch.checkbox(row.id, row.upstreamName)}<Switch aria-label={`${t("fields.enabled")}: ${row.upstreamName}`} checked={row.enabled} disabled={toggle.isPending} onCheckedChange={value => toggle.mutate({ id: row.id, enabled: value })} /></div>
  const actions = (row: ProviderModelDto) => <>
    <Button size="icon-sm" variant="ghost" aria-label={`${t("providers.models.test")}: ${row.upstreamName}`} title={t("providers.models.test")} disabled={probe.isPending} onClick={() => probe.mutate(row.upstreamName)}><Play /></Button>
    <Button size="icon-sm" variant="ghost" aria-label={`${t("providers.models.pricing")}: ${row.upstreamName}`} title={t("providers.models.pricing")} onClick={() => setPricing(row)}><BadgeDollarSign /></Button>
    <Button size="icon-sm" variant="ghost" aria-label={`${t("actions.edit")}: ${row.upstreamName}`} title={t("actions.edit")} onClick={() => { save.reset(); setEditing({ id: row.id, row }) }}><Pencil /></Button>
    <ConfirmButton iconOnly title={t("confirm.deleteTitle", { name: row.upstreamName })} disabled={remove.isPending} onConfirm={() => remove.mutate(row.id)}><Trash2 /></ConfirmButton>
  </>
  return <div className="flex flex-col gap-4">
    {batch.toolbar}
    <CredentialPicker providerId={provider.id} value={credentialId} onChange={setCredentialId} disabled={probe.isPending} />
    <div className="flex flex-col gap-3 sm:flex-row sm:flex-wrap sm:items-center sm:justify-between"><div className="flex w-full min-w-0 items-center gap-3 sm:w-auto sm:min-w-64 sm:flex-1"><InputGroup className="w-full sm:max-w-xs"><InputGroupAddon><Search /></InputGroupAddon><InputGroupInput aria-label={t("actions.search")} placeholder={t("actions.search")} value={search} onChange={e => { setSearch(e.target.value); setPage(1) }} /></InputGroup><span className="shrink-0 text-sm">{t("catalog.count", { count: rows.length })}</span></div><div className="flex flex-wrap items-center gap-2"><Button variant="outline" size="sm" onClick={() => setImporting("models")}><Download data-icon="inline-start" />{t("providers.models.pull")}</Button><Button variant="outline" size="sm" onClick={() => setImporting("metadata")}><Download data-icon="inline-start" />{t("modelUI.defaultMetadata")}</Button>{batch.trigger}<Button size="sm" onClick={() => { save.reset(); setEditing({ id: crypto.randomUUID() }) }}><Plus data-icon="inline-start" />{t("actions.new")}</Button></div></div>
    {toggle.error || remove.error || probe.error ? <ErrorNotice error={toggle.error || remove.error || probe.error} /> : null}
    {probe.data ? <div className="rounded-lg border p-3 text-sm"><p>{probe.data.ok ? t("modelUI.testOk") : probe.data.error} · {probe.data.latencyMs} ms · {probe.data.credentialLabel ?? "—"}</p>{probe.data.reply ? <p className="whitespace-pre-wrap">{probe.data.reply}</p> : null}</div> : null}
    <QueryState isPending={list.isPending || prices.isPending} error={list.error || prices.error}><DataTable paginate={false} rows={rows.slice((visiblePage - 1) * pageSize, visiblePage * pageSize)} rowKey={r => r.id} empty={<EmptyNotice title={t("state.emptyTitle")} />} columns={[
      { key: "upstreamName", className: "max-w-52", cell: r => <ModelIdentity name={r.upstreamName} /> },
      { key: "limits", header: t("catalog.limits"), cell: r => <ModelLimits metadata={object(r.metadata)} /> },
      { key: "capabilities", className: "max-w-36", header: t("catalog.capabilities"), cell: r => <div className="flex flex-col gap-1.5"><ModelCapabilities metadata={object(r.metadata)} />{Array.isArray(object(r.metadata).variants) && (object(r.metadata).variants as unknown[]).length > 0 ? <Badge variant="outline">{t("providers.models.variants")} +{(object(r.metadata).variants as unknown[]).length}</Badge> : null}</div> },
      { key: "price", header: t("providers.models.pricing"), cell: r => priced(r) ? <Badge variant="secondary">{t("providers.models.priced")}</Badge> : <span className="text-muted-foreground">—</span> },
      { key: "enabled", cell: enabled },
    ]} actions={actions} renderCard={r => <ModelSummaryCard name={r.upstreamName} metadata={object(r.metadata)} control={enabled(r)} actions={actions(r)}>{priced(r) ? <Badge variant="secondary">{t("providers.models.priced")}</Badge> : null}</ModelSummaryCard>} /><Pagination page={visiblePage} pageSize={pageSize} total={rows.length} onPage={setPage} onPageSize={setPageSize} /></QueryState>
    {editing ? <QueryState isPending={behavior.isPending} error={behavior.error}>{behavior.data ? <ProviderModelDialog key={editing.id} model={editing.row} channel={provider.channel} variants={readVariants(object(editing.row?.metadata), behavior.data)} onClose={() => setEditing(null)} onSave={value => save.mutate(value)} pending={save.isPending} error={save.error} /> : null}</QueryState> : null}
    {importing ? <ModelImportDialog mode={importing} providerId={provider.id} models={list.data ?? []} onClose={() => setImporting(null)} onSaved={refresh} /> : null}
    {pricing ? <ModelPricingDialog providerId={provider.id} model={pricing.upstreamName} onClose={() => setPricing(null)} /> : null}
  </div>
}
