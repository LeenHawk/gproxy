import { Button, Card } from "../../../components/ui";
import type { ProviderRow } from "../../../lib/types/admin";

export function ProviderList({
  rows,
  selectedProviderId,
  onSelect,
  onCreate,
  onRefresh,
  title,
  emptyLabel,
  newLabel,
  refreshLabel,
}: {
  rows: ProviderRow[];
  selectedProviderId: number | null;
  onSelect: (row: ProviderRow) => void;
  onCreate: () => void;
  onRefresh: () => void;
  title: string;
  emptyLabel: string;
  newLabel: string;
  refreshLabel: string;
}) {
  return (
    <Card
      title={title}
      action={
        <div className="flex gap-2">
          <Button variant="neutral" onClick={onRefresh}>
            {refreshLabel}
          </Button>
          <Button onClick={onCreate}>{newLabel}</Button>
        </div>
      }
    >
      <div className="space-y-2">
        {rows.length === 0 ? <p className="text-sm text-muted">{emptyLabel}</p> : null}
        {rows.map((row) => {
          const displayName = row.label?.trim() || row.name;
          const showTechnicalName = displayName !== row.name;
          return (
            <button
              key={row.id}
              type="button"
              className={`nav-item w-full ${selectedProviderId === row.id ? "nav-item-active" : ""}`}
              onClick={() => onSelect(row)}
            >
              <div className="font-semibold">{displayName}</div>
              <div className="text-xs text-muted">
                #{row.id} · {showTechnicalName ? `${row.name} · ` : ""}
                {row.channel} · {row.credential_count} creds
              </div>
            </button>
          );
        })}
      </div>
    </Card>
  );
}
