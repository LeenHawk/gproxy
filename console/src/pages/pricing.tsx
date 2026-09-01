import { useQueries } from "@tanstack/react-query"
import { useTranslation } from "react-i18next"
import { priceRates, priceRules, providers } from "@/api/control"
import { PageLayout } from "@/components/page-layout"
import { PricingWorkspace } from "@/components/pricing/pricing-workspace"
import { QueryState } from "@/components/query-state"

export function PricingPage() {
  const { t } = useTranslation()
  const [ruleQuery, rateQuery, providerQuery] = useQueries({ queries: [
    { queryKey: ["price-rules"], queryFn: priceRules },
    { queryKey: ["price-rates"], queryFn: priceRates },
    { queryKey: ["providers"], queryFn: providers },
  ] })
  const queries = [ruleQuery, rateQuery, providerQuery]
  return (
    <PageLayout title={t("pricing.title")} description={t("pricing.subtitle")}>
      <QueryState loading={queries.some((query) => query.isLoading)} error={queries.some((query) => query.error) ? t("pricing.loadError") : ""}>
        <PricingWorkspace rules={ruleQuery.data ?? []} rates={rateQuery.data ?? []} providers={providerQuery.data ?? []} scopeProviderId={null} />
      </QueryState>
    </PageLayout>
  )
}
