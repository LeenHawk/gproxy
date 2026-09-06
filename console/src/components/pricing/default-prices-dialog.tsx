import { useMemo, useState } from "react"
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query"
import { DownloadIcon, LoaderCircleIcon, SearchIcon } from "lucide-react"
import { useTranslation } from "react-i18next"
import { toast } from "sonner"
import { applyDefaultModelPrices, defaultModelCatalog } from "@/api/control"
import type { PriceRuleDto } from "@/generated/PriceRuleDto"
import { QueryState } from "@/components/query-state"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { Checkbox } from "@/components/ui/checkbox"
import { Dialog, DialogBody, DialogContent, DialogDescription, DialogFooter, DialogHeader, DialogTitle, DialogTrigger } from "@/components/ui/dialog"
import { Empty, EmptyHeader, EmptyTitle } from "@/components/ui/empty"
import { InputGroup, InputGroupAddon, InputGroupInput } from "@/components/ui/input-group"

export function DefaultPricesDialog({ providerId, rules }: { providerId: number | null; rules: Array<PriceRuleDto> }) {
  const { t } = useTranslation()
  const client = useQueryClient()
  const [open, setOpen] = useState(false)
  const [search, setSearch] = useState("")
  const [selected, setSelected] = useState<Set<string>>(new Set())
  const catalog = useQuery({ queryKey: ["default-model-catalog"], queryFn: defaultModelCatalog, enabled: open })
  const models = useMemo(() => (catalog.data?.models ?? []).filter((model) => model.pricing != null), [catalog.data])
  const existing = useMemo(() => new Set(rules.filter((rule) => rule.provider_id === providerId).map((rule) => rule.model_pattern)), [rules, providerId])
  const term = search.trim().toLowerCase()
  const visible = models.filter((model) => `${model.model_id} ${model.display_name ?? ""}`.toLowerCase().includes(term))
  const picked = models.filter((model) => selected.has(model.model_id))
  const allPicked = visible.length > 0 && visible.every((model) => selected.has(model.model_id))
  const somePicked = visible.some((model) => selected.has(model.model_id))
  const toggle = (id: string) => setSelected((previous) => {
    const next = new Set(previous)
    if (next.has(id)) next.delete(id); else next.add(id)
    return next
  })
  const toggleAll = () => setSelected((previous) => {
    const next = new Set(previous)
    visible.forEach((model) => allPicked ? next.delete(model.model_id) : next.add(model.model_id))
    return next
  })
  const changeOpen = (value: boolean) => {
    if (apply.isPending) return
    setOpen(value)
    setSearch("")
    setSelected(new Set())
  }
  const apply = useMutation({
    mutationFn: () => applyDefaultModelPrices({ provider_id: providerId, model_ids: picked.map((model) => model.model_id) }),
    onSuccess: async (result) => {
      await Promise.all([
        client.invalidateQueries({ queryKey: ["price-rules"] }),
        client.invalidateQueries({ queryKey: ["price-rates"] }),
      ])
      toast.success(t("pricing.defaults.applied", result))
      setOpen(false)
      setSearch("")
      setSelected(new Set())
    },
    onError: () => toast.error(t("pricing.defaults.saveError")),
  })

  return <Dialog open={open} onOpenChange={changeOpen}>
    <DialogTrigger asChild><Button size="sm" variant="outline"><DownloadIcon data-icon="inline-start" aria-hidden />{t("pricing.defaults.title")}</Button></DialogTrigger>
    <DialogContent className="sm:max-w-3xl" closeLabel={t("common.actions.close")} showCloseButton={!apply.isPending}>
      <DialogHeader>
        <DialogTitle>{t("pricing.defaults.title")}</DialogTitle>
        <DialogDescription>{t("pricing.defaults.description")}</DialogDescription>
      </DialogHeader>
      <DialogBody className="flex flex-col gap-3">
        <QueryState loading={catalog.isLoading} error={catalog.isError ? t("pricing.defaults.loadError") : ""}>
          <InputGroup>
            <InputGroupAddon><SearchIcon aria-hidden /></InputGroupAddon>
            <InputGroupInput aria-label={t("pricing.defaults.search")} placeholder={t("pricing.defaults.search")} value={search} onChange={(event) => setSearch(event.target.value)} disabled={apply.isPending} />
          </InputGroup>
          <div className="flex flex-wrap items-center justify-between gap-2 text-xs text-muted-foreground">
            <label className="flex items-center gap-2">
              <Checkbox checked={allPicked ? true : somePicked ? "indeterminate" : false} onCheckedChange={toggleAll} disabled={visible.length === 0 || apply.isPending} />
              {t("pricing.defaults.selectAll")}
            </label>
            <span>{t("pricing.defaults.count", { shown: visible.length, selected: picked.length })}</span>
          </div>
          {visible.length === 0 ? <Empty><EmptyHeader><EmptyTitle>{t("pricing.defaults.empty")}</EmptyTitle></EmptyHeader></Empty> : <div className="max-h-96 divide-y overflow-y-auto rounded-lg border">
            {visible.map((model) => <label key={model.model_id} className="flex min-h-16 items-center gap-3 px-3 py-2 [content-visibility:auto] [contain-intrinsic-size:auto_64px] hover:bg-muted/50">
              <Checkbox checked={selected.has(model.model_id)} onCheckedChange={() => toggle(model.model_id)} disabled={apply.isPending} aria-label={model.model_id} />
              <span className="min-w-0 flex-1">
                <span className="machine-text block break-all text-xs font-medium">{model.model_id}</span>
                <span className="block text-xs text-muted-foreground">{model.display_name}</span>
              </span>
              {existing.has(providerId == null ? model.pricing!.model_pattern : model.model_id) ? <Badge variant="secondary">{t("pricing.defaults.existing")}</Badge> : null}
            </label>)}
          </div>}
        </QueryState>
        {catalog.isError ? <Button variant="outline" onClick={() => void catalog.refetch()}>{t("pricing.defaults.retry")}</Button> : null}
      </DialogBody>
      <DialogFooter>
        <Button variant="outline" disabled={apply.isPending} onClick={() => changeOpen(false)}>{t("common.actions.cancel")}</Button>
        <Button disabled={picked.length === 0 || apply.isPending || catalog.isError} onClick={() => apply.mutate()}>
          {apply.isPending ? <LoaderCircleIcon className="animate-spin" data-icon="inline-start" aria-hidden /> : null}
          {t("pricing.defaults.apply", { count: picked.length })}
        </Button>
      </DialogFooter>
    </DialogContent>
  </Dialog>
}
