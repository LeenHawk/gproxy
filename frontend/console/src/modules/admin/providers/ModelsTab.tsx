import { Button, Card, Input, Label, TextArea } from "../../../components/ui";
import type { MemoryModelRow } from "../../../lib/types/admin";

export function ModelsTab({
  rows,
  selectedId,
  form,
  onSelect,
  onCreate,
  onChangeForm,
  onSave,
  onDelete,
  labels,
}: {
  rows: MemoryModelRow[];
  selectedId: number | null;
  form: {
    id: string;
    model_id: string;
    display_name: string;
    enabled: boolean;
    price_each_call: string;
    price_tiers_json: string;
  };
  onSelect: (row: MemoryModelRow) => void;
  onCreate: () => void;
  onChangeForm: (patch: Partial<{
    id: string;
    model_id: string;
    display_name: string;
    enabled: boolean;
    price_each_call: string;
    price_tiers_json: string;
  }>) => void;
  onSave: () => void;
  onDelete: (id: number) => void;
  labels: {
    title: string;
    empty: string;
    create: string;
    save: string;
    delete: string;
    modelId: string;
    displayName: string;
    enabled: string;
    priceEachCall: string;
    priceTiersJson: string;
  };
}) {
  const selected = rows.find((row) => row.id === selectedId) ?? null;

  return (
    <div className="grid gap-4 xl:grid-cols-[360px_minmax(0,1fr)]">
      <Card title={labels.title}>
        <div className="space-y-2">
          {rows.length === 0 ? <p className="text-sm text-muted">{labels.empty}</p> : null}
          {rows.map((row) => (
            <button
              key={row.id}
              type="button"
              className={`nav-item w-full ${row.id === selectedId ? "nav-item-active" : ""}`}
              onClick={() => onSelect(row)}
            >
              <div className="font-semibold">{row.model_id}</div>
              <div className="text-xs text-muted">{row.display_name ?? "—"}</div>
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
        <div className="space-y-4">
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
      </Card>
    </div>
  );
}
