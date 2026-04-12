import { useState } from "react";

import { Button, Card, Input, Label } from "../../../components/ui";
import type { MemoryModelAliasRow } from "../../../lib/types/admin";

export function ModelAliasesTab({
  rows,
  selectedId,
  form,
  onSelect,
  onCreate,
  onChangeForm,
  onSave,
  onDelete,
  onPull,
  onImport,
  labels,
}: {
  rows: MemoryModelAliasRow[];
  selectedId: number | null;
  form: {
    id: string;
    alias: string;
    model_id: string;
  };
  onSelect: (row: MemoryModelAliasRow) => void;
  onCreate: () => void;
  onChangeForm: (patch: Partial<{ id: string; alias: string; model_id: string }>) => void;
  onSave: () => void;
  onDelete: (alias: string) => void;
  onPull?: () => Promise<string[]>;
  onImport?: (models: string[]) => void;
  labels: {
    title: string;
    empty: string;
    create: string;
    save: string;
    delete: string;
    alias: string;
    modelId: string;
    cancel?: string;
    pull?: string;
    pullLoading?: string;
    pullEmpty?: string;
    pullFound?: string;
    pullImport?: string;
    pullSelectAll?: string;
    pullDeselectAll?: string;
  };
}) {
  const selected = rows.find((row) => row.id === selectedId) ?? null;

  // Pull state
  const [pullLoading, setPullLoading] = useState(false);
  const [pulledModels, setPulledModels] = useState<string[] | null>(null);
  const [pullSelected, setPullSelected] = useState<Set<string>>(new Set());

  const doPull = async () => {
    if (!onPull) return;
    setPullLoading(true);
    try {
      const models = await onPull();
      const existingAliases = new Set(rows.map((r) => r.alias));
      const newModels = models.filter((m) => !existingAliases.has(m));
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

  return (
    <div className="grid gap-4 xl:grid-cols-[360px_minmax(0,1fr)]">
      <Card
        title={labels.title}
        action={
          onPull ? (
            <Button variant="neutral" onClick={() => void doPull()} disabled={pullLoading}>
              {pullLoading ? (labels.pullLoading ?? "...") : (labels.pull ?? "Pull")}
            </Button>
          ) : undefined
        }
      >
        <div className="space-y-2">
          {rows.length === 0 ? <p className="text-sm text-muted">{labels.empty}</p> : null}
          {rows.map((row) => (
            <button
              key={row.id}
              type="button"
              className={`nav-item w-full ${row.id === selectedId ? "nav-item-active" : ""}`}
              onClick={() => onSelect(row)}
            >
              <div className="font-semibold">{row.alias}</div>
              <div className="text-xs text-muted">{row.model_id}</div>
            </button>
          ))}
        </div>
      </Card>
      <Card
        title={selected ? labels.title : labels.create}
        action={
          <Button variant="neutral" onClick={onCreate}>
            {labels.create}
          </Button>
        }
      >
        {pulledModels !== null ? (
          <div className="space-y-3">
            {pulledModels.length === 0 ? (
              <p className="text-sm text-muted">{labels.pullEmpty ?? "No new models found."}</p>
            ) : (
              <>
                <p className="text-sm">{(labels.pullFound ?? "Found {count} new models").replace("{count}", String(pulledModels.length))}</p>
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
                      ? (labels.pullDeselectAll ?? "Deselect All")
                      : (labels.pullSelectAll ?? "Select All")}
                  </Button>
                </div>
                <div className="max-h-60 overflow-y-auto space-y-1 border border-border rounded p-2">
                  {pulledModels.map((model) => (
                    <label key={model} className="flex items-center gap-2 cursor-pointer text-sm py-0.5">
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
              <Button variant="neutral" onClick={closePull}>{labels.cancel ?? "Cancel"}</Button>
              {pulledModels.length > 0 ? (
                <Button onClick={() => {
                  if (onImport) onImport([...pullSelected]);
                  closePull();
                }} disabled={pullSelected.size === 0}>
                  {(labels.pullImport ?? "Import ({count})").replace("{count}", String(pullSelected.size))}
                </Button>
              ) : null}
            </div>
          </div>
        ) : (
          <div className="space-y-4">
            <div>
              <Label>{labels.alias}</Label>
              <Input value={form.alias} onChange={(value) => onChangeForm({ alias: value })} />
            </div>
            <div>
              <Label>{labels.modelId}</Label>
              <Input value={form.model_id} onChange={(value) => onChangeForm({ model_id: value })} />
            </div>
            <div className="flex gap-2">
              <Button onClick={onSave}>{labels.save}</Button>
              {selected ? (
                <Button variant="danger" onClick={() => onDelete(selected.alias)}>
                  {labels.delete}
                </Button>
              ) : null}
            </div>
          </div>
        )}
      </Card>
    </div>
  );
}
