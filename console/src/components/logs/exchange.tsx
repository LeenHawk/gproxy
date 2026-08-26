import type { ReactNode } from "react"
import { useTranslation } from "react-i18next"
import { BodyView, HeadersView } from "@/components/logs/body-view"
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card"

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
}

function Section({ title, children }: { title: string; children: ReactNode }) {
  return <section className="flex min-w-0 flex-col gap-2"><h4 className="text-xs font-semibold tracking-wide text-muted-foreground uppercase">{title}</h4>{children}</section>
}

export function Exchange({ title, subtitle, method, target, requestHeaders, requestBody, status, responseHeaders, responseBody }: Props) {
  const { t } = useTranslation()
  return (
    <Card size="sm" className="min-w-0">
      <CardHeader>
        <CardTitle headingLevel={3}>{title}</CardTitle>
        <CardDescription className="machine-text break-all">{subtitle}</CardDescription>
      </CardHeader>
      <CardContent className="flex min-w-0 flex-col gap-5">
        <p className="machine-text break-all text-sm"><span className="font-semibold">{method ?? t("common.none")}</span> {target}</p>
        <div className="grid min-w-0 gap-4 md:grid-cols-2">
          <Section title={t("logs.detail.requestHeaders")}><HeadersView value={requestHeaders} /></Section>
          <Section title={t("logs.detail.requestBody")}><BodyView value={requestBody} /></Section>
          <Section title={t("logs.detail.responseHeaders")}><HeadersView value={responseHeaders} /></Section>
          <Section title={t("logs.detail.responseBody")}><BodyView value={responseBody} /></Section>
        </div>
        <p className="text-sm text-muted-foreground">{t("logs.detail.status")}: <span className="machine-text text-foreground">{status ?? t("logs.pending")}</span></p>
      </CardContent>
    </Card>
  )
}
