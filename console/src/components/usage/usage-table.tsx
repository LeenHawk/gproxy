import { useMemo } from "react"
import { useTranslation } from "react-i18next"
import type { UsageAggregateDto } from "@/generated/UsageAggregateDto"
import type { UsageGroupByDto } from "@/generated/UsageGroupByDto"
import type { ProviderDto } from "@/generated/ProviderDto"
import type { UserDto } from "@/generated/UserDto"
import type { UserKeyDto } from "@/generated/UserKeyDto"
import { DataTable, type DataTableColumn } from "@/components/data-table"
import { formatCost, formatCount } from "@/lib/format"
import { cn } from "@/lib/utils"

export function UsageTable({ rows, group, providers, users, keys }: { rows: Array<UsageAggregateDto>; group: UsageGroupByDto; providers: Array<ProviderDto>; users: Array<UserDto>; keys: Array<UserKeyDto> }) {
  const { t, i18n } = useTranslation()
  const providerNames = useMemo(() => new Map(providers.map((provider) => [provider.id, provider.name])), [providers])
  const userNames = useMemo(() => new Map(users.map((user) => [user.id, user.name])), [users])
  const keyNames = useMemo(() => new Map(keys.map((key) => [key.id, key.label ?? key.prefix ?? String(key.id)])), [keys])
  const label = (value: string) => {
    const id = Number(value)
    if (group === "provider") return providerNames.get(id) ?? value
    if (group === "user") return userNames.get(id) ?? value
    if (group === "user_key") return keyNames.get(id) ?? value
    return value
  }
  const columns: Array<DataTableColumn<UsageAggregateDto>> = [
    { key: "group", label: t("usage.group"), header: t("usage.group"), cell: (row) => { const groupLabel = label(row.group); return <div className={cn("text-sm", group === "model" && "font-mono text-xs")}>{groupLabel}{groupLabel !== row.group ? <p className="font-mono text-xs text-muted-foreground">{row.group}</p> : null}</div> } },
    { key: "requests", label: t("usage.requests"), header: t("usage.requests"), cell: (row) => <span className="font-mono text-xs">{formatCount(row.requests, i18n.language)}</span>, className: "text-right" },
    { key: "input", label: t("usage.inputTokens"), header: t("usage.inputTokens"), cell: (row) => <span className="font-mono text-xs">{formatCount(row.input_tokens, i18n.language)}</span>, className: "text-right" },
    { key: "cached", label: t("usage.cachedTokens"), header: t("usage.cachedTokens"), cell: (row) => <span className="font-mono text-xs">{formatCount(row.cached_input_tokens, i18n.language)}</span>, className: "text-right" },
    { key: "output", label: t("usage.outputTokens"), header: t("usage.outputTokens"), cell: (row) => <span className="font-mono text-xs">{formatCount(row.output_tokens, i18n.language)}</span>, className: "text-right" },
    { key: "cost", label: t("usage.cost.label"), header: t("usage.cost.label"), cell: (row) => <span className="font-mono text-xs">{formatCost(row.cost, i18n.language)}</span>, className: "text-right" },
  ]
  return (
    <DataTable columns={columns} rows={rows} rowKey={(row) => row.group} searchText={(row) => `${row.group} ${label(row.group)}`} renderCard={(row) => <div className="flex flex-col gap-3"><div className="flex items-start justify-between gap-3"><p className={cn("font-medium", group === "model" && "font-mono text-xs")}>{label(row.group)}</p><p className="font-mono text-sm">{formatCost(row.cost, i18n.language)}</p></div><dl className="grid grid-cols-2 gap-2 text-xs"><div><dt className="text-muted-foreground">{t("usage.requests")}</dt><dd className="font-mono">{formatCount(row.requests, i18n.language)}</dd></div><div><dt className="text-muted-foreground">{t("usage.inputTokens")}</dt><dd className="font-mono">{formatCount(row.input_tokens, i18n.language)}</dd></div><div><dt className="text-muted-foreground">{t("usage.cachedTokens")}</dt><dd className="font-mono">{formatCount(row.cached_input_tokens, i18n.language)}</dd></div><div><dt className="text-muted-foreground">{t("usage.outputTokens")}</dt><dd className="font-mono">{formatCount(row.output_tokens, i18n.language)}</dd></div></dl></div>} empty={t("usage.empty")} storageKey="usage" />
  )
}
