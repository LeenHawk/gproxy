import { useState } from "react"
import { useMutation, useQuery } from "@tanstack/react-query"
import { useTranslation } from "react-i18next"
import { toast } from "sonner"
import { applyDefaultPrices, defaultModels, discoverModels } from "@/api/models"
import { providerModels } from "@/api/configuration"
import type { ProviderModelDto } from "@/generated/sdk"
import { object } from "@/components/providers/provider-model-state"
import { ErrorNotice, QueryState } from "@/components/state"
import { Button } from "@/components/ui/button"
import { Checkbox } from "@/components/ui/checkbox"
import { Input } from "@/components/ui/input"
import { Dialog, DialogBody, DialogContent, DialogFooter, DialogHeader, DialogTitle } from "@/components/ui/dialog"
export function ModelImportDialog({ mode, providerId, models, onClose, onSaved }: { mode: "models" | "prices"; providerId: string; models: ProviderModelDto[]; onClose: () => void; onSaved: () => Promise<unknown> }) {
  const { t } = useTranslation()
  const [search, setSearch] = useState(""), [selected, setSelected] = useState<Set<string>>(new Set()), [prices, setPrices] = useState(true)
  const catalog = useQuery({ queryKey: ["default-model-catalog"], queryFn: defaultModels })
  const discovery = useQuery({ queryKey: ["discover-models", providerId], queryFn: () => discoverModels(providerId), enabled: mode === "models", retry: false, staleTime: 0 })
  const rows = mode === "models" ? (discovery.data ?? []).map(m => ({ name: m.upstreamName, metadata: m.metadata, price: m.hasDefaultPrice })) : (catalog.data?.models ?? []).filter(m => m.pricing).map(m => ({ name: m.modelId, metadata: { display_name: m.displayName, context_window: m.contextWindow, max_output_tokens: m.maxOutputTokens }, price: true }))
  const visible = rows.filter(r => `${r.name} ${object(r.metadata).display_name ?? ""}`.toLowerCase().includes(search.toLowerCase()))
  const all = visible.length > 0 && visible.every(r => selected.has(r.name))
  const apply = useMutation({ mutationFn: async () => {
    const picked = rows.filter(r => selected.has(r.name))
    if (mode === "models") for (const item of picked) {
      const existing = models.find(m => m.upstreamName === item.name)
      const metadata = { ...object(existing?.metadata) }
      for (const [key, value] of Object.entries(object(item.metadata))) if (metadata[key] == null && value != null) metadata[key] = value
      if (existing) { if (JSON.stringify(metadata) !== JSON.stringify(existing.metadata)) await providerModels.update(existing.id, { metadata }) }
      else await providerModels.create({ providerId, upstreamName: item.name, metadata, enabled: true })
    }
    if (mode === "prices" || prices) await applyDefaultPrices(providerId, picked.filter(r => r.price).map(r => r.name))
  }, onSuccess: async () => { await onSaved(); toast.success(t("toast.saved")); onClose() } })
  return <Dialog open onOpenChange={open => { if (!open && !apply.isPending) onClose() }}><DialogContent className="sm:max-w-3xl" aria-describedby={undefined}>
    <DialogHeader><DialogTitle>{t(mode === "models" ? "providers.models.pullTitle" : "modelUI.defaultPrices")}</DialogTitle></DialogHeader>
    <DialogBody className="flex flex-col gap-3"><QueryState isPending={catalog.isPending || (mode === "models" && discovery.isPending)} error={catalog.error || discovery.error}>
      <div className="flex gap-2"><Input aria-label={t("actions.search")} placeholder={t("actions.search")} value={search} onChange={e => setSearch(e.target.value)} />{mode === "models" ? <Button variant="outline" onClick={() => void discovery.refetch()} disabled={discovery.isFetching}>{t("modelUI.refresh")}</Button> : null}</div>
      <label className="flex items-center gap-2"><Checkbox checked={all} onCheckedChange={() => setSelected(previous => { const next = new Set(previous); visible.forEach(r => all ? next.delete(r.name) : next.add(r.name)); return next })} />{t("modelUI.selectAll")} ({selected.size}/{rows.length})</label>
      {mode === "models" ? <label className="flex items-center gap-2"><Checkbox checked={prices} onCheckedChange={v => setPrices(v === true)} />{t("modelUI.importPrices")}</label> : null}
      <div className="flex flex-col divide-y">{visible.map(r => <label key={r.name} className="flex items-center gap-3 py-3"><Checkbox checked={selected.has(r.name)} onCheckedChange={checked => setSelected(previous => { const next = new Set(previous); if (checked) next.add(r.name); else next.delete(r.name); return next })} /><span className="min-w-0 flex-1 break-all">{r.name}</span>{models.some(m => m.upstreamName === r.name) ? <span className="text-xs text-muted-foreground">{t("modelUI.known")}</span> : null}{r.price ? <span className="text-xs text-muted-foreground">{t("providers.models.priced")}</span> : null}</label>)}</div>
    </QueryState>{apply.error ? <ErrorNotice error={apply.error} /> : null}</DialogBody>
    <DialogFooter><Button variant="outline" disabled={apply.isPending} onClick={onClose}>{t("actions.cancel")}</Button><Button disabled={apply.isPending || !selected.size} onClick={() => apply.mutate()}>{t("modelUI.importSelected")}</Button></DialogFooter>
  </DialogContent></Dialog>
}
