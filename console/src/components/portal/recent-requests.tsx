import { useTranslation } from "react-i18next"
import type { PortalRecentRequestDto } from "@/generated/PortalRecentRequestDto"
import { Badge } from "@/components/ui/badge"
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card"
import { Empty, EmptyDescription, EmptyHeader, EmptyTitle } from "@/components/ui/empty"
import { QueryState } from "@/components/query-state"
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from "@/components/ui/table"
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

  return (
    <Card>
      <CardHeader>
        <CardTitle>{t("portal.recent.title")}</CardTitle>
        <CardDescription>{t("portal.recent.description")}</CardDescription>
      </CardHeader>
      <CardContent>
        <QueryState loading={loading} error={error ? t("portal.recent.loadError") : ""}>
          {requests.length === 0 ? (
            <Empty>
              <EmptyHeader>
                <EmptyTitle>{t("portal.recent.empty")}</EmptyTitle>
                <EmptyDescription>{t("portal.recent.emptyDescription")}</EmptyDescription>
              </EmptyHeader>
            </Empty>
          ) : (
            <Table>
              <TableHeader><TableRow>
                <TableHead>{t("portal.recent.time")}</TableHead>
                <TableHead>{t("portal.recent.request")}</TableHead>
                <TableHead>{t("portal.recent.provider")}</TableHead>
                <TableHead>{t("portal.recent.operation")}</TableHead>
                <TableHead>{t("portal.recent.model")}</TableHead>
                <TableHead>{t("portal.recent.tokens")}</TableHead>
                <TableHead className="text-right">{t("portal.recent.cost")}</TableHead>
              </TableRow></TableHeader>
              <TableBody>{requests.map((request) => (
                <TableRow key={request.request_id}>
                  <TableCell>{formatInstant(request.at, i18n.language)}</TableCell>
                  <TableCell>
                    <div className="flex flex-col gap-1">
                      <code className="font-mono text-xs">{request.request_id}</code>
                      <span className="text-xs text-muted-foreground">
                        {t("portal.recent.latency", { value: formatNumber(request.latency_ms, i18n.language) })}
                      </span>
                    </div>
                  </TableCell>
                  <TableCell>{request.provider_name ?? t("portal.common.unknown")}</TableCell>
                  <TableCell><code className="font-mono text-xs">{request.operation ?? t("portal.common.unknown")}</code></TableCell>
                  <TableCell><code className="font-mono text-xs">{request.upstream_model}</code></TableCell>
                  <TableCell className="font-mono text-xs">
                    {t("portal.recent.tokenPair", {
                      input: formatCount(request.input_tokens, i18n.language),
                      output: formatCount(request.output_tokens, i18n.language),
                    })}
                  </TableCell>
                  <TableCell className="text-right">
                    <div className="flex flex-col items-end gap-1">
                      <span className="font-mono text-xs">{formatCost(request.cost, i18n.language)}</span>
                      <Badge variant={request.ended === "interrupted" ? "destructive" : "outline"}>
                        {t(`portal.recent.ended.${request.ended}`)}
                      </Badge>
                    </div>
                  </TableCell>
                </TableRow>
              ))}</TableBody>
            </Table>
          )}
        </QueryState>
      </CardContent>
    </Card>
  )
}
