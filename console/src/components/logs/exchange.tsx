import { CopyIcon } from "lucide-react"
import { useTranslation } from "react-i18next"
import { toast } from "sonner"
import { BodyView } from "@/components/logs/body-view"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { Card, CardAction, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card"
import { copyText } from "@/lib/copy-text"
import { formattedLogContent } from "@/lib/log-content"

type Metric = { label: string; value: string }
const EMPTY_METRICS: Array<Metric> = []

type Props = {
  title: string
  subtitle: string
  method: string | null
  target: string
  requestHeaders: Record<string, string> | null
  requestBody: string | null
  status: number | null
  responseHeaders: Record<string, string> | null
  responseBody: string | null
  metrics?: Array<Metric>
}

function Section({ title, value }: { title: string; value: string | null }) {
  const { t } = useTranslation()
  const display = value == null ? null : formattedLogContent(value)

  async function copy() {
    if (display == null) return
    try {
      await copyText(display)
      toast.success(t("logs.detail.copied", { label: title }))
    } catch {
      toast.error(t("logs.detail.copyError", { label: title }))
    }
  }

  return (
    <section className="flex min-w-0 flex-col gap-2">
      <div className="flex items-center justify-between gap-2">
        <h4 className="text-xs font-semibold tracking-wide text-muted-foreground uppercase">{title}</h4>
        <Button type="button" size="icon-xs" variant="ghost" disabled={display == null} aria-label={t("logs.detail.copy", { label: title })} onClick={() => void copy()}>
          <CopyIcon />
        </Button>
      </div>
      <BodyView value={value} />
    </section>
  )
}

function headers(value: Record<string, string> | null) {
  return value == null ? null : JSON.stringify(value)
}

function statusVariant(status: number | null) {
  if (status == null) return "secondary"
  if (status >= 400) return "destructive"
  if (status >= 300) return "warning"
  return "success"
}

export function Exchange({ title, subtitle, method, target, requestHeaders, requestBody, status, responseHeaders, responseBody, metrics = EMPTY_METRICS }: Props) {
  const { t } = useTranslation()
  return (
    <Card size="sm" className="min-w-0">
      <CardHeader>
        <CardTitle headingLevel={3}>{title}</CardTitle>
        <CardDescription className="machine-text break-all">{subtitle}</CardDescription>
        <CardAction>
          <Badge variant={statusVariant(status)} aria-label={`${t("logs.detail.status")}: ${status ?? t("logs.pending")}`}>{status ?? t("logs.pending")}</Badge>
        </CardAction>
      </CardHeader>
      <CardContent className="flex min-w-0 flex-col gap-5">
        <p className="machine-text break-all text-sm"><span className="font-semibold">{method ?? t("common.none")}</span> {target}</p>
        {metrics.length ? <dl className="flex flex-wrap gap-x-5 gap-y-2 text-xs">{metrics.map((metric) => <div key={metric.label} className="flex gap-1.5"><dt className="text-muted-foreground">{metric.label}</dt><dd className="machine-text">{metric.value}</dd></div>)}</dl> : null}
        <div className="flex min-w-0 flex-col gap-4">
          <Section title={t("logs.detail.requestHeaders")} value={headers(requestHeaders)} />
          <Section title={t("logs.detail.requestBody")} value={requestBody} />
          <Section title={t("logs.detail.responseHeaders")} value={headers(responseHeaders)} />
          <Section title={t("logs.detail.responseBody")} value={responseBody} />
        </div>
      </CardContent>
    </Card>
  )
}
