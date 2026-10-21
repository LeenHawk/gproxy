import { useId, useState } from "react"
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query"
import { useTranslation } from "react-i18next"
import { directory, priceRules, priceRates, priceTiers } from "@/api/models"
import type { PriceRateDto, PriceRuleDto, PriceTierDto } from "@/generated/sdk"
import { RecordDialog, type FormField } from "@/components/record-form"
import { DataTable } from "@/components/data-table"
import { ConfirmButton } from "@/components/confirm"
import { QueryState, ErrorNotice, EmptyNotice } from "@/components/state"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import { Field, FieldDescription, FieldGroup, FieldLabel, FieldLegend, FieldSet } from "@/components/ui/field"
import { Dialog, DialogBody, DialogContent, DialogHeader, DialogTitle } from "@/components/ui/dialog"
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs"
import { Select, SelectContent, SelectGroup, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select"
import { priceRuleFor } from "@/lib/model-pricing"
import { object } from "@/components/providers/provider-model-state"
const profiles: Record<string, string[]> = {
  generation: ["input_tokens", "output_tokens", "cached_input_tokens", "cache_creation_tokens", "reasoning_tokens"],
  embedding: ["input_tokens"], rerank: ["search_units"], image: ["image_input_tokens", "image_output_tokens", "image_outputs"],
  audio: ["audio_input_tokens", "cached_audio_input_tokens", "audio_output_tokens", "audio_seconds", "audio_characters"],
  video: ["video_input_tokens", "video_tokens", "video_seconds", "video_outputs"],
  tools: ["web_searches", "web_fetches", "file_searches", "code_interpreter_sessions", "tool_calls", "requests"],
}
const tokenTiers = ["inputPerMillion", "outputPerMillion", "cacheReadPerMillion", "reasoningPerMillion", "imageInputPerMillion", "imageOutputPerMillion", "audioInputPerMillion", "cachedAudioInputPerMillion", "audioOutputPerMillion", "videoInputPerMillion", "videoPerMillion"]
export function ModelPricingDialog({ providerId, model, matchingRules = [], onClose }: { providerId: string | null; model: string; matchingRules?: readonly PriceRuleDto[]; onClose: () => void }) {
  const { t } = useTranslation(), client = useQueryClient()
  const [cachePrices, setCachePrices] = useState<CachePriceRow[]>([])
  const [profile, setProfile] = useState("generation"), [selected, setSelected] = useState<string | null>(null)
  const [editing, setEditing] = useState<{ kind: "rule" | "rate" | "tier"; row?: PriceRuleDto | PriceRateDto | PriceTierDto; metric?: string } | null>(null)
  const list = useQuery({ queryKey: ["admin", "/price-rules", providerId, model], queryFn: () => directory(priceRules, { modelPattern: model, ...(providerId ? { providerId } : { globalOnly: true }) }) })
  const local = list.data?.filter(r => r.modelPattern === model && r.providerId === providerId) ?? []
  const candidates = [...local, ...matchingRules.filter(rule => !local.some(row => row.id === rule.id))]
    .sort((a, b) => Number(a.providerId === null) - Number(b.providerId === null) || a.priority - b.priority || (a.id < b.id ? -1 : a.id > b.id ? 1 : 0))
  const rule = candidates.find(r => r.id === selected) ?? priceRuleFor(candidates) ?? candidates.find(r => r.enabled) ?? candidates[0]
  const readOnly = !!rule && (rule.providerId !== providerId || rule.modelPattern !== model)
  const rates = useQuery({ queryKey: ["admin", "/price-rates", rule?.id], queryFn: () => directory(priceRates, { priceRuleId: rule!.id }), enabled: !!rule })
  const tiers = useQuery({ queryKey: ["admin", "/price-tiers", rule?.id], queryFn: () => directory(priceTiers, { priceRuleId: rule!.id }), enabled: !!rule })
  const refresh = () => Promise.all([client.invalidateQueries({ queryKey: ["admin", "/models"] }), client.invalidateQueries({ queryKey: ["discover-models"] }), ...["/price-rules", "/price-rates", "/price-tiers", "/provider-models"].map(path => client.invalidateQueries({ queryKey: ["admin", path] }))])
  const save = useMutation({ mutationFn: async (body: Record<string, unknown>) => {
    if (editing!.kind === "rule") return editing!.row ? priceRules.update(editing!.row.id, body) : priceRules.create({ currency: "USD", ...body, providerId, modelPattern: model })
    if (editing!.kind === "tier") {
      const prices = cachePrices.map(entry => {
        const ttlSeconds = entry.ttl.trim() ? Number(entry.ttl) : null
        if (ttlSeconds !== null && (!Number.isSafeInteger(ttlSeconds) || ttlSeconds <= 0)) throw new Error(t("modelUI.invalidCacheTtl"))
        return { ttlSeconds, price: entry.price.trim() }
      })
      const previous = editing!.row as PriceTierDto | undefined
      if (!previous || JSON.stringify(prices) !== JSON.stringify(previous.cacheCreationPerMillion)) body = { ...body, cacheCreationPerMillion: prices }
      return previous ? priceTiers.update(previous.id, body) : priceTiers.create({ ...body, priceRuleId: rule!.id })
    }
    if ("conditions" in body) {
      const pairs = (Array.isArray(body.conditions) ? body.conditions as string[] : []).map(line => { const i = line.indexOf("="); if (i < 1 || !line.slice(i + 1).trim()) throw new Error(t("modelUI.invalidCondition")); return [line.slice(0, i).trim(), line.slice(i + 1).trim()] })
      if (new Set(pairs.map(p => p[0])).size !== pairs.length) throw new Error(t("modelUI.invalidCondition"))
      body = { ...body, conditions: pairs.length ? Object.fromEntries(pairs) : null }
    }
    if ("ttlSeconds" in body || (cacheRate && "conditions" in body)) {
      const { ttlSeconds: rawTtl, ...rest } = body
      const value = "ttlSeconds" in body ? rawTtl : object((editing!.row as PriceRateDto | undefined)?.conditions).ttl_seconds
      const ttlSeconds = value == null ? null : Number(value)
      if (ttlSeconds !== null && (!Number.isSafeInteger(ttlSeconds) || Number(ttlSeconds) <= 0)) throw new Error(t("modelUI.invalidCacheTtl"))
      const conditions = { ...object("conditions" in rest ? rest.conditions : (editing!.row as PriceRateDto | undefined)?.conditions) }
      delete conditions.ttl_seconds
      if (ttlSeconds !== null) conditions.ttl_seconds = ttlSeconds
      body = { ...rest, conditions: Object.keys(conditions).length ? conditions : null }
    }
    return editing!.row ? priceRates.update(editing!.row.id, body) : priceRates.create({ ...body, priceRuleId: rule!.id })
  }, onSuccess: async row => { if (editing?.kind === "rule") setSelected(row.id); setEditing(null); await refresh() } })
  const remove = useMutation({ mutationFn: ({ kind, id }: { kind: "rule" | "rate" | "tier"; id: string }) => kind === "rule" ? priceRules.remove(id) : kind === "rate" ? priceRates.remove(id) : priceTiers.remove(id), onSuccess: refresh })
  const open = (next: NonNullable<typeof editing>) => {
    save.reset()
    setCachePrices(next.kind === "tier" && next.row ? (next.row as PriceTierDto).cacheCreationPerMillion.map(entry => ({ ttl: entry.ttlSeconds?.toString() ?? "", price: entry.price })) : [])
    setEditing(next)
  }
  const field = (name: string, kind: FormField["kind"] = "text", nullable = true): FormField => ({ name, kind, nullable, label: t(`modelUI.priceFields.${name}`) })
  const cacheRate = editing?.kind === "rate" && (editing.metric ?? (editing.row as PriceRateDto | undefined)?.metric) === "cache_creation_tokens"
  const fields: FormField[] = editing?.kind === "rule" ? [field("operation"), field("priority", "number", false), field("enabled", "switch", false)] : editing?.kind === "tier" ? [field("serviceTier"), field("minPromptTokens", "number", false), field("priority", "number", false), field("multiplier"), ...tokenTiers.map(name => field(name))] : [{ ...field("metric"), required: true }, { ...field("unit", "select", false), choices: ["token", "count", "second", "character"].map(value => ({ value, label: t(`modelUI.units.${value}`) })), required: true }, { ...field("unitQuantity"), required: true }, { ...field("value"), required: true }, field("priority", "number", false), ...(cacheRate ? [{ ...field("ttlSeconds", "number"), label: t("modelUI.cacheTtlSeconds"), description: t("modelUI.cacheRateTtlHelp") }] : []), field("conditions", "lines")]
  const original = editing?.row ? { ...editing.row, ...("conditions" in editing.row ? { conditions: Object.entries(object(editing.row.conditions)).filter(([k]) => !cacheRate || k !== "ttl_seconds").map(([k, v]) => `${k}=${String(v)}`), ...(cacheRate ? { ttlSeconds: object(editing.row.conditions).ttl_seconds ?? null } : {}) } : {}) } : editing?.kind === "rate" ? { metric: editing.metric ?? "", unit: editing.metric?.includes("tokens") ? "token" : editing.metric?.endsWith("seconds") ? "second" : editing.metric?.endsWith("characters") ? "character" : "count", unitQuantity: editing.metric?.includes("tokens") ? "1000000" : "1", value: "", priority: 0, conditions: [] } : editing?.kind === "rule" ? { currency: "USD", enabled: true, priority: 0 } : { minPromptTokens: 0, priority: 0 }
  const empty = <EmptyNotice title={t("state.emptyTitle")} />
  return <Dialog open onOpenChange={v => { if (!v && !save.isPending) onClose() }}><DialogContent className="sm:max-w-5xl" closeLabel={t("actions.close")} aria-describedby={undefined}>
    <DialogHeader><DialogTitle>{model} · {t("providers.models.pricing")}</DialogTitle></DialogHeader><DialogBody>
      <QueryState isPending={list.isPending} error={list.error}>
        {!rule ? <Button onClick={() => open({ kind: "rule" })}>{t("modelUI.createPrice")}</Button> : <div className="flex flex-col gap-4">
          <Button className="self-start" variant="outline" onClick={() => open({ kind: "rule" })}>{t("management.newPriceRule")}</Button>
          {readOnly ? <p className="text-sm text-muted-foreground">{t(rule.providerId === null ? "modelUI.inheritedPrice" : "modelUI.matchedPrice", { pattern: rule.modelPattern })}</p> : null}
          {candidates.length > 1 ? <Select value={rule.id} onValueChange={setSelected}><SelectTrigger aria-label={t("form.priceRule")}><SelectValue /></SelectTrigger><SelectContent><SelectGroup>{candidates.map(r => <SelectItem key={r.id} value={r.id}>{r.operation ?? t("modelUI.allOperations")} · {r.currency} · {r.priority}</SelectItem>)}</SelectGroup></SelectContent></Select> : null}
          <Tabs defaultValue="rates"><TabsList><TabsTrigger value="rates">{t("modelUI.rates")}</TabsTrigger><TabsTrigger value="tiers">{t("modelUI.tiers")}</TabsTrigger><TabsTrigger value="settings">{t("providers.settings")}</TabsTrigger></TabsList>
            <TabsContent value="settings"><div className="flex flex-wrap items-center gap-3"><span>{rule.currency} · {rule.operation ?? t("modelUI.allOperations")}</span><Button disabled={readOnly} onClick={() => open({ kind: "rule", row: rule })}>{t("actions.edit")}</Button><ConfirmButton disabled={readOnly} title={t("confirm.deleteTitle", { name: model })} onConfirm={() => remove.mutate({ kind: "rule", id: rule.id })}>{t("actions.delete")}</ConfirmButton></div></TabsContent>
            <TabsContent value="rates" className="flex flex-col gap-3"><div className="flex flex-wrap gap-2"><Select value={profile} onValueChange={setProfile}><SelectTrigger aria-label={t("form.priceProfile")}><SelectValue /></SelectTrigger><SelectContent><SelectGroup>{Object.keys(profiles).map(k => <SelectItem key={k} value={k}>{t(`modelUI.profiles.${k}`)}</SelectItem>)}</SelectGroup></SelectContent></Select><Button disabled={readOnly} variant="outline" onClick={() => open({ kind: "rate" })}>{t("modelUI.customRate")}</Button></div><div className="flex flex-wrap gap-2">{profiles[profile].map(metric => <Button disabled={readOnly} key={metric} variant="outline" size="sm" onClick={() => open({ kind: "rate", metric })}>{t(`modelUI.metrics.${metric}`)}</Button>)}</div>
              <QueryState isPending={rates.isPending} error={rates.error}><DataTable rows={rates.data ?? []} rowKey={r => r.id} empty={empty} columns={[
                { key: "metric", header: t("modelUI.priceFields.metric"), cell: r => t(`modelUI.metrics.${r.metric}`, { defaultValue: r.metric }) },
                { key: "value", header: t("modelUI.priceFields.value"), cell: r => `${r.value} ${rule.currency} / ${r.unitQuantity} ${t(`modelUI.units.${r.unit}`)}` },
                { key: "conditions", header: t("modelUI.conditions"), cell: r => Object.entries(object(r.conditions)).map(([k,v]) => `${k}=${String(v)}`).join(", ") || "—" },
                { key: "priority", cell: r => r.priority },
              ]} actions={readOnly ? undefined : r => <><Button variant="ghost" size="sm" onClick={() => open({ kind: "rate", row: r })}>{t("actions.edit")}</Button><ConfirmButton title={t("confirm.deleteTitle", { name: r.metric })} onConfirm={() => remove.mutate({ kind: "rate", id: r.id })}>{t("actions.delete")}</ConfirmButton></>} /></QueryState>
            </TabsContent>
            <TabsContent value="tiers" className="flex flex-col gap-3"><Button disabled={readOnly} className="self-start" onClick={() => open({ kind: "tier" })}>{t("actions.new")}</Button><QueryState isPending={tiers.isPending} error={tiers.error}><DataTable rows={tiers.data ?? []} rowKey={r => r.id} empty={empty} columns={[
              { key: "serviceTier", header: t("modelUI.priceFields.serviceTier"), cell: r => r.serviceTier ?? "—" }, { key: "minPromptTokens", header: t("modelUI.priceFields.minPromptTokens"), cell: r => r.minPromptTokens }, { key: "multiplier", header: t("modelUI.priceFields.multiplier"), cell: r => r.multiplier ?? "—" }, { key: "priority", cell: r => r.priority },
            ]} actions={readOnly ? undefined : r => <><Button variant="ghost" size="sm" onClick={() => open({ kind: "tier", row: r })}>{t("actions.edit")}</Button><ConfirmButton title={t("confirm.deleteTitle", { name: r.serviceTier ?? model })} onConfirm={() => remove.mutate({ kind: "tier", id: r.id })}>{t("actions.delete")}</ConfirmButton></>} /></QueryState></TabsContent>
          </Tabs>
        </div>}
      </QueryState>{remove.error ? <ErrorNotice error={remove.error} /> : null}
    </DialogBody>
    {editing ? <RecordDialog open onOpenChange={v => { if (!v && !save.isPending) setEditing(null) }} title={t(`modelUI.edit.${editing.kind}`)} mode={editing.row ? "edit" : "create"} original={original} fields={fields} onSubmit={body => save.mutate(body)} pending={save.isPending} error={save.error} extra={editing.kind === "tier" ? <CachePriceEditor rows={cachePrices} onChange={setCachePrices} disabled={save.isPending} /> : undefined} /> : null}
  </DialogContent></Dialog>
}


type CachePriceRow = { ttl: string; price: string }

function CachePriceEditor({ rows, onChange, disabled }: { rows: CachePriceRow[]; onChange: (rows: CachePriceRow[]) => void; disabled: boolean }) {
  const { t } = useTranslation(), id = useId()
  const change = (index: number, patch: Partial<CachePriceRow>) => onChange(rows.map((row, i) => i === index ? { ...row, ...patch } : row))
  return <FieldSet disabled={disabled}>
    <FieldLegend>{t("modelUI.cacheWritePrices")}</FieldLegend>
    <FieldDescription>{t("modelUI.cacheWritePricesHelp")}</FieldDescription>
    {rows.map((row, index) => <FieldGroup key={index} className="flex flex-row flex-wrap items-end">
      <Field className="min-w-32 flex-1"><FieldLabel htmlFor={`${id}-ttl-${index}`}>{t("modelUI.cacheTtlSeconds")}</FieldLabel><Input id={`${id}-ttl-${index}`} type="number" min={1} step={1} value={row.ttl} onChange={event => change(index, { ttl: event.target.value })} /></Field>
      <Field className="min-w-32 flex-1"><FieldLabel htmlFor={`${id}-price-${index}`}>{t("modelUI.cacheWritePerMillion")}</FieldLabel><Input id={`${id}-price-${index}`} inputMode="decimal" value={row.price} onChange={event => change(index, { price: event.target.value })} /></Field>
      <Button type="button" variant="outline" onClick={() => onChange(rows.filter((_, i) => i !== index))}>{t("actions.delete")}</Button>
    </FieldGroup>)}
    <Button type="button" variant="outline" className="self-start" onClick={() => onChange([...rows, { ttl: "", price: "" }])}>{t("actions.add")}</Button>
  </FieldSet>
}
