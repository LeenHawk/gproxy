import { Button } from "../../../components/ui";
import type { MemoryUserKeyRow, MemoryUserRow } from "../../../lib/types/admin";

export function UserKeysPane({
  selectedUser,
  keyRows,
  onGenerateKey,
  onRefreshKeys,
  onDeleteKey,
}: {
  selectedUser: MemoryUserRow | null;
  keyRows: MemoryUserKeyRow[];
  onGenerateKey: () => void;
  onRefreshKeys: () => void;
  onDeleteKey: (id: number) => void;
}) {
  return (
    <div className="card-shell">
      <div className="flex flex-wrap items-center justify-between gap-2">
        <div>
          <div className="text-sm font-semibold text-text">Selected User Keys</div>
          <div className="text-xs text-muted">
            {selectedUser ? `${selectedUser.name} (#${selectedUser.id})` : "Select a user"}
          </div>
        </div>
        <div className="flex gap-2">
          <Button disabled={!selectedUser} onClick={onGenerateKey}>
            Generate Key
          </Button>
          <Button variant="neutral" disabled={!selectedUser} onClick={onRefreshKeys}>
            Refresh
          </Button>
        </div>
      </div>
      <div className="mt-4 space-y-2">
        {keyRows.map((row) => (
          <div key={row.id} className="card-shell">
            <div className="flex items-start justify-between gap-2">
              <div>
                <div className="font-semibold">#{row.id}</div>
                <div className="mt-1 font-mono text-xs text-muted">{row.api_key}</div>
                <div className="text-xs text-muted">
                  label={row.label ?? "—"} · enabled={String(row.enabled)}
                </div>
              </div>
              <Button variant="danger" onClick={() => onDeleteKey(row.id)}>
                Delete
              </Button>
            </div>
          </div>
        ))}
        {selectedUser && keyRows.length === 0 ? <p className="text-sm text-muted">No keys</p> : null}
      </div>
    </div>
  );
}
