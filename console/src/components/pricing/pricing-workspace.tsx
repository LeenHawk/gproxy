import { useMutation, useQueryClient } from "@tanstack/react-query"
import { PencilIcon, Trash2Icon } from "lucide-react"
import { useMemo } from "react"
import { useTranslation } from "react-i18next"
import { toast } from "sonner"
import { deletePriceRate } from "@/api/control"
import type { PriceRateDto } from "@/generated/PriceRateDto"
import type { PriceRuleDto } from "@/generated/PriceRuleDto"
import type { ProviderDto } from "@/generated/ProviderDto"
import { DataTable, type DataTableColumn } from "@/components/data-table"
import { BatchActions } from "@/components/batch-actions"
import { EntityDeleteButton } from "@/components/entity-delete-button"
import { PriceRateDialog } from "@/components/pricing/price-rate-dialog"
import { PriceRuleDialog } from "@/components/pricing/price-rule-dialog"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { Card, CardAction, CardContent, CardHeader, CardTitle } from "@/components/ui/card"
import { adminPath, navigateAdminPath, useAdminLocation } from "@/lib/admin-route"
import { cn } from "@/lib/utils"

export function PricingWorkspace({ rules, rates, providers }: { rules: Array<PriceRuleDto>; rates: Array<PriceRateDto>; providers: Array<ProviderDto> }) {
  const { t } = useTranslation()
  const queryClient = useQueryClient()
  const location = useAdminLocation()
  const providerNames = useMemo(() => new Map(providers.map((provider) => [provider.id, provider.name])), [providers])
  const selectedId = Number(location.segments[0])
  const selected = rules.find((rule) => rule.id === selectedId) ?? null
  const selectedRates = rates.filter((rate) => rate.rule_id === selected?.id)
  const remove = useMutation({
    mutationFn: deletePriceRate,
    onSuccess: async () => { await queryClient.invalidateQueries({ queryKey: ["price-rates"] }); toast.success(t("pricing.rates.deleted")) },
    onError: () => toast.error(t("pricing.rates.deleteError")),
  })
  const ruleActions = (rule: PriceRuleDto) => <div className="flex items-center justify-end gap-2" onClick={(event) => event.stopPropagation()}><PriceRuleDialog rule={rule} providers={providers} trigger={<Button size="icon-sm" variant="outline" aria-label={t("common.actions.edit")}><PencilIcon aria-hidden /></Button>} /><EntityDeleteButton entity="price-rules" id={rule.id} label={rule.model_pattern} queryKeys={["price-rules", "price-rates"]} /></div>
  const ruleColumns: Array<DataTableColumn<PriceRuleDto>> = [
    { key: "model", label: t("pricing.rules.pattern"), header: t("pricing.rules.pattern"), cell: (rule) => <span className="font-mono text-xs">{rule.model_pattern}</span> },
    { key: "provider", label: t("pricing.rules.provider"), header: t("pricing.rules.provider"), cell: (rule) => rule.provider_id == null ? t("pricing.rules.allProviders") : providerNames.get(rule.provider_id) ?? rule.provider_id },
    { key: "status", label: t("common.status.label"), header: t("common.status.label"), cell: (rule) => <Badge variant={rule.enabled ? "outline" : "secondary"}>{t(`common.status.${rule.enabled ? "enabled" : "disabled"}`)}</Badge> },
    { key: "actions", label: t("common.actions.edit"), header: <span className="sr-only">{t("common.actions.edit")}</span>, cell: ruleActions },
  ]
  const rateActions = (rate: PriceRateDto) => <div className="flex items-center justify-end gap-2"><PriceRateDialog rate={rate} rules={rules} trigger={<Button size="sm" variant="outline">{t("common.actions.edit")}</Button>} /><Button size="icon-sm" variant="ghost" aria-label={t("common.actions.delete")} disabled={remove.isPending} onClick={() => remove.mutate(rate.id)}><Trash2Icon aria-hidden /></Button></div>
  const rateColumns: Array<DataTableColumn<PriceRateDto>> = [
    { key: "metric", label: t("pricing.rates.metric"), header: t("pricing.rates.metric"), cell: (rate) => <div><p className="font-mono text-xs">{rate.metric}</p><p className="text-xs text-muted-foreground">{t("pricing.rates.summary", { price: rate.price, units: rate.unit_size })}</p></div> },
    { key: "priority", label: t("pricing.rates.priority"), header: t("pricing.rates.priority"), cell: (rate) => <Badge variant="outline">{t("pricing.rates.priorityValue", { value: rate.priority })}</Badge> },
    { key: "actions", label: t("common.actions.edit"), header: <span className="sr-only">{t("common.actions.edit")}</span>, cell: rateActions },
  ]
  return (
    <div className="flex flex-col gap-4">
      <div className="flex justify-end"><PriceRuleDialog providers={providers} trigger={<Button>{t("pricing.rules.add")}</Button>} /></div>
      <div className="grid min-w-0 gap-5 md:grid-cols-[minmax(18rem,0.8fr)_minmax(0,1.2fr)]">
        <div className={cn(selected && "hidden md:block")}><DataTable columns={ruleColumns} rows={rules} rowKey={(rule) => rule.id} searchText={(rule) => `${rule.model_pattern} ${rule.provider_id == null ? t("pricing.rules.allProviders") : providerNames.get(rule.provider_id) ?? rule.provider_id}`} renderCard={(rule) => <div className="flex flex-col gap-3"><div className="flex items-start justify-between gap-3"><div><p className="font-mono text-xs">{rule.model_pattern}</p><p className="text-xs text-muted-foreground">{rule.provider_id == null ? t("pricing.rules.allProviders") : providerNames.get(rule.provider_id) ?? rule.provider_id}</p></div><Badge variant={rule.enabled ? "outline" : "secondary"}>{t(`common.status.${rule.enabled ? "enabled" : "disabled"}`)}</Badge></div>{ruleActions(rule)}</div>} empty={t("pricing.rules.empty")} storageKey="price-rules" activeRowKey={selected?.id} selectable batchActions={(rows) => <BatchActions entity="price-rules" rows={rows} queryKeys={["price-rules", "price-rates"]} />} onRowClick={(rule) => navigateAdminPath(`/admin/pricing/${rule.id}/rates`)} /></div>
        <div className={cn("min-w-0", !selected && "hidden md:block")}>
          {selected ? <><Button className="mb-3 md:hidden" variant="ghost" onClick={() => navigateAdminPath(adminPath("pricing"))}>{t("common.actions.back")}</Button><Card><CardHeader><CardTitle className="font-mono">{selected.model_pattern}</CardTitle><CardAction><PriceRateDialog rules={rules} initialRuleId={selected.id} trigger={<Button size="sm" variant="outline">{t("pricing.rates.add")}</Button>} /></CardAction></CardHeader><CardContent><DataTable columns={rateColumns} rows={selectedRates} rowKey={(rate) => rate.id} searchText={(rate) => `${rate.metric} ${rate.price}`} renderCard={(rate) => <div className="flex flex-col gap-3"><div><p className="font-mono text-xs">{rate.metric}</p><p className="text-xs text-muted-foreground">{t("pricing.rates.summary", { price: rate.price, units: rate.unit_size })}</p></div>{rateActions(rate)}</div>} empty={t("pricing.rates.empty")} storageKey="price-rates" selectable batchActions={(rows) => <BatchActions entity="price-rates" rows={rows} queryKeys={["price-rates"]} toggle={false} remove />} /></CardContent></Card></> : <div className="grid min-h-80 place-items-center text-sm text-muted-foreground">{t("pricing.rules.selectPrompt")}</div>}
        </div>
      </div>
    </div>
  )
}
