import { Card, Table } from "../../../components/ui";
import type { CredentialHealthRow } from "../../../lib/types/admin";

function statusDotClass(status: string): string {
  if (status === "healthy") {
    return "bg-emerald-500";
  }
  if (status === "cooldown") {
    return "bg-amber-500";
  }
  return "bg-rose-500";
}

export function CredentialHealthPanel({
  rows,
  error,
  labels,
}: {
  rows: CredentialHealthRow[];
  error: string | null;
  labels: {
    title: string;
    provider: string;
    index: string;
    status: string;
    available: string;
    yes: string;
    no: string;
  };
}) {
  const columns = [labels.provider, labels.index, labels.status, labels.available];

  return (
    <Card title={labels.title} subtitle={error ?? undefined}>
      <Table
        columns={columns}
        rows={rows.map((row) => ({
          [columns[0]]: row.provider,
          [columns[1]]: row.index,
          [columns[2]]: (
            <span className="inline-flex items-center gap-2">
              <span className={`inline-block h-2.5 w-2.5 rounded-full ${statusDotClass(row.status)}`} />
              <span>{row.status}</span>
            </span>
          ),
          [columns[3]]: row.available ? labels.yes : labels.no,
        }))}
      />
    </Card>
  );
}
