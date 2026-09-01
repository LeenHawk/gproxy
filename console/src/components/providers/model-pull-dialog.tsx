import { useMemo, useState, type ReactElement } from "react"
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query"
import { AlertCircleIcon, LoaderCircleIcon, RefreshCwIcon, SearchIcon } from "lucide-react"
import { useTranslation } from "react-i18next"
import { toast } from "sonner"
import { applyDefaultPrices, defaultPriceCatalog, discoverModels, saveProviderModel } from "@/api/control"
import type { DiscoveredModelDto } from "@/generated/DiscoveredModelDto"
import type { PriceRuleDto } from "@/generated/PriceRuleDto"
import type { ProviderModelDto } from "@/generated/ProviderModelDto"
import { ModelPullList, type ModelPullAction } from "@/components/providers/model-pull-list"
import { ModelPullPriceOption } from "@/components/providers/model-pull-price-option"
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import { Button } from "@/components/ui/button"
import { Checkbox } from "@/components/ui/checkbox"
import { Dialog, DialogBody, DialogContent, DialogDescription, DialogFooter, DialogHeader, DialogTitle, DialogTrigger } from "@/components/ui/dialog"
import { Empty, EmptyDescription, EmptyHeader, EmptyTitle } from "@/components/ui/empty"
import { InputGroup, InputGroupAddon, InputGroupInput } from "@/components/ui/input-group"
import { exactProviderPrices, findDefaultPrice } from "@/lib/default-pricing"

type Props = {
  providerId: number
  existing: Array<ProviderModelDto>
  priceRules: Array<PriceRuleDto>
  trigger: ReactElement
}

