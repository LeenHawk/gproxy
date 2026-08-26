import { useQueries } from "@tanstack/react-query"
import { useTranslation } from "react-i18next"
import { providerRuleSets, providers, ruleSets, rules } from "@/api/control"
import { PageLayout } from "@/components/page-layout"
import { QueryState } from "@/components/query-state"
import { RulesWorkspace } from "@/components/rules/rules-workspace"
import { useRuleMutations } from "@/components/rules/use-rule-mutations"

export function RulesPage() {
  const { t } = useTranslation()
  const mutations = useRuleMutations()
  const [setQuery, ruleQuery, attachmentQuery, providerQuery] = useQueries({ queries: [
    { queryKey: ["rule-sets"], queryFn: ruleSets },
    { queryKey: ["rules"], queryFn: rules },
    { queryKey: ["provider-rule-sets"], queryFn: providerRuleSets },
    { queryKey: ["providers"], queryFn: providers },
  ] })
  const queries = [setQuery, ruleQuery, attachmentQuery, providerQuery]
  return <PageLayout title={t("rules.title")} description={t("rules.subtitle")}><QueryState loading={queries.some((query) => query.isLoading)} error={queries.some((query) => query.isError) ? t("rules.loadError") : ""}><RulesWorkspace ruleSets={setQuery.data ?? []} rules={ruleQuery.data ?? []} attachments={attachmentQuery.data ?? []} providers={providerQuery.data ?? []} mutations={mutations} /></QueryState></PageLayout>
}
