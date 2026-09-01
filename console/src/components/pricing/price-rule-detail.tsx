import { useMutation, useQueryClient } from "@tanstack/react-query"
import { Trash2Icon } from "lucide-react"
import { useTranslation } from "react-i18next"
import { toast } from "sonner"
import { deletePriceRate } from "@/api/control"
import type { PriceRateDto } from "@/generated/PriceRateDto"
import type { PriceRuleDto } from "@/generated/PriceRuleDto"
import type { ProviderDto } from "@/generated/ProviderDto"
import { BatchActions } from "@/components/batch-actions"
import { DataTable, type DataTableColumn } from "@/components/data-table"
import { EntityDeleteButton } from "@/components/entity-delete-button"
import { PriceRateDialog } from "@/components/pricing/price-rate-dialog"
import { PriceRuleDialog } from "@/components/pricing/price-rule-dialog"
import { PRICE_FIELDS, tierDrafts } from "@/components/pricing/tier-values"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { Card, CardAction, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card"
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs"

type Props = {
  rule: PriceRuleDto
  rules: Array<PriceRuleDto>
  rates: Array<PriceRateDto>
  providers: Array<ProviderDto>
  providerNames: Map<number, string>
  scopeProviderId?: number | null
  tab: string
  onTab: (tab: string) => void
}

export function PriceRuleDetail(props: Props) {
  const { t } = useTranslation()
  const queryClient = useQueryClient()
  const tiers = tierDrafts(props.rule.tiers)
  const removeRate = useMutation({
    mutationFn: deletePriceRate,
    onSuccess: async () => {
      await queryClient.invalidateQueries({ queryKey: ["price-rates"] })
      toast.success(t("pricing.rates.deleted"))
    },
    onError: () => toast.error(t("pricing.rates.deleteError")),
  })
  const editRule = () => <PriceRuleDialog
    rule={props.rule}
    providers={props.providers}
    fixedProviderId={props.scopeProviderId}
    trigger={<Button size="sm" variant="outline">{t("common.actions.edit")}</Button>}
  />
  const rateActions = (rate: PriceRateDto) => <div className="flex items-center justify-end gap-2">
    <PriceRateDialog rate={rate} rules={props.rules} trigger={<Button size="sm" variant="outline">{t("common.actions.edit")}</Button>} />
    <Button size="icon-sm" variant="ghost" aria-label={t("common.actions.delete")} disabled={removeRate.isPending} onClick={() => removeRate.mutate(rate.id)}><Trash2Icon aria-hidden /></Button>
  </div>
  const rateColumns: Array<DataTableColumn<PriceRateDto>> = [
    { key: "metric", label: t("pricing.rates.metric"), header: t("pricing.rates.metric"), cell: (rate) => <div><p className="font-mono text-xs">{rate.metric}</p><p className="text-xs text-muted-foreground">{t("pricing.rates.summary", { price: rate.price, units: rate.unit_size })}</p></div> },
    { key: "priority", label: t("pricing.rates.priority"), header: t("pricing.rates.priority"), cell: (rate) => <Badge variant="outline">{t("pricing.rates.priorityValue", { value: rate.priority })}</Badge> },
    { key: "actions", label: t("common.actions.edit"), header: <span className="sr-only">{t("common.actions.edit")}</span>, cell: rateActions },
  ]

  return <div className="flex flex-col gap-4">
    <Card>
      <CardHeader>
        <CardTitle className="font-mono">{props.rule.model_pattern}</CardTitle>
        <CardDescription>{providerLabel(props.rule, props.providerNames, t)}</CardDescription>
      </CardHeader>
      <CardContent className="flex flex-wrap gap-2">
        <Badge variant={props.rule.enabled ? "outline" : "secondary"}>{t(`common.status.${props.rule.enabled ? "enabled" : "disabled"}`)}</Badge>
        <Badge variant="secondary">{t("pricing.rules.priority")}: {props.rule.priority}</Badge>
      </CardContent>
    </Card>
    <Tabs value={props.tab} onValueChange={props.onTab}>
      <TabsList variant="line">
        <TabsTrigger value="settings">{t("pricing.rules.settings")}</TabsTrigger>
        <TabsTrigger value="rates">{t("pricing.rates.title")}</TabsTrigger>
        <TabsTrigger value="tiers">{t("pricing.tiers.title")}</TabsTrigger>
      </TabsList>
      <TabsContent value="settings" className="pt-4">
        <Card>
          <CardHeader>
            <CardTitle>{t("pricing.rules.settings")}</CardTitle>
            <CardAction className="flex items-center gap-2">
              {editRule()}
              <EntityDeleteButton entity="price-rules" id={props.rule.id} label={props.rule.model_pattern} queryKeys={["price-rules", "price-rates"]} />
            </CardAction>
          </CardHeader>
          <CardContent className="grid gap-3 text-sm sm:grid-cols-2">
            <p>{t("pricing.rules.pattern")}: <span className="font-mono">{props.rule.model_pattern}</span></p>
            <p>{t("pricing.rules.provider")}: {providerLabel(props.rule, props.providerNames, t)}</p>
            <p>{t("pricing.rules.priority")}: <span className="font-mono">{props.rule.priority}</span></p>
            <p>{t("common.status.label")}: {t(`common.status.${props.rule.enabled ? "enabled" : "disabled"}`)}</p>
          </CardContent>
        </Card>
      </TabsContent>
      <TabsContent value="rates" className="pt-4">
        <Card>
          <CardHeader>
            <CardTitle>{t("pricing.rates.title")}</CardTitle>
            <CardAction><PriceRateDialog rules={props.rules} initialRuleId={props.rule.id} trigger={<Button size="sm" variant="outline">{t("pricing.rates.add")}</Button>} /></CardAction>
          </CardHeader>
          <CardContent>
            <DataTable columns={rateColumns} rows={props.rates} rowKey={(rate) => rate.id} searchText={(rate) => `${rate.metric} ${rate.price}`} renderCard={(rate) => <div className="flex flex-col gap-3"><div><p className="font-mono text-xs">{rate.metric}</p><p className="text-xs text-muted-foreground">{t("pricing.rates.summary", { price: rate.price, units: rate.unit_size })}</p></div>{rateActions(rate)}</div>} empty={t("pricing.rates.empty")} storageKey={`price-rule-${props.rule.id}-rates`} selectable batchActions={(rows, done) => <BatchActions entity="price-rates" rows={rows} queryKeys={["price-rates"]} toggle={false} remove onApplied={done} />} />
          </CardContent>
        </Card>
      </TabsContent>
      <TabsContent value="tiers" className="pt-4">
        <Card>
          <CardHeader><CardTitle>{t("pricing.tiers.title")}</CardTitle><CardAction>{editRule()}</CardAction></CardHeader>
          <CardContent className="flex flex-col gap-3">
            {tiers.length ? tiers.map((tier, index) => <div key={index} className="flex flex-col gap-3 rounded-md border p-3">
              <div className="flex flex-wrap items-center gap-2">
                <Badge>{tier.serviceTier || t("pricing.tiers.base")}</Badge>
                {tier.threshold ? <span className="text-xs text-muted-foreground">{t("pricing.tiers.threshold")}: {tier.threshold}</span> : null}
                {tier.multiplier ? <span className="text-xs text-muted-foreground">{t("pricing.tiers.multiplier")}: {tier.multiplier}</span> : null}
              </div>
              <div className="grid gap-2 text-sm sm:grid-cols-2 lg:grid-cols-3">
                {PRICE_FIELDS.filter((field) => tier.prices[field]).map((field) => <p key={field}>{t(`pricing.tiers.${field}`)}: <span className="font-mono">{tier.prices[field]}</span></p>)}
              </div>
            </div>) : <p className="text-sm text-muted-foreground">{t("pricing.tiers.empty")}</p>}
          </CardContent>
        </Card>
      </TabsContent>
    </Tabs>
  </div>
}

function providerLabel(rule: PriceRuleDto, names: Map<number, string>, t: (key: string) => string) {
  return rule.provider_id == null ? t("pricing.rules.allProviders") : names.get(rule.provider_id) ?? `#${rule.provider_id}`
}
