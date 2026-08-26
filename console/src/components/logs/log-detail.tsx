import { useMemo } from "react"
import { useTranslation } from "react-i18next"
import type { LogDetailDto } from "@/generated/LogDetailDto"
import type { ProviderDto } from "@/generated/ProviderDto"
import { Exchange } from "@/components/logs/exchange"
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card"
import { QueryState } from "@/components/query-state"

export function LogDetail({ value, loading, error, providers }: { value: LogDetailDto | null; loading: boolean; error: boolean; providers: Array<ProviderDto> }) {
  const { t } = useTranslation()
  const providerNames = useMemo(() => new Map(providers.map((provider) => [provider.id, provider.name])), [providers])
  if (!value && !loading && !error) {
    return <Card size="sm"><CardHeader><CardTitle>{t("logs.detail.title")}</CardTitle></CardHeader><CardContent><p className="py-10 text-center text-sm text-muted-foreground">{t("logs.detail.select")}</p></CardContent></Card>
  }
  return (
    <QueryState loading={loading} error={error ? t("logs.detail.loadError") : ""}>
      {value ? (
        <div className="grid min-w-0 gap-4 lg:grid-cols-2">
          <Exchange
            title={t("logs.detail.downstream")}
            subtitle={value.downstream.request_id}
            method={value.downstream.method}
            target={`${value.downstream.path}${value.downstream.query ? `?${value.downstream.query}` : ""}`}
            requestHeaders={value.downstream.request_headers}
            requestBody={value.downstream.request_body}
            status={value.downstream.response_status}
            responseHeaders={value.downstream.response_headers}
            responseBody={value.downstream.response_body}
          />
          <div className="flex min-w-0 flex-col gap-4">
            {value.upstream.length === 0 ? <Card size="sm"><CardHeader><CardTitle>{t("logs.detail.upstream")}</CardTitle></CardHeader><CardContent><p className="text-sm text-muted-foreground">{t("logs.detail.noAttempts")}</p></CardContent></Card> : value.upstream.map((attempt, index) => (
              <Exchange
                key={attempt.id}
                title={t("logs.detail.attempt", { number: index + 1 })}
                subtitle={attempt.provider_id == null ? t("logs.detail.unknownProvider") : providerNames.get(attempt.provider_id) ?? `#${attempt.provider_id}`}
                method={attempt.request_method}
                target={attempt.upstream_url ?? t("common.none")}
                requestHeaders={attempt.request_headers}
                requestBody={attempt.request_body}
                status={attempt.response_status}
                responseHeaders={attempt.response_headers}
                responseBody={attempt.response_body}
              />
            ))}
          </div>
        </div>
      ) : null}
    </QueryState>
  )
}
