import { useMemo, useState } from "react"
import { useTranslation } from "react-i18next"
import type { UsageRecordDto } from "@/generated/UsageRecordDto"
import type { UsageRecordPageDto } from "@/generated/UsageRecordPageDto"
import type { ProviderDto } from "@/generated/ProviderDto"
import type { CredentialDto } from "@/generated/CredentialDto"
import type { UserDto } from "@/generated/UserDto"
import type { UserKeyDto } from "@/generated/UserKeyDto"
import { DataTable, type DataTableColumn } from "@/components/data-table"
import type { PageSize } from "@/components/data-table-pagination"
import { UsageRecordDetail } from "@/components/usage/usage-record-detail"
import { formatCost, formatCount, formatInstant, formatTokensPerSecond } from "@/lib/format"

type Props = {
  page: UsageRecordPageDto
  providers: Array<ProviderDto>
  credentials: Array<CredentialDto>
  users: Array<UserDto>
  keys: Array<UserKeyDto>
  pending: boolean
  onPage: (page: number) => void
  onPageSize: (size: PageSize) => void
}

export function UsageTable({ page, providers, credentials, users, keys, pending, onPage, onPageSize }: Props) {
  const { t, i18n } = useTranslation()
  const [selected, setSelected] = useState<UsageRecordDto | null>(null)
  const names = useMemo(() => ({
    providers: new Map(providers.map((value) => [value.id, value.name])),
    credentials: new Map(credentials.map((value) => [value.id, value.label ?? `#${value.id}`])),
    users: new Map(users.map((value) => [value.id, value.name])),
    keys: new Map(keys.map((value) => [value.id, value.label ?? value.prefix ?? `#${value.id}`])),
  }), [providers, credentials, users, keys])
  const name = (kind: keyof typeof names, id: number | null) => id == null ? "—" : names[kind].get(id) ?? `#${id}`
  const count = (value: number | string) => <span className="font-mono text-xs tabular-nums">{formatCount(Number(value), i18n.language)}</span>
  const columns: Array<DataTableColumn<UsageRecordDto>> = [
    { key: "at", label: t("usage.record.time"), header: t("usage.record.time"), cell: (row) => <span className="whitespace-nowrap text-xs">{formatInstant(row.at, i18n.language)}</span> },
    { key: "latency", label: t("usage.record.latency"), header: t("usage.record.latency"), cell: (row) => <span className="whitespace-nowrap font-mono text-xs tabular-nums">{row.latency_ms} ms</span> },
    { key: "tps", label: t("usage.record.tps"), header: <span title={t("usage.record.tpsHint")}>{t("usage.record.tps")}</span>, cell: (row) => <span className="font-mono text-xs tabular-nums" title={t("usage.record.tpsHint")}>{formatTokensPerSecond(row.output_tokens, row.latency_ms, i18n.language)}</span> },
    { key: "request", label: t("usage.record.requestId"), header: t("usage.record.requestId"), cell: (row) => <span className="block max-w-40 truncate font-mono text-xs" title={row.request_id}>{row.request_id}</span> },
    { key: "model", label: t("usage.filters.model"), header: t("usage.filters.model"), cell: (row) => <span className="font-mono text-xs">{row.model}</span> },
    ...(["provider", "credential", "user", "key"] as const).map((key) => ({ key, label: t(`usage.filters.${key}`), header: t(`usage.filters.${key}`), cell: (row: UsageRecordDto) => name(`${key}s` as keyof typeof names, row[key === "key" ? "user_key_id" : `${key}_id` as const]) })),
    { key: "input", label: t("usage.inputTokens"), header: t("usage.inputTokens"), cell: (row) => count(row.input_tokens) },
    { key: "output", label: t("usage.outputTokens"), header: t("usage.outputTokens"), cell: (row) => count(row.output_tokens) },
    { key: "cached", label: t("usage.cachedTokens"), header: t("usage.cachedTokens"), cell: (row) => count(row.cached_input_tokens) },
    ...(["5m", "30m", "1h"] as const).map((duration) => ({ key: `cache${duration}`, label: t(`usage.cacheCreation${duration}`), header: t(`usage.cacheCreation${duration}`), cell: (row: UsageRecordDto) => count(row.metrics[`cache_creation_${duration}_tokens`] ?? 0) })),
    { key: "cost", label: t("usage.cost.label"), header: "USD", cell: (row) => <span className="font-mono tabular-nums">{formatCost(row.cost, i18n.language)}</span> },
    { key: "source", label: t("usage.record.source"), header: t("usage.record.source"), cell: (row) => t(`usage.record.${row.usage_source}`) },
    { key: "ended", label: t("usage.record.ended"), header: t("usage.record.ended"), cell: (row) => t(`usage.record.${row.ended}`) },
  ]
  return <div aria-busy={pending}>
    <DataTable columns={columns} rows={page.items} rowKey={(row) => row.id} searchText={(row) => row.request_id}
      renderCard={(row) => <div className="grid gap-2 text-xs"><div className="flex justify-between gap-3"><span className="break-all font-mono">{row.model}</span><strong className="tabular-nums">{formatCost(row.cost, i18n.language)}</strong></div><p>{t("usage.record.time")}: {formatInstant(row.at, i18n.language)} · {name("providers", row.provider_id)}</p><p className="break-all font-mono">{row.request_id}</p><p>{t("usage.inputTokens")}: {row.input_tokens} · {t("usage.outputTokens")}: {row.output_tokens}</p><p>{t("usage.record.latency")}: {row.latency_ms} ms · <span title={t("usage.record.tpsHint")}>{t("usage.record.tps")}: {formatTokensPerSecond(row.output_tokens, row.latency_ms, i18n.language)}</span></p><p>{t(`usage.record.${row.usage_source}`)} · {t(`usage.record.${row.ended}`)}</p></div>}
      onRowClick={setSelected} empty={t("usage.empty")} storageKey="usage-records"
      pagination={{ page: page.page, pageSize: page.page_size as PageSize, total: page.total, onPage: (next) => { if (!pending) onPage(next) }, onPageSize }} />
    <UsageRecordDetail record={selected} onClose={() => setSelected(null)} providers={providers} />
  </div>
}
