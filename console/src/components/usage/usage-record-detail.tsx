import { useQuery } from "@tanstack/react-query"
import { useTranslation } from "react-i18next"
import type { UsageRecordDto } from "@/generated/UsageRecordDto"
import type { ProviderDto } from "@/generated/ProviderDto"
import { logDetail } from "@/api/observability"
import { LogDetail } from "@/components/logs/log-detail"
import { Sheet, SheetContent, SheetDescription, SheetHeader, SheetTitle } from "@/components/ui/sheet"
import { formatCost, formatInstant, formatTokensPerSecond } from "@/lib/format"

export function UsageRecordDetail({ record, onClose, providers }: { record: UsageRecordDto | null; onClose: () => void; providers: Array<ProviderDto> }) {
  const { t, i18n } = useTranslation()
  const detail = useQuery({ queryKey: ["log-detail", record?.request_id], queryFn: () => logDetail(record!.request_id), enabled: record != null, retry: false })
  return <Sheet open={record != null} onOpenChange={(open) => { if (!open) onClose() }}>
    <SheetContent className="overflow-y-auto sm:max-w-3xl">
      <SheetHeader><SheetTitle>{t("usage.record.detail")}</SheetTitle><SheetDescription className="break-all font-mono">{record?.request_id}</SheetDescription></SheetHeader>
      {record ? <div className="grid gap-5 px-4 pb-6">
        <div className="flex justify-between gap-4"><span className="break-all font-mono">{record.model}</span><strong>{formatCost(record.cost, i18n.language)}</strong></div>
        <dl className="grid gap-2 text-sm">
          {[[t("usage.record.time"), formatInstant(record.at, i18n.language)], [t("usage.record.latency"), `${record.latency_ms} ms`], [t("usage.record.tps"), formatTokensPerSecond(record.output_tokens, record.latency_ms, i18n.language)], [t("usage.inputTokens"), record.input_tokens], [t("usage.outputTokens"), record.output_tokens], [t("usage.cachedTokens"), record.cached_input_tokens], ...Object.entries(record.metrics), ...Object.entries(record.dimensions)].map(([key, value]) => <div key={key} className="flex justify-between gap-4"><dt className="break-all text-muted-foreground">{key}</dt><dd className="break-all font-mono">{value}</dd></div>)}
        </dl>
        <p className="text-xs text-muted-foreground">{t("usage.record.tpsHint")}</p>
        {detail.data ? <LogDetail value={detail.data} loading={false} error={false} providers={providers} /> : <p className="text-sm text-muted-foreground">{detail.isFetching ? t("common.loading") : t("usage.record.noLogs")}</p>}
      </div> : null}
    </SheetContent>
  </Sheet>
}
