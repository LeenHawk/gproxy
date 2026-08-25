import { useQueries } from "@tanstack/react-query"
import { useTranslation } from "react-i18next"
import {
  aliases,
  credentials,
  modelAliases,
  providers,
  routeMembers,
  routes,
} from "@/api/control"
import { PageLayout } from "@/components/page-layout"
import { QueryState } from "@/components/query-state"
import { RoutesWorkspace } from "@/components/routes/routes-workspace"

export function RoutesPage() {
  const { t } = useTranslation()
  const [routeQuery, memberQuery, providerQuery, credentialQuery, aliasQuery, modelAliasQuery] = useQueries({
    queries: [
      { queryKey: ["routes"], queryFn: routes },
      { queryKey: ["route-members"], queryFn: routeMembers },
      { queryKey: ["providers"], queryFn: providers },
      { queryKey: ["credentials"], queryFn: credentials },
      { queryKey: ["aliases"], queryFn: aliases },
      { queryKey: ["model-aliases"], queryFn: modelAliases },
    ],
  })
  const queries = [routeQuery, memberQuery, providerQuery, credentialQuery, aliasQuery, modelAliasQuery]

  return (
    <PageLayout title={t("routes.title")} description={t("routes.subtitle")}>
      <QueryState
        loading={queries.some((query) => query.isLoading)}
        error={queries.some((query) => query.error) ? t("routes.loadError") : ""}
      >
        <RoutesWorkspace
          routes={routeQuery.data ?? []}
          members={memberQuery.data ?? []}
          providers={providerQuery.data ?? []}
          credentials={credentialQuery.data ?? []}
          routingAliases={aliasQuery.data ?? []}
          modelAliases={modelAliasQuery.data ?? []}
          onRoutesChanged={() => void routeQuery.refetch()}
          onMembersChanged={() => void memberQuery.refetch()}
          onRoutingAliasesChanged={() => void aliasQuery.refetch()}
          onModelAliasesChanged={() => void modelAliasQuery.refetch()}
        />
      </QueryState>
    </PageLayout>
  )
}
