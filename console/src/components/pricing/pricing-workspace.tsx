import type { PriceRateDto } from "@/generated/PriceRateDto"
import type { PriceRuleDto } from "@/generated/PriceRuleDto"
import type { ProviderDto } from "@/generated/ProviderDto"
import { useMutation, useQueryClient } from "@tanstack/react-query"
import { PencilIcon, PlusIcon, Trash2Icon } from "lucide-react"
import { useMemo } from "react"
import { useTranslation } from "react-i18next"
import { toast } from "sonner"
import { deletePriceRate } from "@/api/control"
import { PriceRateDialog } from "./price-rate-dialog"
import { PriceRuleDialog } from "./price-rule-dialog"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { Card, CardAction, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card"
import { Empty, EmptyContent, EmptyHeader, EmptyTitle } from "@/components/ui/empty"

export function PricingWorkspace({ rules, rates, providers }: {
  rules: Array<PriceRuleDto>
  rates: Array<PriceRateDto>
  providers: Array<ProviderDto>
}) {
  const { t } = useTranslation()
  const queryClient = useQueryClient()
  const providerNames = useMemo(() => new Map(providers.map((provider) => [provider.id, provider.name])), [providers])
  const remove = useMutation({
    mutationFn: deletePriceRate,
    onSuccess: async () => {
      await queryClient.invalidateQueries({ queryKey: ["price-rates"] })
      toast.success(t("pricing.rates.deleted"))
    },
    onError: () => toast.error(t("pricing.rates.deleteError")),
  })
  if (!rules.length) {
    return (
      <Empty>
        <EmptyHeader><EmptyTitle>{t("pricing.rules.empty")}</EmptyTitle></EmptyHeader>
        <EmptyContent><PriceRuleDialog providers={providers} trigger={<Button><PlusIcon data-icon="inline-start" />{t("pricing.rules.add")}</Button>} /></EmptyContent>
      </Empty>
    )
  }
  return (
    <div className="flex flex-col gap-4">
      <div className="flex justify-end"><PriceRuleDialog providers={providers} trigger={<Button><PlusIcon data-icon="inline-start" />{t("pricing.rules.add")}</Button>} /></div>
      {rules.map((rule) => {
        const ruleRates = rates.filter((rate) => rate.rule_id === rule.id)
        return (
          <Card key={rule.id}>
            <CardHeader>
              <CardTitle className="font-mono">{rule.model_pattern}</CardTitle>
              <CardDescription>{rule.provider_id == null ? t("pricing.rules.allProviders") : providerNames.get(rule.provider_id) ?? rule.provider_id}</CardDescription>
              <CardAction className="flex items-center gap-2">
                <Badge variant={rule.enabled ? "outline" : "secondary"}>{t(`common.status.${rule.enabled ? "enabled" : "disabled"}`)}</Badge>
                <PriceRuleDialog rule={rule} providers={providers} trigger={<Button size="sm" variant="outline"><PencilIcon data-icon="inline-start" />{t("common.actions.edit")}</Button>} />
              </CardAction>
            </CardHeader>
            <CardContent className="flex flex-col gap-3">
              <div className="flex items-center justify-between gap-2">
                <h3 className="text-sm font-medium">{t("pricing.rates.title")}</h3>
                <PriceRateDialog rules={rules} initialRuleId={rule.id} trigger={<Button size="sm" variant="outline"><PlusIcon data-icon="inline-start" />{t("pricing.rates.add")}</Button>} />
              </div>
              {ruleRates.length ? ruleRates.map((rate) => (
                <div key={rate.id} className="grid gap-2 rounded-lg border p-3 sm:grid-cols-[1fr_auto_auto_auto] sm:items-center">
                  <div><p className="font-mono text-sm">{rate.metric}</p><p className="text-xs text-muted-foreground">{t("pricing.rates.summary", { price: rate.price, units: rate.unit_size })}</p></div>
                  <Badge variant="outline">{t("pricing.rates.priorityValue", { value: rate.priority })}</Badge>
                  <PriceRateDialog rate={rate} rules={rules} trigger={<Button size="sm" variant="outline">{t("common.actions.edit")}</Button>} />
                  <Button size="icon-sm" variant="ghost" aria-label={t("common.actions.delete")} disabled={remove.isPending} onClick={() => remove.mutate(rate.id)}><Trash2Icon /></Button>
                </div>
              )) : <p className="text-sm text-muted-foreground">{t("pricing.rates.empty")}</p>}
            </CardContent>
          </Card>
        )
      })}
    </div>
  )
}
