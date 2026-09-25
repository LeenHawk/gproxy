import { defaultMetadata, matchingModel, providersByModel } from "@/lib/model-catalog"
import { usePagination } from "@/lib/use-pagination"
import { useMemo, useState } from "react"
import { BadgeDollarSign, Info, Pencil, Plus, Search } from "lucide-react"
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query"
import { useTranslation } from "react-i18next"
import { toast } from "sonner"
import { applyDefaultPrices, defaultModels, directory, models, priceRules } from "@/api/models"
import { providers, providerModels, providerPath } from "@/api/configuration"
import { Link } from "@/lib/router"
import type { DefaultModelDto, ModelDto, ProviderDto } from "@/generated/sdk"
import { object } from "@/components/providers/provider-model-state"
import { Page, PageHeader } from "@/components/page"
import { DataTable, Pagination } from "@/components/data-table"
import { ConfirmButton } from "@/components/confirm"
import { RecordDialog, type FormField } from "@/components/record-form"
import { EmptyNotice, ErrorNotice, QueryState } from "@/components/state"
import { Button } from "@/components/ui/button"
import { Badge } from "@/components/ui/badge"
import { InputGroup, InputGroupAddon, InputGroupInput } from "@/components/ui/input-group"
import { ModelCapabilities, ModelIdentity, ModelLimits, ModelSummaryCard } from "@/components/providers/model-summary"
import { Dialog, DialogBody, DialogContent, DialogHeader, DialogTitle } from "@/components/ui/dialog"
import { ModelPricingDialog } from "@/pages/providers/model-pricing"

type Row = { name: string; metadata: Record<string, unknown>; providers?: ProviderDto[]; defaults?: DefaultModelDto; local?: ModelDto }
const metadataFields = ["display_name", "description", "context_window", "max_output_tokens", "input_modalities", "output_modalities", "supported_parameters"] as const
const strings = (value: unknown) => Array.isArray(value) ? value.join(", ") : "—"

function ReferencePrice({ model }: { model?: DefaultModelDto }) {
  const { t } = useTranslation()
  return <div className="flex flex-col gap-1 text-sm"><dl className="grid grid-cols-[auto_auto] gap-x-3 gap-y-1">{["input_tokens", "output_tokens"].map((metric, index) => <div key={metric} className="contents"><dt className="text-muted-foreground">{t(index ? "catalog.tierFields.outputPrice" : "catalog.tierFields.inputPrice")}</dt><dd className="text-right font-mono tabular-nums">{model?.pricing?.rates.find(rate => rate.metric === metric)?.price ?? "—"}</dd></div>)}</dl>{model?.pricing?.tiers?.length ? <Badge variant="outline">{t("catalog.tierCount", { count: model.pricing.tiers.length })}</Badge> : null}</div>
}

function ModelProviders({ providers }: { providers: readonly ProviderDto[] }) {
  return providers.length ? <div className="flex flex-wrap gap-1">{providers.map(provider => <Badge key={provider.id} variant="outline" asChild><Link to={`${providerPath(provider.id)}/models`} className="max-w-48 truncate">{provider.displayName ?? provider.name}</Link></Badge>)}</div> : <span className="text-sm text-muted-foreground">—</span>
}

