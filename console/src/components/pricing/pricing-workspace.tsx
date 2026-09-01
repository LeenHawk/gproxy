import { useMemo, useState } from "react"
import { PlusIcon } from "lucide-react"
import { useTranslation } from "react-i18next"
import type { PriceRateDto } from "@/generated/PriceRateDto"
import type { PriceRuleDto } from "@/generated/PriceRuleDto"
import type { ProviderDto } from "@/generated/ProviderDto"
import { BatchActions } from "@/components/batch-actions"
import { PriceRuleDetail } from "@/components/pricing/price-rule-detail"
import { PriceRuleDialog } from "@/components/pricing/price-rule-dialog"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { Empty, EmptyDescription, EmptyHeader, EmptyTitle } from "@/components/ui/empty"
import { WorkspaceLayout } from "@/components/workspace/workspace-layout"
import { adminPath, navigateAdminPath, useAdminLocation } from "@/lib/admin-route"

type Props = {
  rules: Array<PriceRuleDto>
  rates: Array<PriceRateDto>
  providers: Array<ProviderDto>
  scopeProviderId?: number | null
}

export function PricingWorkspace(props: Props) {
  const { t } = useTranslation()
  const location = useAdminLocation()
  const embedded = typeof props.scopeProviderId === "number"
  const scopedRules = useMemo(
    () => props.rules.filter((rule) => props.scopeProviderId === undefined || rule.provider_id === props.scopeProviderId),
    [props.rules, props.scopeProviderId],
  )
  const providerNames = useMemo(
    () => new Map(props.providers.map((provider) => [provider.id, provider.name])),
    [props.providers],
  )
  const [localSelectedId, setLocalSelectedId] = useState<number | null>(null)
  const [localTab, setLocalTab] = useState("rates")
  const routedId = Number(location.segments[0])
  const selectedId = embedded ? localSelectedId : Number.isFinite(routedId) ? routedId : null
  const selected = scopedRules.find((rule) => rule.id === selectedId) ?? null
  const detailTab = embedded
    ? localTab
    : location.segments[1] === "tiers" || location.segments[1] === "settings"
      ? location.segments[1]
      : "rates"

  return <WorkspaceLayout
    storageKey={embedded ? `gproxy.workspace.provider-${props.scopeProviderId}-pricing.width` : "gproxy.workspace.pricing.width"}
    title={t("pricing.title")}
    items={scopedRules}
    selectedId={selected?.id ?? null}
    getSearchText={(rule) => `${rule.model_pattern} ${providerLabel(rule, providerNames, t)}`}
    renderTitle={(rule) => <span className="font-mono">{rule.model_pattern}</span>}
    renderSummary={(rule) => providerLabel(rule, providerNames, t)}
    renderAction={(rule) => <Badge variant={rule.enabled ? "outline" : "secondary"}>{t(`common.status.${rule.enabled ? "enabled" : "disabled"}`)}</Badge>}
    onSelect={(rule) => embedded ? setLocalSelectedId(rule.id) : navigateAdminPath(`/admin/pricing/${rule.id}/rates`)}
    onBack={() => embedded ? setLocalSelectedId(null) : navigateAdminPath(adminPath("pricing"))}
    searchPlaceholder={t("pricing.rules.search")}
    emptyLabel={t("pricing.rules.empty")}
    resizeLabel={t("pricing.rules.resize")}
    selectAllLabel={t("common.dataTable.selectAll")}
    selectRowLabel={(rule) => `${t("common.dataTable.selectRow")}: ${rule.model_pattern}`}
    selectedLabel={(count) => t("common.dataTable.selected", { count })}
    mobileBackLabel={t("common.actions.back")}
    createAction={<PriceRuleDialog providers={props.providers} fixedProviderId={props.scopeProviderId} trigger={<Button size="icon-sm" aria-label={t("pricing.rules.add")}><PlusIcon aria-hidden /></Button>} />}
    batchActions={(rows, done) => <BatchActions entity="price-rules" rows={rows} queryKeys={["price-rules", "price-rates"]} onApplied={done} size="xs" />}
    emptyState={<Empty><EmptyHeader><EmptyTitle>{t("pricing.title")}</EmptyTitle><EmptyDescription>{t("pricing.rules.selectPrompt")}</EmptyDescription></EmptyHeader></Empty>}
  >
    {selected ? <PriceRuleDetail
      rule={selected}
      rules={scopedRules}
      rates={props.rates.filter((rate) => rate.rule_id === selected.id)}
      providers={props.providers}
      providerNames={providerNames}
      scopeProviderId={props.scopeProviderId}
      tab={detailTab}
      onTab={(tab) => embedded ? setLocalTab(tab) : navigateAdminPath(`/admin/pricing/${selected.id}/${tab}`, true)}
    /> : null}
  </WorkspaceLayout>
}

function providerLabel(rule: PriceRuleDto, names: Map<number, string>, t: (key: string) => string) {
  return rule.provider_id == null ? t("pricing.rules.allProviders") : names.get(rule.provider_id) ?? `#${rule.provider_id}`
}
