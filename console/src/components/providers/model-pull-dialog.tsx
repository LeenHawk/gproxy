import { useMemo, useState } from "react"
import { useMutation, useQueryClient } from "@tanstack/react-query"
import { useTranslation } from "react-i18next"
import { toast } from "sonner"
import { discoverModels, saveProviderModel } from "@/api/control"
import type { DiscoveredModelDto } from "@/generated/DiscoveredModelDto"
import type { ProviderModelDto } from "@/generated/ProviderModelDto"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { Checkbox } from "@/components/ui/checkbox"
import { Dialog, DialogContent, DialogFooter, DialogHeader, DialogTitle } from "@/components/ui/dialog"
import { Input } from "@/components/ui/input"

export function ModelPullDialog({ providerId, existing, open, onOpenChange }: { providerId: number; existing: Array<ProviderModelDto>; open: boolean; onOpenChange: (open: boolean) => void }) {
  const { t, i18n } = useTranslation()
  const client = useQueryClient()
  const [models, setModels] = useState<Array<DiscoveredModelDto>>([])
  const [selected, setSelected] = useState<Set<string>>(new Set())
  const [search, setSearch] = useState("")
  const [keyPrefix, setKeyPrefix] = useState<string>()

  const pull = useMutation({
    mutationFn: () => discoverModels({ provider_id: providerId }),
    onSuccess: (result) => {
      setKeyPrefix(result.key_prefix)
      if (!result.ok) { toast.error(result.message ?? t("providers.models.pullFailed", { status: result.status })); return }
      setModels(result.models)
      setSelected(new Set())
      if (result.models.length === 0) toast.info(t("providers.models.pullEmpty"))
    },
    onError: () => toast.error(t("providers.models.pullError")),
  })

  const rows = useMemo(() => new Map(existing.map((model) => [model.model_id, model])), [existing])
  // A row already added is still worth listing when the wire knows something it does
  // not: picking it fills only the blanks, never overwriting what the operator set.
  const gaps = (model: DiscoveredModelDto) => {
    const row = rows.get(model.model_id)
    if (!row) return 0
    return [
      row.display_name == null && model.display_name != null,
      row.context_window == null && model.context_window != null,
      row.max_output_tokens == null && model.max_output_tokens != null,
    ].filter(Boolean).length
  }
  const actionable = (model: DiscoveredModelDto) => !model.known || gaps(model) > 0

  const add = useMutation({
    mutationFn: async () => {
      const picked = models.filter((model) => selected.has(model.model_id) && actionable(model))
      for (const model of picked) {
        const row = rows.get(model.model_id)
        await saveProviderModel({
          provider_id: providerId,
          model_id: model.model_id,
          display_name: row?.display_name ?? model.display_name,
          variants: row?.variants ?? null,
          context_window: row?.context_window ?? model.context_window,
          max_output_tokens: row?.max_output_tokens ?? model.max_output_tokens,
          thinking_supported: row?.thinking_supported ?? null,
          thinking_adaptive_supported: row?.thinking_adaptive_supported ?? null,
          thinking_enabled_supported: row?.thinking_enabled_supported ?? null,
          enabled: row?.enabled ?? true,
        }, row?.id)
      }
      return picked.length
    },
    onSuccess: async (added) => {
      await client.invalidateQueries({ queryKey: ["provider-models"] })
      toast.success(t("providers.models.pullAdded", { added }))
      onOpenChange(false)
    },
    onError: () => toast.error(t("providers.models.saveError")),
  })

  const term = search.trim().toLowerCase()
  const visible = useMemo(() => term
    ? models.filter((model) => model.model_id.toLowerCase().includes(term) || (model.display_name ?? "").toLowerCase().includes(term))
    : models, [models, term])
  // Select-all acts on what is both new and visible, so you can search, select, search
  // again, and keep what you already picked.
  const addable = visible.filter(actionable)
  const allPicked = addable.length > 0 && addable.every((model) => selected.has(model.model_id))
  const toggle = (id: string) => setSelected((previous) => {
    const next = new Set(previous)
    if (next.has(id)) next.delete(id); else next.add(id)
    return next
  })
  const toggleAll = () => setSelected((previous) => {
    const next = new Set(previous)
    addable.forEach((model) => allPicked ? next.delete(model.model_id) : next.add(model.model_id))
    return next
  })
  const number = (value: number | null) => value == null ? "—" : value.toLocaleString(i18n.language)

  return (
    <Dialog open={open} onOpenChange={(value) => { onOpenChange(value); if (!value) { setModels([]); setSelected(new Set()); setSearch("") } }}>
      <DialogContent className="sm:max-w-2xl">
        <DialogHeader><DialogTitle>{t("providers.models.pullTitle")}</DialogTitle></DialogHeader>
        <div className="flex items-center gap-2">
          <Button size="sm" onClick={() => pull.mutate()} disabled={pull.isPending}>{t(pull.isPending ? "providers.models.pulling" : "providers.models.pull")}</Button>
          {models.length > 0 ? <Input value={search} onChange={(event) => setSearch(event.target.value)} placeholder={t("common.dataTable.search")} className="h-8" /> : null}
          {keyPrefix ? <span className="machine-text shrink-0 text-xs text-muted-foreground">{keyPrefix}</span> : null}
        </div>
        {models.length > 0 ? (
          <div className="flex max-h-[50vh] flex-col gap-1 overflow-y-auto rounded-md border p-1">
            <label className="flex items-center gap-2 border-b px-2 py-1.5 text-xs text-muted-foreground">
              <Checkbox checked={allPicked} onCheckedChange={toggleAll} disabled={addable.length === 0} aria-label={t("common.dataTable.selectAll")} />
              {t("providers.models.pullCount", { shown: visible.length, selected: selected.size })}
            </label>
            {visible.map((model) => (
              <label key={model.model_id} className="flex items-center gap-2 rounded px-2 py-1.5 hover:bg-muted/50">
                <Checkbox checked={selected.has(model.model_id)} disabled={!actionable(model)} onCheckedChange={() => toggle(model.model_id)} aria-label={model.model_id} />
                <span className="min-w-0 flex-1">
                  <span className="machine-text block truncate text-xs">{model.model_id}</span>
                  {model.display_name ? <span className="block truncate text-xs text-muted-foreground">{model.display_name}</span> : null}
                </span>
                <span className="machine-text shrink-0 text-xs text-muted-foreground">{number(model.context_window)} / {number(model.max_output_tokens)}</span>
                {model.known ? <Badge variant={gaps(model) > 0 ? "outline" : "secondary"}>{t(gaps(model) > 0 ? "providers.models.pullGaps" : "providers.models.pullKnown", { count: gaps(model) })}</Badge> : null}
              </label>
            ))}
          </div>
        ) : null}
        <DialogFooter>
          <Button variant="ghost" onClick={() => onOpenChange(false)}>{t("common.actions.cancel")}</Button>
          <Button onClick={() => add.mutate()} disabled={selected.size === 0 || add.isPending}>{t("providers.models.pullAdd", { count: selected.size })}</Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  )
}