export function ModelCatalogPage() {
  const { t } = useTranslation(), client = useQueryClient()
  const catalog = useQuery({ queryKey: ["default-model-catalog"], queryFn: defaultModels })
  const local = useQuery({ queryKey: ["admin", "/models"], queryFn: () => directory(models) })
  const instances = useQuery({ queryKey: ["admin", "/providers", "directory"], queryFn: () => directory(providers) })
  const bindings = useQuery({ queryKey: ["admin", "/provider-models", "directory"], queryFn: () => directory(providerModels) })
  const prices = useQuery({ queryKey: ["admin", "/price-rules", "global"], queryFn: () => directory(priceRules) })
  const [search, setSearch] = useState("")
  const { page, pageSize, setPage, setPageSize } = usePagination()
  const [editing, setEditing] = useState<Row | null>(null), [detail, setDetail] = useState<Row | null>(null), [pricing, setPricing] = useState<string | null>(null)
  const rows = useMemo(() => {
    const attached = new Set<string>()
    const defaults: Row[] = (catalog.data?.models ?? []).map(model => {
      const override = matchingModel(local.data ?? [], model.modelId, row => row.name)
      if (override) attached.add(override.id)
      return { name: model.modelId, metadata: { ...defaultMetadata(model), ...object(override?.metadata) }, defaults: model, local: override }
    })
    const custom: Row[] = (local.data ?? []).filter(row => !attached.has(row.id)).map(row => ({ name: row.name, metadata: object(row.metadata), local: row }))
    const combined = [...defaults, ...custom]
    const associations = providersByModel(combined, instances.data ?? [], bindings.data ?? [])
    const needle = search.trim().toLowerCase()
    return combined.map(row => ({ ...row, providers: associations.get(row) ?? [] })).filter(row => `${row.name} ${row.providers.map(provider => `${provider.name} ${provider.displayName ?? ""}`).join(" ")} ${row.metadata.display_name ?? ""} ${strings(row.metadata.input_modalities)} ${strings(row.metadata.output_modalities)} ${strings(row.metadata.supported_parameters)}`.toLowerCase().includes(needle)).sort((a, b) => a.name.localeCompare(b.name))
  }, [catalog.data, local.data, instances.data, bindings.data, search])
  const refresh = () => Promise.all([client.invalidateQueries({ queryKey: ["admin", "/models"] }), client.invalidateQueries({ queryKey: ["discover-models"] })])
  const save = useMutation({ mutationFn: async (body: Record<string, unknown>) => {
    const metadata = { ...object(editing?.local?.metadata) }
    for (const key of metadataFields) if (key in body) metadata[key] = body[key]
    if (editing?.local) return models.update(editing.local.id, { name: String(body.name ?? editing.local.name), metadata })
    return models.create({ name: String(body.name ?? editing?.name ?? ""), metadata })
  }, onSuccess: async () => { await refresh(); setEditing(null); toast.success(t("toast.saved")) } })
  const remove = useMutation({ mutationFn: (id: string) => models.remove(id), onSuccess: refresh })
  const apply = useMutation({ mutationFn: (row: Row) => applyDefaultPrices(null, [row.name]), onSuccess: async (report, row) => {
    await client.invalidateQueries({ queryKey: ["admin", "/price-rules"] })
    toast.success(t("catalog.applied", report))
    setDetail(null)
    setPricing(row.defaults?.pricing?.modelPattern ?? row.name)
  } })
  const fields: FormField[] = [{ name: "name", kind: "text", required: true }, ...metadataFields.map(name => ({ name, label: t(`catalog.${name}`), kind: name.endsWith("modalities") || name === "supported_parameters" ? "lines" as const : name === "context_window" || name === "max_output_tokens" ? "number" as const : "text" as const, nullable: true }))]
  const visiblePage = Math.min(page, Math.max(1, Math.ceil(rows.length / pageSize)))
  const pricePattern = (row: Row) => prices.data?.find(p => p.providerId === null && p.modelPattern === (row.local?.name ?? row.name))?.modelPattern ?? row.defaults?.pricing?.modelPattern ?? row.name
  const actions = (row: Row) => <>
    <Button variant="ghost" size="icon-sm" aria-label={`${t("catalog.details")}: ${row.name}`} title={t("catalog.details")} onClick={() => setDetail(row)}><Info /></Button>
    <Button variant="ghost" size="icon-sm" aria-label={`${t("actions.edit")}: ${row.name}`} title={t("actions.edit")} onClick={() => { save.reset(); setEditing(row) }}><Pencil /></Button>
    <Button variant="ghost" size="icon-sm" aria-label={`${t("providers.models.pricing")}: ${row.name}`} title={t("providers.models.pricing")} onClick={() => setPricing(pricePattern(row))}><BadgeDollarSign /></Button>
  </>
  return <Page>
    <PageHeader title={t("nav.model-catalog")} actions={<Button onClick={() => { save.reset(); setEditing({ name: "", metadata: {} }) }}><Plus data-icon="inline-start" />{t("actions.new")}</Button>} />
    <div className="flex flex-col gap-4">
      <div className="flex flex-wrap items-center justify-between gap-3">
        <InputGroup className="w-full sm:max-w-sm"><InputGroupAddon><Search /></InputGroupAddon><InputGroupInput aria-label={t("catalog.searchPlaceholder")} placeholder={t("catalog.searchPlaceholder")} value={search} onChange={e => { setSearch(e.target.value); setPage(1) }} /></InputGroup>
        <span className="text-sm">{t("catalog.count", { count: rows.length })}</span>
      </div>
    </div>
    {save.error || remove.error || apply.error ? <ErrorNotice error={save.error || remove.error || apply.error} /> : null}
    <QueryState isPending={catalog.isPending || local.isPending || prices.isPending || instances.isPending || bindings.isPending} error={catalog.error || local.error || prices.error || instances.error || bindings.error}>
      <DataTable paginate={false} rows={rows.slice((visiblePage - 1) * pageSize, visiblePage * pageSize)} rowKey={r => r.name} empty={<EmptyNotice title={t("state.emptyTitle")} />} columns={[
        { key: "name", cell: r => <ModelIdentity name={r.name} /> },
        { key: "providers", header: t("catalog.providerInstances"), cell: r => <ModelProviders providers={r.providers ?? []} /> },
        { key: "limits", header: t("catalog.limits"), cell: r => <ModelLimits metadata={r.metadata} /> },
        { key: "capabilities", header: t("catalog.capabilities"), cell: r => <ModelCapabilities metadata={r.metadata} /> },
        { key: "price", header: t("catalog.referencePrice"), cell: r => <ReferencePrice model={r.defaults} /> },
      ]} actions={actions} renderCard={r => <ModelSummaryCard name={r.name} metadata={r.metadata} actions={actions(r)}><div className="flex flex-col gap-1"><span className="text-sm text-muted-foreground">{t("catalog.providerInstances")}</span><ModelProviders providers={r.providers ?? []} /></div><div className="flex items-start justify-between gap-3 border-t pt-3"><span className="text-sm text-muted-foreground">{t("catalog.referencePrice")}</span><ReferencePrice model={r.defaults} /></div></ModelSummaryCard>} />
      <Pagination page={visiblePage} pageSize={pageSize} total={rows.length} onPage={setPage} onPageSize={setPageSize} />
    </QueryState>
    {editing ? <RecordDialog open mode={editing.name ? "edit" : "create"} onOpenChange={open => { if (!open && !save.isPending) setEditing(null) }} title={t("actions.edit")} fields={fields} original={{ name: editing.local?.name ?? editing.name, ...editing.metadata }} pending={save.isPending} error={save.error} onSubmit={body => save.mutate(body)} /> : null}
    {pricing ? <ModelPricingDialog providerId={null} model={pricing} onClose={() => setPricing(null)} /> : null}
    {detail ? <Dialog open onOpenChange={open => { if (!open) setDetail(null) }}><DialogContent className="sm:max-w-3xl" aria-describedby={undefined}><DialogHeader><DialogTitle>{detail.name}</DialogTitle></DialogHeader><DialogBody>
      <div className="mb-3 flex flex-wrap items-center gap-2"><span className="text-sm text-muted-foreground">{t("catalog.providerInstances")}</span><ModelProviders providers={detail.providers ?? []} /></div>
      <div className="mb-3 flex flex-wrap gap-2">{detail.defaults?.pricing ? <Button disabled={apply.isPending} onClick={() => apply.mutate(detail)}>{t("catalog.applyPrice")}</Button> : null}{detail.local ? <ConfirmButton title={t("confirm.deleteTitle", { name: detail.name })} disabled={remove.isPending} onConfirm={() => remove.mutate(detail.local!.id, { onSuccess: () => setDetail(null) })}>{t("catalog.reset")}</ConfirmButton> : null}</div>
      {apply.error || remove.error ? <ErrorNotice error={apply.error || remove.error} /> : null}
      <p className="mb-3 whitespace-pre-wrap text-sm">{String(detail.metadata.description ?? "")}</p>
      <DataTable rows={detail.defaults?.pricing?.rates ?? []} rowKey={r => r.metric} empty={<EmptyNotice title={t("state.emptyTitle")} />} columns={[
        { key: "metric", header: t("modelUI.priceFields.metric"), cell: r => t(`modelUI.metrics.${r.metric}`, { defaultValue: r.metric }) },
        { key: "value", header: t("modelUI.priceFields.value"), cell: r => `${r.price} USD / ${r.unitSize}` },
      ]} />
      <dl className="my-4 flex flex-col gap-2 text-sm">{["input_modalities", "output_modalities", "supported_parameters"].map(key => <div key={key}><dt className="text-muted-foreground">{t(`catalog.${key}`)}</dt><dd className="break-words">{strings(detail.metadata[key])}</dd></div>)}</dl>
      <h3 className="my-3 text-sm font-medium">{t("modelUI.tiers")}</h3>
      <DataTable rows={(detail.defaults?.pricing?.tiers ?? []).map((tier, index) => ({ ...tier, id: String(index) }))} rowKey={r => r.id} empty={<EmptyNotice title={t("state.emptyTitle")} />} columns={[
        { key: "threshold", header: t("modelUI.priceFields.minPromptTokens"), cell: r => r.minPromptTokens ?? 0 },
        { key: "service", header: t("modelUI.priceFields.serviceTier"), cell: r => r.serviceTier ?? "—" },
        { key: "rates", header: t("modelUI.rates"), cell: r => <div className="whitespace-normal">{Object.entries(r).filter(([key, value]) => key.endsWith("Price") && value != null).map(([key, value]) => <div key={key}>{t(`catalog.tierFields.${key}`, { defaultValue: key })}: {String(value)}</div>)}</div> },
      ]} />
    </DialogBody></DialogContent></Dialog> : null}
  </Page>
}
