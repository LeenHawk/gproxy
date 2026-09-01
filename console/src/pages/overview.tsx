import { useQueries } from "@tanstack/react-query"
import { useTranslation } from "react-i18next"
import { credentials, providers } from "@/api/control"
import { credentialCycles, quotaWindows, usage } from "@/api/observability"
import { OverviewDashboard } from "@/components/overview/overview-dashboard"
import { PageLayout } from "@/components/page-layout"
import { QueryState } from "@/components/query-state"
import { useNow } from "@/lib/use-now"

export function OverviewPage() {
  const { t } = useTranslation()
  const now = useNow()
  const [providerQuery, credentialQuery, usageQuery, quotaQuery, cycleQuery] = useQueries({ queries: [
    { queryKey: ["providers"], queryFn: providers },
    { queryKey: ["credentials"], queryFn: credentials },
    { queryKey: ["usage", "provider", now - 86_400, now], queryFn: () => usage({ from: now - 86_400, to: now, group_by: "provider", user_key_id: null, user_id: null, provider_id: null, credential_id: null, model: null }) },
    { queryKey: ["quota-windows"], queryFn: () => quotaWindows() },
    { queryKey: ["credential-cycles", now - 604_800, now], queryFn: () => credentialCycles(now - 604_800, now) },
  ] })
  const queries = [providerQuery, credentialQuery, usageQuery, quotaQuery, cycleQuery]
  const error = queries.find((query) => query.error)?.error
  return (
    <PageLayout title={t("nav.overview")} description={t("usage.overviewDescription")}>
      <QueryState loading={queries.some((query) => query.isLoading)} error={error ? t("common.loadError") : ""}>
        <OverviewDashboard providers={providerQuery.data ?? []} credentials={credentialQuery.data ?? []} usage={usageQuery.data ?? []} quotas={quotaQuery.data ?? []} cycles={cycleQuery.data ?? []} />
      </QueryState>
    </PageLayout>
  )
}
