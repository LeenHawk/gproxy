import { Card } from "../../../components/ui";
import type { DashboardTrafficBucket } from "../../../lib/types/admin";
import {
  CartesianGrid,
  Legend,
  Line,
  LineChart,
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

export function TrafficChart({
  rows,
  loading,
  error,
  labels,
}: {
  rows: DashboardTrafficBucket[];
  loading: boolean;
  error: string | null;
  labels: {
    title: string;
    subtitle: string;
    requests: string;
    cost: string;
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
            <LineChart data={data} margin={{ top: 12, right: 12, bottom: 12, left: 0 }}>
              <CartesianGrid strokeDasharray="3 3" stroke="rgba(120, 130, 150, 0.18)" />
              <XAxis dataKey="label" minTickGap={24} tick={{ fontSize: 12 }} />
              <YAxis yAxisId="requests" tick={{ fontSize: 12 }} allowDecimals={false} />
              <YAxis
                yAxisId="cost"
                orientation="right"
                tick={{ fontSize: 12 }}
                tickFormatter={(value) => `${value}`}
              />
              <Tooltip />
              <Legend />
              <Line
                yAxisId="requests"
                type="monotone"
                dataKey="request_count"
                name={labels.requests}
                stroke="#1d4ed8"
                strokeWidth={2.5}
                dot={false}
              />
              <Line
                yAxisId="cost"
                type="monotone"
                dataKey="cost"
                name={labels.cost}
                stroke="#c2410c"
                strokeWidth={2.5}
                dot={false}
              />
            </LineChart>
          </ResponsiveContainer>
        )}
      </div>
    </Card>
  );
}
