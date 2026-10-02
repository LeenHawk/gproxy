import { useState } from "react"
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query"
import { useTranslation } from "react-i18next"
import { directory, priceRules, priceRates, priceTiers } from "@/api/models"
import type { PriceRateDto, PriceRuleDto, PriceTierDto } from "@/generated/sdk"
import { RecordDialog, type FormField } from "@/components/record-form"
import { DataTable } from "@/components/data-table"
import { ConfirmButton } from "@/components/confirm"
import { QueryState, ErrorNotice, EmptyNotice } from "@/components/state"
import { Button } from "@/components/ui/button"
import { Dialog, DialogBody, DialogContent, DialogHeader, DialogTitle } from "@/components/ui/dialog"
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs"
import { Select, SelectContent, SelectGroup, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select"
import { object } from "@/components/providers/provider-model-state"
const profiles: Record<string, string[]> = {
  generation: ["input_tokens", "output_tokens", "cached_input_tokens", "cache_creation_5m_tokens", "cache_creation_30m_tokens", "cache_creation_1h_tokens", "reasoning_tokens"],
  embedding: ["input_tokens"], rerank: ["search_units"], image: ["image_input_tokens", "image_output_tokens", "image_outputs"],
  audio: ["audio_input_tokens", "cached_audio_input_tokens", "audio_output_tokens", "audio_seconds", "audio_characters"],
  video: ["video_input_tokens", "video_tokens", "video_seconds", "video_outputs"],
  tools: ["web_searches", "web_fetches", "file_searches", "code_interpreter_sessions", "tool_calls", "requests"],
}
const tokenTiers = ["inputPerMillion", "outputPerMillion", "cacheReadPerMillion", "cacheCreation5mPerMillion", "cacheCreation30mPerMillion", "cacheCreation1hPerMillion", "reasoningPerMillion", "imageInputPerMillion", "imageOutputPerMillion", "audioInputPerMillion", "cachedAudioInputPerMillion", "audioOutputPerMillion", "videoInputPerMillion", "videoPerMillion"]
export function ModelPricingDialog({ providerId, model, onClose }: { providerId: string | null; model: string; onClose: () => void }) {
  const { t } = useTranslation(), client = useQueryClient()
  const [profile, setProfile] = useState("generation"), [selected, setSelected] = useState<string | null>(null)
  const [editing, setEditing] = useState<{ kind: "rule" | "rate" | "tier"; row?: PriceRuleDto | PriceRateDto | PriceTierDto; metric?: string } | null>(null)
  const list = useQuery({ queryKey: ["admin", "/price-rules", providerId, model], queryFn: () => directory(priceRules, { modelPattern: model, ...(providerId ? { providerId } : { globalOnly: true }) }) })
  const candidates = list.data?.filter(r => r.modelPattern === model && r.providerId === providerId) ?? []
  const rule = candidates.find(r => r.id === selected) ?? candidates.find(r => r.operation === null) ?? candidates[0]
  const rates = useQuery({ queryKey: ["admin", "/price-rates", rule?.id], queryFn: () => directory(priceRates, { priceRuleId: rule!.id }), enabled: !!rule })
  const tiers = useQuery({ queryKey: ["admin", "/price-tiers", rule?.id], queryFn: () => directory(priceTiers, { priceRuleId: rule!.id }), enabled: !!rule })
  const refresh = () => Promise.all([client.invalidateQueries({ queryKey: ["model-catalog"] }), ...["/price-rules", "/price-rates", "/price-tiers", "/provider-models"].map(path => client.invalidateQueries({ queryKey: ["admin", path] }))])
  const save = useMutation({ mutationFn: async (body: Record<string, unknown>) => {
    if (editing!.kind === "rule") return editing!.row ? priceRules.update(editing!.row.id, body) : priceRules.create({ currency: "USD", ...body, providerId, modelPattern: model })
    if (editing!.kind === "tier") return editing!.row ? priceTiers.update(editing!.row.id, body) : priceTiers.create({ ...body, priceRuleId: rule!.id })
    if ("conditions" in body) {
      const pairs = (Array.isArray(body.conditions) ? body.conditions as string[] : []).map(line => { const i = line.indexOf("="); if (i < 1 || !line.slice(i + 1).trim()) throw new Error(t("modelUI.invalidCondition")); return [line.slice(0, i).trim(), line.slice(i + 1).trim()] })
      if (new Set(pairs.map(p => p[0])).size !== pairs.length) throw new Error(t("modelUI.invalidCondition"))
      body = { ...body, conditions: pairs.length ? Object.fromEntries(pairs) : null }
    }
    return editing!.row ? priceRates.update(editing!.row.id, body) : priceRates.create({ ...body, priceRuleId: rule!.id })
  }, onSuccess: async row => { if (editing?.kind === "rule") setSelected(row.id); setEditing(null); await refresh() } })
  const remove = useMutation({ mutationFn: ({ kind, id }: { kind: "rule" | "rate" | "tier"; id: string }) => kind === "rule" ? priceRules.remove(id) : kind === "rate" ? priceRates.remove(id) : priceTiers.remove(id), onSuccess: refresh })
  const open = (next: NonNullable<typeof editing>) => { save.reset(); setEditing(next) }
  const field = (name: string, kind: FormField["kind"] = "text", nullable = true): FormField => ({ name, kind, nullable, label: t(`modelUI.priceFields.${name}`) })
  const fields: FormField[] = editing?.kind === "rule" ? [field("operation"), field("priority", "number", false), field("enabled", "switch", false)] : editing?.kind === "tier" ? [field("serviceTier"), field("minPromptTokens", "number", false), field("priority", "number", false), field("multiplier"), ...tokenTiers.map(name => field(name))] : [{ ...field("metric"), required: true }, { ...field("unit", "select", false), choices: ["token", "count", "second", "character"].map(value => ({ value, label: t(`modelUI.units.${value}`) })), required: true }, { ...field("unitQuantity"), required: true }, { ...field("value"), required: true }, field("priority", "number", false), field("conditions", "lines")]
  const original = editing?.row ? { ...editing.row, ...("conditions" in editing.row ? { conditions: Object.entries(object(editing.row.conditions)).map(([k, v]) => `${k}=${String(v)}`) } : {}) } : editing?.kind === "rate" ? { metric: editing.metric ?? "", unit: editing.metric?.includes("tokens") ? "token" : editing.metric?.endsWith("seconds") ? "second" : editing.metric?.endsWith("characters") ? "character" : "count", unitQuantity: editing.metric?.includes("tokens") ? "1000000" : "1", value: "", priority: 0, conditions: [] } : editing?.kind === "rule" ? { currency: "USD", enabled: true, priority: 0 } : { minPromptTokens: 0, priority: 0 }
  const empty = <EmptyNotice title={t("state.emptyTitle")} />
  return <Dialog open onOpenChange={v => { if (!v && !save.isPending) onClose() }}><DialogContent className="sm:max-w-5xl" closeLabel={t("actions.close")} aria-describedby={undefined}>
    <DialogHeader><DialogTitle>{model} · {t("providers.models.pricing")}</DialogTitle></DialogHeader><DialogBody>
      <QueryState isPending={list.isPending} error={list.error}>
        {!rule ? <Button onClick={() => open({ kind: "rule" })}>{t("modelUI.createPrice")}</Button> : <div className="flex flex-col gap-4">
          <Button className="self-start" variant="outline" onClick={() => open({ kind: "rule" })}>{t("management.newPriceRule")}</Button>
          {candidates.length > 1 ? <Select value={rule.id} onValueChange={setSelected}><SelectTrigger aria-label={t("form.priceRule")}><SelectValue /></SelectTrigger><SelectContent><SelectGroup>{candidates.map(r => <SelectItem key={r.id} value={r.id}>{r.operation ?? t("modelUI.allOperations")} · {r.currency} · {r.priority}</SelectItem>)}</SelectGroup></SelectContent></Select> : null}
          <Tabs defaultValue="rates"><TabsList><TabsTrigger value="rates">{t("modelUI.rates")}</TabsTrigger><TabsTrigger value="tiers">{t("modelUI.tiers")}</TabsTrigger><TabsTrigger value="settings">{t("providers.settings")}</TabsTrigger></TabsList>
            <TabsContent value="settings"><div className="flex flex-wrap items-center gap-3"><span>{rule.currency} · {rule.operation ?? t("modelUI.allOperations")}</span><Button onClick={() => open({ kind: "rule", row: rule })}>{t("actions.edit")}</Button><ConfirmButton title={t("confirm.deleteTitle", { name: model })} onConfirm={() => remove.mutate({ kind: "rule", id: rule.id })}>{t("actions.delete")}</ConfirmButton></div></TabsContent>
            <TabsContent value="rates" className="flex flex-col gap-3"><div className="flex flex-wrap gap-2"><Select value={profile} onValueChange={setProfile}><SelectTrigger aria-label={t("form.priceProfile")}><SelectValue /></SelectTrigger><SelectContent><SelectGroup>{Object.keys(profiles).map(k => <SelectItem key={k} value={k}>{t(`modelUI.profiles.${k}`)}</SelectItem>)}</SelectGroup></SelectContent></Select><Button variant="outline" onClick={() => open({ kind: "rate" })}>{t("modelUI.customRate")}</Button></div><div className="flex flex-wrap gap-2">{profiles[profile].map(metric => <Button key={metric} variant="outline" size="sm" onClick={() => open({ kind: "rate", metric })}>{t(`modelUI.metrics.${metric}`)}</Button>)}</div>
              <QueryState isPending={rates.isPending} error={rates.error}><DataTable rows={rates.data ?? []} rowKey={r => r.id} empty={empty} columns={[
                { key: "metric", header: t("modelUI.priceFields.metric"), cell: r => t(`modelUI.metrics.${r.metric}`, { defaultValue: r.metric }) },
                { key: "value", header: t("modelUI.priceFields.value"), cell: r => `${r.value} ${rule.currency} / ${r.unitQuantity} ${t(`modelUI.units.${r.unit}`)}` },
                { key: "conditions", header: t("modelUI.conditions"), cell: r => Object.entries(object(r.conditions)).map(([k,v]) => `${k}=${String(v)}`).join(", ") || "—" },
                { key: "priority", cell: r => r.priority },
              ]} actions={r => <><Button variant="ghost" size="sm" onClick={() => open({ kind: "rate", row: r })}>{t("actions.edit")}</Button><ConfirmButton title={t("confirm.deleteTitle", { name: r.metric })} onConfirm={() => remove.mutate({ kind: "rate", id: r.id })}>{t("actions.delete")}</ConfirmButton></>} /></QueryState>
            </TabsContent>
            <TabsContent value="tiers" className="flex flex-col gap-3"><Button className="self-start" onClick={() => open({ kind: "tier" })}>{t("actions.new")}</Button><QueryState isPending={tiers.isPending} error={tiers.error}><DataTable rows={tiers.data ?? []} rowKey={r => r.id} empty={empty} columns={[
              { key: "serviceTier", header: t("modelUI.priceFields.serviceTier"), cell: r => r.serviceTier ?? "—" }, { key: "minPromptTokens", header: t("modelUI.priceFields.minPromptTokens"), cell: r => r.minPromptTokens }, { key: "multiplier", header: t("modelUI.priceFields.multiplier"), cell: r => r.multiplier ?? "—" }, { key: "priority", cell: r => r.priority },
            ]} actions={r => <><Button variant="ghost" size="sm" onClick={() => open({ kind: "tier", row: r })}>{t("actions.edit")}</Button><ConfirmButton title={t("confirm.deleteTitle", { name: r.serviceTier ?? model })} onConfirm={() => remove.mutate({ kind: "tier", id: r.id })}>{t("actions.delete")}</ConfirmButton></>} /></QueryState></TabsContent>
          </Tabs>
        </div>}
      </QueryState>{remove.error ? <ErrorNotice error={remove.error} /> : null}
    </DialogBody>
    {editing ? <RecordDialog open onOpenChange={v => { if (!v && !save.isPending) setEditing(null) }} title={t(`modelUI.edit.${editing.kind}`)} mode={editing.row ? "edit" : "create"} original={original} fields={fields} onSubmit={body => save.mutate(body)} pending={save.isPending} error={save.error} /> : null}
  </DialogContent></Dialog>
}
