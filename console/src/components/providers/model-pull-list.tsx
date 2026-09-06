import { useTranslation } from "react-i18next"
import type { DiscoveredModelDto } from "@/generated/DiscoveredModelDto"
import { Badge } from "@/components/ui/badge"
import { Checkbox } from "@/components/ui/checkbox"

export type ModelPullAction = {
  actionable: boolean
  gaps: number
  priceAvailable: boolean
  priced: boolean
}

export function ModelPullList({ models, selected, pending, actionFor, onToggle }: {
  models: Array<DiscoveredModelDto>
  selected: Set<string>
  pending: boolean
  actionFor: (model: DiscoveredModelDto) => ModelPullAction
  onToggle: (id: string) => void
}) {
  const { t, i18n } = useTranslation()
  const number = (value: number | null) => value == null ? "—" : value.toLocaleString(i18n.language)

  return <div className="divide-y overflow-y-auto rounded-lg border">
    {models.map((model) => {
      const action = actionFor(model)
      return <label key={model.model_id} className="flex min-h-14 items-center gap-3 px-3 py-2.5 [content-visibility:auto] [contain-intrinsic-size:auto_56px] has-disabled:cursor-not-allowed has-disabled:opacity-60 hover:bg-muted/50">
        <Checkbox checked={selected.has(model.model_id)} disabled={pending || !action.actionable} onCheckedChange={() => onToggle(model.model_id)} aria-label={model.model_id} />
        <span className="min-w-0 flex-1">
          <span className="machine-text block truncate text-xs font-medium">{model.model_id}</span>
          <span className="block truncate text-xs text-muted-foreground">{model.display_name ?? t("providers.models.pullNoDisplayName")}</span>
          {(model.metadata.input_modalities?.length ?? 0) > 0 || (model.metadata.output_modalities?.length ?? 0) > 0
            ? <span className="machine-text block truncate text-[11px] text-muted-foreground">{model.metadata.input_modalities?.join(", ") || "—"} → {model.metadata.output_modalities?.join(", ") || "—"}</span>
            : null}
        </span>
        <span className="hidden shrink-0 text-right text-xs text-muted-foreground md:block">
          <span className="machine-text block">{number(model.context_window)}</span>
          <span className="machine-text block">{number(model.max_output_tokens)}</span>
        </span>
        <span className="flex shrink-0 flex-col items-end gap-1 sm:flex-row sm:items-center">
          {action.priceAvailable ? <Badge variant="outline">{t("providers.models.defaultPriceAvailable")}</Badge> : action.priced ? <Badge variant="secondary">{t("providers.models.priced")}</Badge> : null}
          {model.known ? <Badge variant={action.gaps > 0 ? "outline" : "secondary"}>{t(action.gaps > 0 ? "providers.models.pullGaps" : "providers.models.pullKnown", { count: action.gaps })}</Badge> : null}
        </span>
      </label>
    })}
  </div>
}
