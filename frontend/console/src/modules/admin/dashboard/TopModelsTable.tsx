import { Card, Table } from "../../../components/ui";
import type { DashboardTopModelRow } from "../../../lib/types/admin";

function formatCost(value: number): string {
  return value.toFixed(4).replace(/\.?0+$/, "");
}

function formatInteger(value: number): string {
  return new Intl.NumberFormat().format(value);
}

export function TopModelsTable({
  rows,
  error,
  labels,
}: {
  rows: DashboardTopModelRow[];
  error: string | null;
  labels: {
    title: string;
    model: string;
    requests: string;
    cost: string;
    inputTokens: string;
    outputTokens: string;
  };
}) {
  const columns = [
    labels.model,
    labels.requests,
    labels.cost,
    labels.inputTokens,
    labels.outputTokens,
  ];

  return (
    <Card title={labels.title} subtitle={error ?? undefined}>
      <Table
        columns={columns}
        rows={rows.map((row) => ({
          [columns[0]]: row.model ?? "—",
          [columns[1]]: formatInteger(row.request_count),
          [columns[2]]: formatCost(row.total_cost),
          [columns[3]]: formatInteger(row.total_input_tokens),
          [columns[4]]: formatInteger(row.total_output_tokens),
        }))}
      />
    </Card>
  );
}
