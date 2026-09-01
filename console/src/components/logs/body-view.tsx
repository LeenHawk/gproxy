import type { ReactNode } from "react"
import { useTranslation } from "react-i18next"
import { formattedLogContent } from "@/lib/log-content"

const marker = /\[redacted\]/gi

function highlighted(value: string): Array<ReactNode> {
  return value.split(marker).flatMap((part, index, values) => index + 1 < values.length
    ? [part, <mark key={index} className="rounded bg-state-warning/20 px-1 font-semibold text-state-warning">[redacted]</mark>]
    : [part])
}

export function BodyView({ value }: { value: string | null }) {
  const { t } = useTranslation()
  return (
    <pre className="machine-text min-h-20 max-h-72 overflow-auto whitespace-pre-wrap break-words rounded-lg bg-muted p-3 text-xs leading-relaxed">
      {value == null ? <span className="text-muted-foreground">{t("logs.detail.notCaptured")}</span> : highlighted(formattedLogContent(value))}
    </pre>
  )
}
