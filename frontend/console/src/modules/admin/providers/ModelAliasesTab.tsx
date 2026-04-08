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
  labels: {
    title: string;
    empty: string;
    create: string;
    save: string;
    delete: string;
    alias: string;
    modelId: string;
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
      </Card>
    </div>
  );
}
