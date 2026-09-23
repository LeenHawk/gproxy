import { useState } from "react"
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
import { DataTable } from "@/components/data-table"
import { ConfirmButton } from "@/components/confirm"
import { EmptyNotice, ErrorNotice, QueryState } from "@/components/state"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
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
  const [importing, setImporting] = useState<"models" | "prices" | null>(null)
  const [search, setSearch] = useState("")
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
  const remove = useMutation({ mutationFn: async (id: string) => { await providerModels.remove(id); try { await ruleSets.remove(variantSetId(id)) } catch(e) { if (!(e instanceof ApiError && e.status === 404)) throw e } }, onSuccess: refresh })
  const probe = useMutation({ mutationFn: (model: string) => testModel(provider.id, model) })
  const rows = (list.data ?? []).filter(r => `${r.upstreamName} ${object(r.metadata).display_name ?? ""}`.toLowerCase().includes(search.toLowerCase()))
  return <div className="flex flex-col gap-4">
    <div className="flex flex-wrap items-center gap-2"><Input className="max-w-xs" aria-label={t("actions.search")} placeholder={t("actions.search")} value={search} onChange={e => setSearch(e.target.value)} /><Button variant="outline" size="sm" onClick={() => setImporting("models")}>{t("providers.models.pull")}</Button><Button variant="outline" size="sm" onClick={() => setImporting("prices")}>{t("modelUI.defaultPrices")}</Button><Button size="sm" onClick={() => { save.reset(); setEditing({ id: crypto.randomUUID() }) }}>{t("actions.new")}</Button></div>
    {toggle.error || remove.error || probe.error ? <ErrorNotice error={toggle.error || remove.error || probe.error} /> : null}
    {probe.data ? <div className="rounded-lg border p-3 text-sm"><p>{probe.data.ok ? t("modelUI.testOk") : probe.data.error} · {probe.data.latencyMs} ms · {probe.data.credentialLabel ?? "—"}</p>{probe.data.reply ? <p className="whitespace-pre-wrap">{probe.data.reply}</p> : null}</div> : null}
    <QueryState isPending={list.isPending || prices.isPending} error={list.error || prices.error}><DataTable rows={rows} rowKey={r => r.id} empty={<EmptyNotice title={t("state.emptyTitle")} />} columns={[
      { key: "upstreamName", cell: r => <div><div>{r.upstreamName}</div><div className="text-muted-foreground">{String(object(r.metadata).display_name ?? "")}</div></div> },
      { key: "context", header: t("providers.models.contextWindow"), cell: r => String(object(r.metadata).context_window ?? "—") },
      { key: "output", header: t("providers.models.maxOutput"), cell: r => String(object(r.metadata).max_output_tokens ?? "—") },
      { key: "price", header: t("providers.models.pricing"), cell: r => prices.data?.some(p => p.modelPattern === r.upstreamName && p.enabled) ? <Badge variant="secondary">{t("providers.models.priced")}</Badge> : "—" },
      { key: "variants", header: t("providers.models.variants"), cell: r => Array.isArray(object(r.metadata).variants) ? (object(r.metadata).variants as unknown[]).length : 0 },
      { key: "thinking", header: t("providers.models.thinking"), cell: r => object(r.metadata).thinking_supported == null ? "—" : t(object(r.metadata).thinking_supported ? "values.yes" : "values.no") },
      { key: "enabled", cell: r => <Switch aria-label={`${t("fields.enabled")}: ${r.upstreamName}`} checked={r.enabled} disabled={toggle.isPending} onCheckedChange={enabled => toggle.mutate({ id: r.id, enabled })} /> },
    ]} actions={r => <><Button size="sm" variant="ghost" disabled={probe.isPending} onClick={() => probe.mutate(r.upstreamName)}>{t("providers.models.test")}</Button><Button size="sm" variant="ghost" onClick={() => setPricing(r)}>{t("providers.models.pricing")}</Button><Button size="sm" variant="ghost" onClick={() => { save.reset(); setEditing({ id: r.id, row: r }) }}>{t("actions.edit")}</Button><ConfirmButton title={t("confirm.deleteTitle", { name: r.upstreamName })} disabled={remove.isPending} onConfirm={() => remove.mutate(r.id)}>{t("actions.delete")}</ConfirmButton></>} /></QueryState>
    {editing ? <QueryState isPending={behavior.isPending} error={behavior.error}>{behavior.data ? <ProviderModelDialog key={editing.id} model={editing.row} channel={provider.channel} variants={readVariants(object(editing.row?.metadata), behavior.data)} onClose={() => setEditing(null)} onSave={value => save.mutate(value)} pending={save.isPending} error={save.error} /> : null}</QueryState> : null}
    {importing ? <ModelImportDialog mode={importing} providerId={provider.id} models={list.data ?? []} onClose={() => setImporting(null)} onSaved={refresh} /> : null}
    {pricing ? <ModelPricingDialog providerId={provider.id} model={pricing.upstreamName} onClose={() => setPricing(null)} /> : null}
  </div>
}
