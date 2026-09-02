import type { ReactElement } from "react"
import { useState } from "react"
import { useTranslation } from "react-i18next"
import type { PriceRateDto } from "@/generated/PriceRateDto"
import type { PriceRuleDto } from "@/generated/PriceRuleDto"
import type { ProviderDto } from "@/generated/ProviderDto"
import type { ProviderModelDto } from "@/generated/ProviderModelDto"
import { PriceRuleDetail } from "@/components/pricing/price-rule-detail"
import { PriceRuleDialog } from "@/components/pricing/price-rule-dialog"
import { Dialog, DialogBody, DialogContent, DialogHeader, DialogTitle, DialogTrigger } from "@/components/ui/dialog"

export function ProviderModelPricingDialog({ provider, model, rules, rates, trigger }: {
  provider: ProviderDto
  model: ProviderModelDto
  rules: Array<PriceRuleDto>
  rates: Array<PriceRateDto>
  trigger: ReactElement
}) {
  const { t } = useTranslation()
  const [open, setOpen] = useState(false)
  const [tab, setTab] = useState("rates")
  const providerRules = rules.filter((rule) => rule.provider_id === provider.id)
  const exactRule = providerRules.find((rule) => rule.model_pattern === model.model_id)

  if (!exactRule) {
    return <PriceRuleDialog
      providers={[provider]}
      fixedProviderId={provider.id}
      initialPattern={model.model_id}
      lockedPattern
      trigger={trigger}
    />
  }

  return <Dialog open={open} onOpenChange={setOpen}>
    <DialogTrigger asChild>{trigger}</DialogTrigger>
    <DialogContent className="sm:max-w-6xl" closeLabel={t("common.actions.close")}>
      <DialogHeader>
        <DialogTitle>{t("providers.models.priceRuleTitle", { model: model.model_id })}</DialogTitle>
      </DialogHeader>
      <DialogBody>
        <PriceRuleDetail
          rule={exactRule}
          rules={providerRules}
          rates={rates.filter((rate) => rate.rule_id === exactRule.id)}
          providers={[provider]}
          providerNames={new Map([[provider.id, provider.name]])}
          scopeProviderId={provider.id}
          tab={tab}
          onTab={setTab}
          modelId={model.model_id}
        />
      </DialogBody>
    </DialogContent>
  </Dialog>
}