export function ModelPullDialog({ providerId, existing, priceRules, trigger }: Props) {
  const { t } = useTranslation()
  const client = useQueryClient()
  const [open, setOpen] = useState(false)
  const [models, setModels] = useState<Array<DiscoveredModelDto>>([])
  const [selected, setSelected] = useState<Set<string>>(new Set())
  const [search, setSearch] = useState("")
  const [keyPrefix, setKeyPrefix] = useState<string>()
  const [pullError, setPullError] = useState("")
  const [importPrices, setImportPrices] = useState(true)
  const catalog = useQuery({ queryKey: ["default-price-catalog"], queryFn: defaultPriceCatalog, enabled: open, staleTime: Infinity })

  const rows = useMemo(() => new Map(existing.map((model) => [model.model_id, model])), [existing])
  const priced = useMemo(() => exactProviderPrices(providerId, priceRules), [priceRules, providerId])
  const defaultPriced = useMemo(() => new Set(models
    .filter((model) => findDefaultPrice(catalog.data, model.model_id) != null)
    .map((model) => model.model_id)), [catalog.data, models])
  const gaps = (model: DiscoveredModelDto) => {
    const row = rows.get(model.model_id)
    if (!row) return 0
    return [
      row.display_name == null && model.display_name != null,
      row.context_window == null && model.context_window != null,
      row.max_output_tokens == null && model.max_output_tokens != null,
    ].filter(Boolean).length
  }
  const modelWrite = (model: DiscoveredModelDto) => !model.known || gaps(model) > 0
  const priceAvailable = (model: DiscoveredModelDto) => !priced.has(model.model_id) && defaultPriced.has(model.model_id)
  const actionFor = (model: DiscoveredModelDto): ModelPullAction => ({
    actionable: modelWrite(model) || (importPrices && priceAvailable(model)),
    gaps: gaps(model),
    priceAvailable: priceAvailable(model),
    priced: priced.has(model.model_id),
  })

  const pull = useMutation({
    mutationFn: () => discoverModels({ provider_id: providerId }),
    onMutate: () => setPullError(""),
    onSuccess: (result) => {
      setKeyPrefix(result.key_prefix)
      setSelected(new Set())
      if (!result.ok) {
        setModels([])
        setPullError(result.message ?? t("providers.models.pullFailed", { status: result.status }))
        return
      }
      setModels(result.models)
    },
    onError: () => {
      setModels([])
      setPullError(t("providers.models.pullError"))
    },
  })

  const term = search.trim().toLowerCase()
  const visible = useMemo(() => term
    ? models.filter((model) => model.model_id.toLowerCase().includes(term) || (model.display_name ?? "").toLowerCase().includes(term))
    : models, [models, term])
  const addable = visible.filter((model) => actionFor(model).actionable)
  const picked = models.filter((model) => selected.has(model.model_id) && actionFor(model).actionable)
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

  const add = useMutation({
    mutationFn: async () => {
      let saved = 0
      for (const model of picked.filter(modelWrite)) {
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
        saved += 1
      }
      const priceModels = importPrices ? picked.filter(priceAvailable).map((model) => model.model_id) : []
      const prices = priceModels.length > 0 ? await applyDefaultPrices({ provider_id: providerId, model_ids: priceModels }) : { created: 0 }
      return { saved, priced: prices.created }
    },
    onSuccess: async ({ saved, priced: imported }) => {
      await Promise.all([
        client.invalidateQueries({ queryKey: ["provider-models"] }),
        client.invalidateQueries({ queryKey: ["price-rules"] }),
        client.invalidateQueries({ queryKey: ["price-rates"] }),
      ])
      toast.success(t("providers.models.pullSynced", { models: saved, prices: imported }))
      close()
    },
    onError: () => toast.error(t("providers.models.saveError")),
  })

  const reset = () => {
    setModels([])
    setSelected(new Set())
    setSearch("")
    setKeyPrefix(undefined)
    setPullError("")
    setImportPrices(true)
  }
  const close = () => { setOpen(false); reset() }
  const changeOpen = (value: boolean) => {
    setOpen(value)
    if (value) pull.mutate(); else reset()
  }

  return <Dialog open={open} onOpenChange={changeOpen}>
    <DialogTrigger asChild>{trigger}</DialogTrigger>
    <DialogContent className="sm:max-w-3xl" closeLabel={t("common.actions.close")}>
      <DialogHeader>
        <DialogTitle>{t("providers.models.pullTitle")}</DialogTitle>
        <DialogDescription>{t("providers.models.pullDescription")}</DialogDescription>
      </DialogHeader>
      <DialogBody className="flex flex-col gap-3">
        {pull.isPending && models.length === 0 ? <Empty className="min-h-56 border">
          <LoaderCircleIcon className="animate-spin text-muted-foreground" aria-hidden />
          <EmptyHeader><EmptyTitle>{t("providers.models.pulling")}</EmptyTitle><EmptyDescription>{t("providers.models.pullLoadingHint")}</EmptyDescription></EmptyHeader>
        </Empty> : pullError ? <Alert variant="destructive">
          <AlertCircleIcon aria-hidden />
          <AlertTitle>{t("providers.models.pullErrorTitle")}</AlertTitle>
          <AlertDescription>{pullError}</AlertDescription>
          <Button className="mt-2 justify-self-start" size="sm" variant="outline" onClick={() => pull.mutate()}>{t("providers.models.pullRetry")}</Button>
        </Alert> : models.length === 0 ? <Empty className="min-h-56 border">
          <EmptyHeader><EmptyTitle>{t("providers.models.pullEmpty")}</EmptyTitle><EmptyDescription>{t("providers.models.pullEmptyHint")}</EmptyDescription></EmptyHeader>
          <Button size="sm" variant="outline" onClick={() => pull.mutate()}><RefreshCwIcon data-icon="inline-start" aria-hidden />{t("providers.models.pullRefresh")}</Button>
        </Empty> : <>
          <div className="flex items-center gap-2">
            <InputGroup>
              <InputGroupAddon><SearchIcon aria-hidden /></InputGroupAddon>
              <InputGroupInput value={search} onChange={(event) => setSearch(event.target.value)} placeholder={t("providers.models.pullSearch")} />
            </InputGroup>
            <Button size="icon-sm" variant="outline" disabled={pull.isPending || add.isPending} aria-label={t("providers.models.pullRefresh")} onClick={() => pull.mutate()}>{pull.isPending ? <LoaderCircleIcon className="animate-spin" aria-hidden /> : <RefreshCwIcon aria-hidden />}</Button>
          </div>
          <div className="flex items-center justify-between gap-3 px-1 text-xs text-muted-foreground">
            <label className="flex items-center gap-2">
              <Checkbox checked={allPicked} onCheckedChange={toggleAll} disabled={addable.length === 0 || add.isPending} aria-label={t("common.dataTable.selectAll")} />
              {t("providers.models.pullCount", { shown: visible.length, selected: picked.length })}
            </label>
            {keyPrefix ? <span className="machine-text max-w-44 truncate">{t("providers.models.pullCredential", { prefix: keyPrefix })}</span> : null}
          </div>
          <ModelPullPriceOption checked={importPrices} onCheckedChange={setImportPrices} disabled={add.isPending || catalog.isPending || catalog.isError} />
          {visible.length > 0
            ? <ModelPullList models={visible} selected={selected} pending={add.isPending} actionFor={actionFor} onToggle={toggle} />
            : <Empty className="min-h-36 border"><EmptyHeader><EmptyTitle>{t("providers.models.pullNoMatches")}</EmptyTitle><EmptyDescription>{t("providers.models.pullNoMatchesHint")}</EmptyDescription></EmptyHeader></Empty>}
        </>}
      </DialogBody>
      <DialogFooter>
        <Button variant="outline" onClick={close}>{t("common.actions.cancel")}</Button>
        <Button onClick={() => add.mutate()} disabled={picked.length === 0 || add.isPending || pull.isPending}>{add.isPending ? <LoaderCircleIcon className="animate-spin" data-icon="inline-start" aria-hidden /> : null}{t("providers.models.pullAdd", { count: picked.length })}</Button>
      </DialogFooter>
    </DialogContent>
  </Dialog>
}
