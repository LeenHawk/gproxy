import { Button, Card } from "../../../components/ui";
import type { CredentialHealthRow } from "../../../lib/types/admin";

export function StatusTab({
  rows,
  onUpdate,
  labels,
}: {
  rows: CredentialHealthRow[];
  onUpdate: (row: CredentialHealthRow, status: "healthy" | "dead") => void;
  labels: {
    none: string;
  };
}) {
  return (
    <Card title="Credential Status">
      <div className="space-y-2">
        {rows.length === 0 ? <p className="text-sm text-muted">{labels.none}</p> : null}
        {rows.map((row) => (
          <div key={`${row.provider}-${row.index}`} className="card-shell">
            <div className="flex items-center justify-between gap-3">
              <div>
                <div className="font-semibold">
                  {row.provider} #{row.index}
                </div>
                <div className="text-sm text-muted">
                  status={row.status} · available={String(row.available)}
                </div>
              </div>
              <div className="flex gap-2">
                <Button variant="neutral" onClick={() => onUpdate(row, "healthy")}>
                  healthy
                </Button>
                <Button variant="danger" onClick={() => onUpdate(row, "dead")}>
                  dead
                </Button>
              </div>
            </div>
          </div>
        ))}
      </div>
    </Card>
  );
}
