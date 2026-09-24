import { useMemo, useState } from "react"
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query"
import { useTranslation } from "react-i18next"
import { toast } from "sonner"
import { applyDefaultPrices, defaultModels, directory, models, priceRules } from "@/api/models"
import type { DefaultModelDto, ModelDto } from "@/generated/sdk"
import { object } from "@/components/providers/provider-model-state"
import { Page, PageHeader } from "@/components/page"
import { DataTable, Pagination } from "@/components/data-table"
import { ConfirmButton } from "@/components/confirm"
import { RecordDialog, type FormField } from "@/components/record-form"
import { EmptyNotice, ErrorNotice, QueryState } from "@/components/state"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import { Dialog, DialogBody, DialogContent, DialogHeader, DialogTitle } from "@/components/ui/dialog"
import { ModelPricingDialog } from "@/pages/providers/model-pricing"

type Row = { name: string; metadata: Record<string, unknown>; defaults?: DefaultModelDto; local?: ModelDto }
const metadataFields = ["display_name", "description", "context_window", "max_output_tokens", "input_modalities", "output_modalities", "supported_parameters"] as const
function defaultMetadata(model: DefaultModelDto) {
  const metadata: Record<string, unknown> = { ...model }
  for (const key of ["modelId", "pricing", "displayName", "contextWindow", "maxOutputTokens"]) delete metadata[key]
  return { ...metadata, display_name: model.displayName, context_window: model.contextWindow, max_output_tokens: model.maxOutputTokens }
}
const strings = (value: unknown) => Array.isArray(value) ? value.join(", ") : "—"

