//! The four cell shapes every table here repeats.

import { useTranslation } from "react-i18next"
import { Badge } from "@/components/ui/badge"
import { formatInstant } from "@/lib/format"

export function BoolCell({ value }: { value: boolean }) {
  const { t } = useTranslation()
  return <Badge variant={value ? "success" : "outline"}>{value ? t("values.yes") : t("values.no")}</Badge>
}

export function InstantCell({ value }: { value: number | null }) {
  const { i18n } = useTranslation()
  return <span className="whitespace-nowrap text-xs">{formatInstant(value, i18n.language) ?? "—"}</span>
}

/** A nullable string column: the dash is the answer, not a missing render. */
export function MaybeCell({ value, mono }: { value: string | null; mono?: boolean }) {
  if (value === null || value === "") return <span className="text-muted-foreground">—</span>
  return <span className={mono ? "font-mono text-xs" : undefined}>{value}</span>
}
