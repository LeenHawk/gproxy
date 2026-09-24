import type { ReactNode } from "react"
import { useTranslation } from "react-i18next"
import { Badge } from "@/components/ui/badge"
import { Card, CardContent, CardFooter, CardHeader, CardTitle } from "@/components/ui/card"

export function ModelIdentity({ name }: { name: string }) {
  return <div className="truncate py-1 font-mono text-sm font-medium" title={name}>{name}</div>
}

export function ModelLimits({ metadata }: { metadata: Record<string, unknown> }) {
  const { t, i18n } = useTranslation()
  const number = (value: unknown) => typeof value === "number" ? value.toLocaleString(i18n.language) : "—"
  return <dl className="grid grid-cols-[auto_auto] items-baseline gap-x-3 gap-y-1 text-sm"><dt className="text-muted-foreground">{t("catalog.context_window")}</dt><dd className="text-right font-mono tabular-nums">{number(metadata.context_window)}</dd><dt className="text-muted-foreground">{t("providers.models.maxOutput")}</dt><dd className="text-right font-mono tabular-nums">{number(metadata.max_output_tokens)}</dd></dl>
}

export function ModelCapabilities({ metadata }: { metadata: Record<string, unknown> }) {
  const { t } = useTranslation()
  const input = Array.isArray(metadata.input_modalities) ? metadata.input_modalities : []
  const output = Array.isArray(metadata.output_modalities) ? metadata.output_modalities : []
  const parameters = Array.isArray(metadata.supported_parameters) ? metadata.supported_parameters : []
  const thinking = metadata.thinking_supported === true || parameters.includes("reasoning")
  return <div className="flex max-w-56 flex-col gap-1.5">
    {input.length || output.length ? <span className="truncate text-sm text-muted-foreground" title={`${input.join(", ")} → ${output.join(", ")}`}>{input.join(", ") || "—"} → {output.join(", ") || "—"}</span> : null}
    <div className="flex flex-wrap gap-1">{thinking ? <Badge variant="outline">{t("providers.models.thinking")}</Badge> : null}{parameters.includes("tools") ? <Badge variant="outline">{t("modelUI.profiles.tools")}</Badge> : null}{!thinking && !parameters.includes("tools") && !input.length && !output.length ? <span className="text-sm text-muted-foreground">—</span> : null}</div>
  </div>
}

export function ModelSummaryCard({ name, metadata, control, children, actions }: { name: string; metadata: Record<string, unknown>; control?: ReactNode; children?: ReactNode; actions: ReactNode }) {
  return <Card className="gap-4 py-4 shadow-none"><CardHeader className="flex flex-row items-start justify-between gap-3 px-4"><div className="min-w-0"><CardTitle className="break-all font-mono text-sm leading-5">{name}</CardTitle></div>{control}</CardHeader><CardContent className="flex flex-col gap-3 px-4"><ModelLimits metadata={metadata} /><ModelCapabilities metadata={metadata} />{children}</CardContent><CardFooter className="justify-end gap-1 border-t px-4 pt-3">{actions}</CardFooter></Card>
}
