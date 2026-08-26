import type { ReactNode } from "react"
import { useTranslation } from "react-i18next"

const marker = /\[redacted\]/gi

function formatted(value: string) {
  try {
    return JSON.stringify(JSON.parse(value), null, 2)
  } catch {
    return value
  }
}

function highlighted(value: string): Array<ReactNode> {
  return value.split(marker).flatMap((part, index, values) => index + 1 < values.length
    ? [part, <mark key={index} className="rounded bg-state-warning/20 px-1 font-semibold text-state-warning">[redacted]</mark>]
    : [part])
}

export function BodyView({ value }: { value: string | null }) {
  const { t } = useTranslation()
  if (value == null) return <p className="text-sm text-muted-foreground">{t("logs.detail.notCaptured")}</p>
  return <pre className="machine-text max-h-80 overflow-auto whitespace-pre-wrap break-words rounded-lg bg-muted p-3 text-xs leading-relaxed">{highlighted(formatted(value))}</pre>
}

export function HeadersView({ value }: { value: Record<string, string> | null }) {
  return <BodyView value={value == null ? null : JSON.stringify(value)} />
}
