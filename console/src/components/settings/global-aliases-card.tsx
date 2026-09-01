import { useQuery } from "@tanstack/react-query"
import { useTranslation } from "react-i18next"
import { aliases } from "@/api/control"
import { QueryState } from "@/components/query-state"
import { RoutingAliases } from "@/components/routes/routing-aliases"

export function GlobalAliasesCard() {
  const { t } = useTranslation()
  const query = useQuery({ queryKey: ["aliases"], queryFn: aliases })
  return <QueryState loading={query.isLoading} error={query.isError ? t("common.loadError") : ""}><RoutingAliases aliases={query.data ?? []} providers={[]} scopeProviderId={null} onChanged={() => void query.refetch()} /></QueryState>
}
