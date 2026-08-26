import { useTranslation } from "react-i18next"
import type { PortalRecentRequestDto } from "@/generated/PortalRecentRequestDto"
import { DataTable, type DataTableColumn } from "@/components/data-table"
import { Badge } from "@/components/ui/badge"
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card"
import { QueryState } from "@/components/query-state"
import { formatCost, formatCount, formatInstant, formatNumber } from "@/lib/format"

export function RecentRequests({
  requests,
  loading,
  error,
}: {
  requests: Array<PortalRecentRequestDto>
  loading: boolean
  error: boolean
}) {
  const { t, i18n } = useTranslation()
  const outcome = (request: PortalRecentRequestDto) => <div className="flex flex-col items-end gap-1"><span className="font-mono text-xs">{formatCost(request.cost, i18n.language)}</span><Badge variant={request.ended === "interrupted" ? "destructive" : "outline"}>{t(`portal.recent.ended.${request.ended}`)}</Badge></div>
  const columns: Array<DataTableColumn<PortalRecentRequestDto>> = [
    { key: "time", label: t("portal.recent.time"), header: t("portal.recent.time"), cell: (request) => formatInstant(request.at, i18n.language) },
    { key: "request", label: t("portal.recent.request"), header: t("portal.recent.request"), cell: (request) => <div><code className="font-mono text-xs">{request.request_id}</code><p className="text-xs text-muted-foreground">{t("portal.recent.latency", { value: formatNumber(request.latency_ms, i18n.language) })}</p></div> },
    { key: "provider", label: t("portal.recent.provider"), header: t("portal.recent.provider"), cell: (request) => request.provider_name ?? t("portal.common.unknown") },
    { key: "operation", label: t("portal.recent.operation"), header: t("portal.recent.operation"), cell: (request) => <code className="font-mono text-xs">{request.operation ?? t("portal.common.unknown")}</code> },
    { key: "model", label: t("portal.recent.model"), header: t("portal.recent.model"), cell: (request) => <code className="font-mono text-xs">{request.upstream_model}</code> },
    { key: "tokens", label: t("portal.recent.tokens"), header: t("portal.recent.tokens"), cell: (request) => <span className="font-mono text-xs">{t("portal.recent.tokenPair", { input: formatCount(request.input_tokens, i18n.language), output: formatCount(request.output_tokens, i18n.language) })}</span> },
    { key: "cost", label: t("portal.recent.cost"), header: t("portal.recent.cost"), cell: outcome, className: "text-right" },
  ]

  return (
    <Card>
      <CardHeader>
        <CardTitle>{t("portal.recent.title")}</CardTitle>
        <CardDescription>{t("portal.recent.description")}</CardDescription>
      </CardHeader>
      <CardContent>
        <QueryState loading={loading} error={error ? t("portal.recent.loadError") : ""}>
          <DataTable columns={columns} rows={requests} rowKey={(request) => request.request_id} searchText={(request) => `${request.request_id} ${request.provider_name ?? ""} ${request.operation ?? ""} ${request.upstream_model}`} renderCard={(request) => <div className="flex flex-col gap-3"><div className="flex items-start justify-between gap-3"><div><p className="font-mono text-xs">{request.request_id}</p><p className="text-xs text-muted-foreground">{formatInstant(request.at, i18n.language)}</p></div>{outcome(request)}</div><p className="text-sm">{request.provider_name ?? t("portal.common.unknown")} · <span className="font-mono text-xs">{request.upstream_model}</span></p><p className="font-mono text-xs text-muted-foreground">{t("portal.recent.tokenPair", { input: formatCount(request.input_tokens, i18n.language), output: formatCount(request.output_tokens, i18n.language) })}</p></div>} empty={t("portal.recent.empty")} storageKey="portal-recent" />
        </QueryState>
      </CardContent>
    </Card>
  )
}
