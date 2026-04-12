import { useMemo, useState } from "react";

import { Button, Card, Input, Label, Select, TextArea } from "../../../components/ui";
import type { MemoryModelRow } from "../../../lib/types/admin";

export type ModelFormState = {
  id: string;
  model_id: string;
  display_name: string;
  enabled: boolean;
  price_each_call: string;
  price_tiers_json: string;
  alias_of: string;
};

export type ModelsTabFilter = "all" | "real" | "aliases";

type ModelsTabLabels = {
  title: string;
  empty: string;
  create: string;
  save: string;
  delete: string;
  cancel: string;
  modelId: string;
  displayName: string;
  enabled: string;
  priceEachCall: string;
  priceTiersJson: string;
  aliasOf: string;
  aliasOfNone: string;
  aliasBadge: string;
  filterAll: string;
  filterReal: string;
  filterAliases: string;
  priceOverrideHint: string;
  pull: string;
  pullLoading: string;
  pullEmpty: string;
  pullFound: string;
  pullImport: string;
  pullSelectAll: string;
  pullDeselectAll: string;
};

export function ModelsTab({
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
  rows: MemoryModelRow[];
  selectedId: number | null;
  form: ModelFormState;
  onSelect: (row: MemoryModelRow) => void;
  onCreate: () => void;
  onChangeForm: (patch: Partial<ModelFormState>) => void;
  onSave: () => void;
  onDelete: (id: number) => void;
  onPull?: () => Promise<string[]>;
  onImport?: (models: string[]) => void;
  labels: ModelsTabLabels;
}) {
  const selected = rows.find((row) => row.id === selectedId) ?? null;
  const [filter, setFilter] = useState<ModelsTabFilter>("all");

  const realModels = useMemo(
    () => rows.filter((row) => row.alias_of == null),
    [rows],
  );

  const filteredRows = useMemo(() => {
    if (filter === "real") {
      return rows.filter((row) => row.alias_of == null);
    }
    if (filter === "aliases") {
      return rows.filter((row) => row.alias_of != null);
    }
    return rows;
  }, [rows, filter]);

  const targetNameById = useMemo(() => {
    const map = new Map<number, string>();
    for (const row of rows) {
      map.set(row.id, row.model_id);
    }
    return map;
  }, [rows]);

  const aliasOfOptions = useMemo(
    () => [
      { value: "", label: labels.aliasOfNone },
      ...realModels.map((row) => ({
        value: String(row.id),
        label: row.model_id,
      })),
    ],
    [realModels, labels.aliasOfNone],
  );

  const isAliasForm = form.alias_of !== "";

  // Pull state
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

  const filterButtons: Array<{ value: ModelsTabFilter; label: string }> = [
    { value: "all", label: labels.filterAll },
    { value: "real", label: labels.filterReal },
    { value: "aliases", label: labels.filterAliases },
  ];

  return (
    <div className="grid gap-4 xl:grid-cols-[360px_minmax(0,1fr)]">
      <Card
        title={labels.title}
        action={
          onPull ? (
            <Button variant="neutral" onClick={() => void doPull()} disabled={pullLoading}>
              {pullLoading ? labels.pullLoading : labels.pull}
            </Button>
          ) : undefined
        }
      >
        <div className="space-y-3">
          <div className="flex flex-wrap gap-1">
            {filterButtons.map((btn) => (
              <Button
                key={btn.value}
                variant={filter === btn.value ? "primary" : "neutral"}
                onClick={() => setFilter(btn.value)}
              >
                {btn.label}
              </Button>
            ))}
          </div>
          <div className="space-y-2">
            {filteredRows.length === 0 ? (
              <p className="text-sm text-muted">{labels.empty}</p>
            ) : null}
            {filteredRows.map((row) => {
              const isAlias = row.alias_of != null;
              const targetName = isAlias
                ? targetNameById.get(row.alias_of as number) ?? String(row.alias_of)
                : null;
              return (
                <button
                  key={row.id}
                  type="button"
                  className={`nav-item w-full ${row.id === selectedId ? "nav-item-active" : ""}`}
                  onClick={() => onSelect(row)}
                >
                  <div className="flex items-center gap-2">
                    <div className="font-semibold">{row.model_id}</div>
                    {isAlias ? (
                      <span className="rounded border border-border px-1.5 py-0.5 text-[10px] uppercase tracking-wide text-muted">
                        {labels.aliasBadge}
                      </span>
                    ) : null}
                  </div>
                  <div className="text-xs text-muted">
                    {isAlias ? `→ ${targetName}` : row.display_name ?? "—"}
                  </div>
                </button>
              );
            })}
          </div>
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
        ) : (
          <div className="space-y-4">
            <div>
              <Label>{labels.aliasOf}</Label>
              <Select
                value={form.alias_of}
                onChange={(value) => onChangeForm({ alias_of: value })}
                options={aliasOfOptions}
              />
            </div>
            <div>
              <Label>{labels.modelId}</Label>
              <Input value={form.model_id} onChange={(value) => onChangeForm({ model_id: value })} />
            </div>
            <div>
              <Label>{labels.displayName}</Label>
              <Input
                value={form.display_name}
                onChange={(value) => onChangeForm({ display_name: value })}
              />
            </div>
            <label className="flex items-center gap-2 text-sm text-muted">
              <input
                type="checkbox"
                checked={form.enabled}
                onChange={(event) => onChangeForm({ enabled: event.target.checked })}
              />
              {labels.enabled}
            </label>
            {isAliasForm ? (
              <p className="text-xs text-muted">{labels.priceOverrideHint}</p>
            ) : null}
            <div>
              <Label>{labels.priceEachCall}</Label>
              <Input
                value={form.price_each_call}
                onChange={(value) => onChangeForm({ price_each_call: value })}
              />
            </div>
            <div>
              <Label>{labels.priceTiersJson}</Label>
              <TextArea
                value={form.price_tiers_json}
                onChange={(value) => onChangeForm({ price_tiers_json: value })}
                rows={8}
              />
            </div>
            <div className="flex gap-2">
              <Button onClick={onSave}>{labels.save}</Button>
              {selected ? (
                <Button variant="danger" onClick={() => onDelete(selected.id)}>
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