export function ModelCatalogPage() {
  const { t } = useTranslation(), client = useQueryClient()
  const catalog = useQuery({ queryKey: ["default-model-catalog"], queryFn: defaultModels })
  const local = useQuery({ queryKey: ["admin", "/models"], queryFn: () => directory(models) })
  const prices = useQuery({ queryKey: ["admin", "/price-rules", "global"], queryFn: () => directory(priceRules) })
  const [search, setSearch] = useState(""), [page, setPage] = useState(1)
  const [editing, setEditing] = useState<Row | null>(null), [detail, setDetail] = useState<Row | null>(null), [pricing, setPricing] = useState<string | null>(null)
  const rows = useMemo(() => {
    const result = new Map<string, Row>((catalog.data?.models ?? []).map(defaults => [defaults.modelId.toLowerCase(), { name: defaults.modelId, metadata: defaultMetadata(defaults), defaults }]))
    for (const row of local.data ?? []) {
      const key = row.name.toLowerCase(), previous = result.get(key)
      result.set(key, { ...previous, name: row.name, metadata: { ...previous?.metadata, ...object(row.metadata) }, local: row })
    }
    const needle = search.toLowerCase()
    return [...result.values()].filter(row => `${row.name} ${row.metadata.display_name ?? ""} ${strings(row.metadata.input_modalities)} ${strings(row.metadata.output_modalities)} ${strings(row.metadata.supported_parameters)}`.toLowerCase().includes(needle)).sort((a, b) => a.name.localeCompare(b.name))
  }, [catalog.data, local.data, search])
  const refresh = () => Promise.all([client.invalidateQueries({ queryKey: ["admin", "/models"] }), client.invalidateQueries({ queryKey: ["discover-models"] })])
  const save = useMutation({ mutationFn: async (body: Record<string, unknown>) => {
    const metadata = { ...object(editing?.local?.metadata) }
    for (const key of metadataFields) if (key in body) metadata[key] = body[key]
    if (editing?.local) return models.update(editing.local.id, { name: String(body.name ?? editing.name), metadata })
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
  const visiblePage = Math.min(page, Math.max(1, Math.ceil(rows.length / 50)))
  const pricePattern = (row: Row) => prices.data?.find(p => p.providerId === null && p.modelPattern === row.name)?.modelPattern ?? row.defaults?.pricing?.modelPattern ?? row.name
  return <Page>
    <PageHeader title={t("nav.model-catalog")} actions={<Button onClick={() => { save.reset(); setEditing({ name: "", metadata: {} }) }}>{t("actions.new")}</Button>} />
    <p className="text-sm text-muted-foreground">{t("catalog.help")}</p>
    <p className="text-sm text-muted-foreground">{catalog.data?.source.catalog} · {catalog.data?.source.fetchedAt} · {t("catalog.count", { count: catalog.data?.source.totalModels ?? 0 })}</p>
    <Input className="max-w-md" aria-label={t("actions.search")} placeholder={t("actions.search")} value={search} onChange={e => { setSearch(e.target.value); setPage(1) }} />
    {save.error || remove.error || apply.error ? <ErrorNotice error={save.error || remove.error || apply.error} /> : null}
    <QueryState isPending={catalog.isPending || local.isPending || prices.isPending} error={catalog.error || local.error || prices.error}>
      <DataTable rows={rows.slice((visiblePage - 1) * 50, visiblePage * 50)} rowKey={r => r.name} empty={<EmptyNotice title={t("state.emptyTitle")} />} columns={[
        { key: "name", cell: r => <div title={r.name}>{r.name}<div className="text-xs text-muted-foreground">{String(r.metadata.display_name ?? "")}</div></div> },
        { key: "context", header: t("catalog.context_window"), cell: r => String(r.metadata.context_window ?? "—") },
        { key: "output", header: t("catalog.max_output_tokens"), cell: r => String(r.metadata.max_output_tokens ?? "—") },
        { key: "capabilities", header: t("catalog.capabilities"), cell: r => <span title={strings(r.metadata.supported_parameters)}>{strings(r.metadata.input_modalities)} → {strings(r.metadata.output_modalities)}</span> },
        { key: "price", header: t("catalog.referencePrice"), cell: r => r.defaults?.pricing ? ["input_tokens", "output_tokens"].map(metric => r.defaults?.pricing?.rates.find(rate => rate.metric === metric)?.price ?? "—").join(" / ") : "—" },
        { key: "tiers", header: t("modelUI.tiers"), cell: r => r.defaults?.pricing?.tiers?.length ?? 0 },
        { key: "source", header: t("catalog.source"), cell: r => t(r.local ? "catalog.local" : "catalog.bundled") },
      ]} actions={r => <>
        <Button variant="ghost" size="sm" onClick={() => setDetail(r)}>{t("catalog.details")}</Button>
        <Button variant="ghost" size="sm" onClick={() => { save.reset(); setEditing(r) }}>{t("actions.edit")}</Button>
        <Button variant="ghost" size="sm" onClick={() => setPricing(pricePattern(r))}>{t("providers.models.pricing")}</Button>
      </>} />
      <Pagination page={visiblePage} pageSize={50} total={rows.length} onPage={setPage} />
    </QueryState>
    {editing ? <RecordDialog open mode={editing.name ? "edit" : "create"} onOpenChange={open => { if (!open && !save.isPending) setEditing(null) }} title={t("actions.edit")} fields={fields} original={{ name: editing.name, ...editing.metadata }} pending={save.isPending} error={save.error} onSubmit={body => save.mutate(body)} /> : null}
    {pricing ? <ModelPricingDialog providerId={null} model={pricing} onClose={() => setPricing(null)} /> : null}
    {detail ? <Dialog open onOpenChange={open => { if (!open) setDetail(null) }}><DialogContent className="sm:max-w-3xl" aria-describedby={undefined}><DialogHeader><DialogTitle>{detail.name}</DialogTitle></DialogHeader><DialogBody>
      <p className="mb-3 text-sm text-muted-foreground">{t("catalog.referenceHelp")}</p>
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
      {detail.defaults ? <p className="mt-4 text-sm"><a className="underline" href={detail.defaults.metadata_source ? `https://openrouter.ai/${detail.defaults.modelId}` : "https://github.com/openai/codex/tree/main/codex-rs/models-manager"} target="_blank" rel="noreferrer">{detail.defaults.metadata_source ? "OpenRouter" : "Codex"} · {t("catalog.source")}</a> · {catalog.data?.source.fetchedAt}</p> : null}
    </DialogBody></DialogContent></Dialog> : null}
  </Page>
}
