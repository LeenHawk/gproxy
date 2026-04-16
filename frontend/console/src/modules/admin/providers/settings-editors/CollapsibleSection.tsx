import { Button } from "../../../../components/ui";

export type TranslateFn = (key: string, params?: Record<string, string | number>) => string;

/// Collapsible section header — matches the routing table pattern.
export function CollapsibleSection({
  title,
  summary,
  expanded,
  onToggle,
  expandLabel,
  collapseLabel,
  actions,
  children,
}: {
  title: string;
  summary: string;
  expanded: boolean;
  onToggle: () => void;
  expandLabel: string;
  collapseLabel: string;
  actions?: React.ReactNode;
  children: React.ReactNode;
}) {
  return (
    <div className="panel-shell space-y-4">
      <div className="flex flex-wrap items-start justify-between gap-3">
        <div>
          <div className="text-sm font-semibold text-text">{title}</div>
          {!expanded ? <p className="mt-1 text-sm text-muted">{summary}</p> : null}
        </div>
        <div className="flex flex-wrap gap-2">
          <Button variant="neutral" onClick={onToggle}>
            {expanded ? collapseLabel : expandLabel}
          </Button>
          {expanded ? actions : null}
        </div>
      </div>
      {expanded ? children : null}
    </div>
  );
}
