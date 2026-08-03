import { useTranslation } from "react-i18next";
import { cn } from "@/lib/utils";

interface EnabledToggleProps {
  enabled: boolean;
  onToggle: (enabled: boolean) => void;
  disabled?: boolean;
  pending?: boolean;
  className?: string;
}

export function EnabledToggle({
  enabled,
  onToggle,
  disabled = false,
  pending = false,
  className,
}: EnabledToggleProps) {
  const { t } = useTranslation("common");
  const label = t(enabled ? "status.enabled" : "status.disabled");

  return (
    <button
      type="button"
      aria-label={label}
      aria-pressed={enabled}
      aria-busy={pending || undefined}
      title={label}
      disabled={disabled || pending}
      className={cn(
        "inline-flex h-6 shrink-0 items-center rounded-md border px-2 text-xs font-medium transition-colors outline-none",
        "focus-visible:border-ring focus-visible:ring-3 focus-visible:ring-ring/50 disabled:cursor-not-allowed disabled:opacity-50",
        enabled
          ? "border-emerald-500/30 bg-emerald-500/10 text-emerald-700 hover:bg-emerald-500/20 dark:text-emerald-300"
          : "border-border bg-muted/50 text-muted-foreground hover:bg-muted hover:text-foreground",
        className,
      )}
      onClick={(event) => {
        event.stopPropagation();
        onToggle(!enabled);
      }}
      onKeyDown={(event) => event.stopPropagation()}
    >
      {label}
    </button>
  );
}
