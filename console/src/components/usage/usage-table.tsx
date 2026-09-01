import { useMemo } from "react"
import { useTranslation } from "react-i18next"
import type { UsageStatisticsDto } from "@/generated/UsageStatisticsDto"
import type { ProviderDto } from "@/generated/ProviderDto"
import type { UserDto } from "@/generated/UserDto"
import type { UserKeyDto } from "@/generated/UserKeyDto"
import { DataTable, type DataTableColumn } from "@/components/data-table"
import { formatCost, formatCount } from "@/lib/format"

function Metric({ label, value, locale }: { label: string; value: number; locale: string }) {
  return <div><dt className="text-muted-foreground">{label}</dt><dd className="font-mono">{formatCount(value, locale)}</dd></div>
}

export function UsageTable({ rows, providers, users, keys }: { rows: Array<UsageStatisticsDto>; providers: Array<ProviderDto>; users: Array<UserDto>; keys: Array<UserKeyDto> }) {
  const { t, i18n } = useTranslation()
  const providerNames = useMemo(() => new Map(providers.map((provider) => [provider.id, provider.name])), [providers])
  const userNames = useMemo(() => new Map(users.map((user) => [user.id, user.name])), [users])
  const keyNames = useMemo(() => new Map(keys.map((key) => [key.id, key.label ?? key.prefix ?? String(key.id)])), [keys])
  const userName = (id: number | null) => id == null ? t("common.none") : userNames.get(id) ?? String(id)
  const keyName = (id: number | null) => id == null ? t("common.none") : keyNames.get(id) ?? String(id)
  const providerName = (id: number | null) => id == null ? t("common.none") : providerNames.get(id) ?? String(id)
  const modelName = (model: string | null) => model ?? t("common.none")
  const columns: Array<DataTableColumn<UsageStatisticsDto>> = [
    { key: "key", label: t("usage.filters.key"), header: t("usage.filters.key"), cell: (row) => <span className="text-xs">{keyName(row.user_key_id)}</span> },
    { key: "user", label: t("usage.filters.user"), header: t("usage.filters.user"), cell: (row) => <span className="text-xs">{userName(row.user_id)}</span> },
    { key: "provider", label: t("usage.filters.provider"), header: t("usage.filters.provider"), cell: (row) => <span className="text-xs">{providerName(row.provider_id)}</span> },
    { key: "model", label: t("usage.filters.model"), header: t("usage.filters.model"), cell: (row) => <span className="font-mono text-xs">{modelName(row.model)}</span> },
    { key: "requests", label: t("usage.requests"), header: t("usage.requests"), cell: (row) => <span className="font-mono text-xs">{formatCount(row.requests, i18n.language)}</span>, className: "text-right" },
    { key: "input", label: t("usage.inputTokens"), header: t("usage.inputTokens"), cell: (row) => <span className="font-mono text-xs">{formatCount(row.input_tokens, i18n.language)}</span>, className: "text-right" },
    { key: "cached", label: t("usage.cachedTokens"), header: t("usage.cachedTokens"), cell: (row) => <span className="font-mono text-xs">{formatCount(row.cached_input_tokens, i18n.language)}</span>, className: "text-right" },
    { key: "cache5m", label: t("usage.cacheCreation5m"), header: t("usage.cacheCreation5m"), cell: (row) => <span className="font-mono text-xs">{formatCount(row.cache_creation_5m_tokens, i18n.language)}</span>, className: "text-right" },
    { key: "cache30m", label: t("usage.cacheCreation30m"), header: t("usage.cacheCreation30m"), cell: (row) => <span className="font-mono text-xs">{formatCount(row.cache_creation_30m_tokens, i18n.language)}</span>, className: "text-right" },
    { key: "cache1h", label: t("usage.cacheCreation1h"), header: t("usage.cacheCreation1h"), cell: (row) => <span className="font-mono text-xs">{formatCount(row.cache_creation_1h_tokens, i18n.language)}</span>, className: "text-right" },
    { key: "output", label: t("usage.outputTokens"), header: t("usage.outputTokens"), cell: (row) => <span className="font-mono text-xs">{formatCount(row.output_tokens, i18n.language)}</span>, className: "text-right" },
    { key: "cost", label: t("usage.cost.label"), header: t("usage.cost.label"), cell: (row) => <span className="font-mono text-xs">{formatCost(row.cost, i18n.language)}</span>, className: "text-right" },
  ]
  return (
    <DataTable columns={columns} rows={rows} rowKey={(row) => JSON.stringify([row.user_key_id, row.user_id, row.provider_id, row.model])} searchText={(row) => `${keyName(row.user_key_id)} ${userName(row.user_id)} ${providerName(row.provider_id)} ${modelName(row.model)}`} renderCard={(row) => <div className="flex flex-col gap-3"><div className="flex items-start justify-between gap-3"><div><p className="font-mono text-xs">{modelName(row.model)}</p><p className="text-xs text-muted-foreground">{providerName(row.provider_id)}</p></div><p className="font-mono text-sm">{formatCost(row.cost, i18n.language)}</p></div><dl className="grid grid-cols-2 gap-2 text-xs"><Metric label={t("usage.requests")} value={row.requests} locale={i18n.language} /><Metric label={t("usage.inputTokens")} value={row.input_tokens} locale={i18n.language} /><Metric label={t("usage.cachedTokens")} value={row.cached_input_tokens} locale={i18n.language} /><Metric label={t("usage.cacheCreation5m")} value={row.cache_creation_5m_tokens} locale={i18n.language} /><Metric label={t("usage.cacheCreation30m")} value={row.cache_creation_30m_tokens} locale={i18n.language} /><Metric label={t("usage.cacheCreation1h")} value={row.cache_creation_1h_tokens} locale={i18n.language} /><Metric label={t("usage.outputTokens")} value={row.output_tokens} locale={i18n.language} /></dl><p className="text-xs text-muted-foreground">{keyName(row.user_key_id)} · {userName(row.user_id)}</p></div>} empty={t("usage.empty")} storageKey="usage" />
  )
}
