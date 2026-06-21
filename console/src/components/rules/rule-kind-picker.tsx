import { useTranslation } from "react-i18next";
import { RULE_KINDS } from "@/api/rules";
import { RULE_KIND_META } from "./rule-kind-meta";
import { cn } from "@/lib/utils";

interface Props { value: string; onChange: (k: string) => void }

export function RuleKindPicker({ value, onChange }: Props) {
  const { t } = useTranslation("rules");
  return (
    <div className="grid grid-cols-1 gap-2 sm:grid-cols-2">
      {RULE_KINDS.map((k) => {
        const meta = RULE_KIND_META[k];
        const Icon = meta.icon;
        const selected = value === k;
        return (
          <button
            key={k}
            type="button"
            onClick={() => onChange(k)}
            aria-pressed={selected}
            className={cn(
              "flex items-start gap-3 rounded-md border p-3 text-left transition-colors",
              selected ? "border-primary bg-primary/5" : "hover:bg-muted/50",
            )}
          >
            <Icon className="mt-0.5 size-4 shrink-0" aria-hidden />
            <span className="grid gap-0.5">
              <span className="text-sm font-medium">{t(`kind.${k}`)}</span>
              <span className="text-xs text-muted-foreground">{t(meta.descKey)}</span>
            </span>
          </button>
        );
      })}
    </div>
  );
}
