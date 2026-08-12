import { Check } from "lucide-react";
import { useTranslation } from "react-i18next";
import type { UpstreamModel } from "@/api/provider-models";
import { Badge } from "@/components/ui/badge";
import { cn } from "@/lib/utils";

interface RowAction {
  existing: boolean;
  metadata: number;
  price: boolean;
  actionable: boolean;
}

export function ModelPullList({
  models,
  selected,
  importPrices,
  pending,
  actionFor,
  onToggle,
}: {
  models: UpstreamModel[];
  selected: Set<string>;
  importPrices: boolean;
  pending: boolean;
  actionFor: (model: UpstreamModel) => RowAction;
  onToggle: (id: string) => void;
}) {
  const { t } = useTranslation("providers");

  if (models.length === 0) {
    return <p className="py-6 text-center text-sm text-muted-foreground">{t("models.pullNoMatch")}</p>;
  }

  return models.map((model) => {
    const action = actionFor(model);
    const checked = selected.has(model.id);
    return (
      <button
        key={model.id}
        type="button"
        disabled={!action.actionable || pending}
        onClick={() => onToggle(model.id)}
        className={cn(
          "flex w-full items-center gap-3 px-3 py-2 text-left text-sm disabled:opacity-60",
          action.actionable && "hover:bg-accent/50",
          checked && "bg-primary/5",
        )}
      >
        <span
          className={cn(
            "grid size-4 shrink-0 place-items-center rounded border",
            checked ? "border-primary bg-primary text-primary-foreground" : "border-input",
          )}
        >
          {checked ? <Check className="size-3" aria-hidden /> : null}
        </span>
        <span className="flex-1 truncate font-mono text-xs">{model.id}</span>
        {model.display_name ? (
          <span className="truncate text-xs text-muted-foreground">{model.display_name}</span>
        ) : null}
        {action.metadata > 0 ? (
          <Badge variant="secondary" className="text-[10px]">
            {t("models.metadataAvailable", { count: action.metadata })}
          </Badge>
        ) : null}
        {importPrices && action.price ? (
          <Badge variant="secondary" className="text-[10px]">
            {t("models.defaultPriceAvailable")}
          </Badge>
        ) : null}
        {action.existing && !action.actionable ? (
          <Badge variant="outline" className="text-[10px]">{t("models.alreadyAdded")}</Badge>
        ) : null}
      </button>
    );
  });
}
