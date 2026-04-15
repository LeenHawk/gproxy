import { Card } from "../../../components/ui";
import type { DashboardStatusBucket } from "../../../lib/types/admin";
import {
  Bar,
  BarChart,
  CartesianGrid,
  Legend,
  ResponsiveContainer,
  Tooltip,
  XAxis,
  YAxis,
} from "recharts";

function formatBucketLabel(bucket: number): string {
  return new Date(bucket * 1000).toLocaleString([], {
    month: "2-digit",
    day: "2-digit",
    hour: "2-digit",
    minute: "2-digit",
  });
}

export function StatusCodesChart({
  rows,
  loading,
  error,
  labels,
}: {
  rows: DashboardStatusBucket[];
  loading: boolean;
  error: string | null;
  labels: {
    title: string;
    subtitle: string;
    ok: string;
    err4xx: string;
    err5xx: string;
    noData: string;
  };
}) {
  const data = rows.map((row) => ({
    ...row,
    label: formatBucketLabel(row.bucket),
  }));

  return (
    <Card title={labels.title} subtitle={error ?? labels.subtitle}>
      <div className="h-72">
        {loading ? (
          <div className="flex h-full items-center justify-center text-sm text-muted">
            {labels.noData}
          </div>
        ) : data.length === 0 ? (
          <div className="flex h-full items-center justify-center text-sm text-muted">
            {labels.noData}
          </div>
        ) : (
          <ResponsiveContainer width="100%" height="100%">
            <BarChart data={data} margin={{ top: 12, right: 12, bottom: 12, left: 0 }}>
              <CartesianGrid strokeDasharray="3 3" stroke="rgba(120, 130, 150, 0.18)" />
              <XAxis dataKey="label" minTickGap={24} tick={{ fontSize: 12 }} />
              <YAxis allowDecimals={false} tick={{ fontSize: 12 }} />
              <Tooltip />
              <Legend />
              <Bar dataKey="ok" stackId="status" name={labels.ok} fill="#15803d" />
              <Bar dataKey="err_4xx" stackId="status" name={labels.err4xx} fill="#d97706" />
              <Bar dataKey="err_5xx" stackId="status" name={labels.err5xx} fill="#b91c1c" />
            </BarChart>
          </ResponsiveContainer>
        )}
      </div>
    </Card>
  );
}
