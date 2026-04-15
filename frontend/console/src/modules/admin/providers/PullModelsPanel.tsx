import { useState, type ReactNode } from "react";

import { Button } from "../../../components/ui";
import type { MemoryModelRow } from "../../../lib/types/admin";

export type PullModelsPanelLabels = {
  pull: string;
  pullLoading: string;
  pullEmpty: string;
  pullFound: string;
  pullImport: string;
  pullSelectAll: string;
  pullDeselectAll: string;
  cancel: string;
};

/// Hook that owns the "pull models" state (loading + the list of candidate
/// model ids + which ones the user has selected for import) and returns
/// both the trigger button JSX and the panel JSX as ReactNode so the caller
/// can mount them in two different spots (the list card header and the
/// right-hand card body) without the state leaking out.
export function usePullModelsPanel({
  rows,
  onPull,
  onImport,
  labels,
}: {
  rows: MemoryModelRow[];
  onPull?: () => Promise<string[]>;
  onImport?: (models: string[]) => void;
  labels: PullModelsPanelLabels;
}): {
  isActive: boolean;
  trigger: ReactNode;
  panel: ReactNode;
} {
  const [pullLoading, setPullLoading] = useState(false);
  const [pulledModels, setPulledModels] = useState<string[] | null>(null);
  const [pullSelected, setPullSelected] = useState<Set<string>>(new Set());

  const doPull = async () => {
    if (!onPull) return;
    setPullLoading(true);
    try {
      const models = await onPull();
      const existing = new Set(rows.map((row) => row.model_id));
      const newModels = models.filter((m) => !existing.has(m));
      setPulledModels(newModels);
      setPullSelected(new Set(newModels));
    } finally {
      setPullLoading(false);
    }
  };

  const closePull = () => {
    setPulledModels(null);
    setPullSelected(new Set());
  };

  const trigger: ReactNode = onPull ? (
    <Button variant="neutral" onClick={() => void doPull()} disabled={pullLoading}>
      {pullLoading ? labels.pullLoading : labels.pull}
    </Button>
  ) : null;

  const panel: ReactNode =
    pulledModels !== null ? (
      <div className="space-y-3">
        {pulledModels.length === 0 ? (
          <p className="text-sm text-muted">{labels.pullEmpty}</p>
        ) : (
          <>
            <p className="text-sm">
              {labels.pullFound.replace("{count}", String(pulledModels.length))}
            </p>
            <div className="flex gap-2">
              <Button
                variant="neutral"
                onClick={() =>
                  setPullSelected((prev) =>
                    prev.size === pulledModels.length ? new Set() : new Set(pulledModels),
                  )
                }
              >
                {pullSelected.size === pulledModels.length
                  ? labels.pullDeselectAll
                  : labels.pullSelectAll}
              </Button>
            </div>
            <div className="max-h-60 overflow-y-auto space-y-1 border border-border rounded p-2">
              {pulledModels.map((model) => (
                <label
                  key={model}
                  className="flex items-center gap-2 cursor-pointer text-sm py-0.5"
                >
                  <input
                    type="checkbox"
                    checked={pullSelected.has(model)}
                    onChange={() =>
                      setPullSelected((prev) => {
                        const next = new Set(prev);
                        if (next.has(model)) next.delete(model);
                        else next.add(model);
                        return next;
                      })
                    }
                  />
                  {model}
                </label>
              ))}
            </div>
          </>
        )}
        <div className="flex gap-2 justify-end">
          <Button variant="neutral" onClick={closePull}>
            {labels.cancel}
          </Button>
          {pulledModels.length > 0 ? (
            <Button
              onClick={() => {
                if (onImport) onImport([...pullSelected]);
                closePull();
              }}
              disabled={pullSelected.size === 0}
            >
              {labels.pullImport.replace("{count}", String(pullSelected.size))}
            </Button>
          ) : null}
        </div>
      </div>
    ) : null;

  return { isActive: pulledModels !== null, trigger, panel };
}
