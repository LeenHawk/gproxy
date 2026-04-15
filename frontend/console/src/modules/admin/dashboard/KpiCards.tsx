import { MetricCard } from "../../../components/ui";
import type { DashboardKpi } from "../../../lib/types/admin";

function formatInteger(value: number): string {
  return new Intl.NumberFormat().format(value);
}

function formatCost(value: number): string {
  return value.toFixed(4).replace(/\.?0+$/, "");
}

function formatLatency(value?: number | null): string {
  if (value === null || value === undefined || !Number.isFinite(value)) {
    return "—";
  }
  if (value >= 1000) {
    return `${(value / 1000).toFixed(1)}s`;
  }
  return `${Math.round(value)}ms`;
}

function formatSuccessRate(kpi: DashboardKpi): string {
  if (kpi.total_requests <= 0) {
    return "—";
  }
  return `${((kpi.success_count / kpi.total_requests) * 100).toFixed(1)}%`;
}

export function KpiCards({
  kpi,
  loading,
  labels,
}: {
  kpi: DashboardKpi;
  loading: boolean;
  labels: {
    requests: string;
    successRate: string;
    cost: string;
    tokens: string;
    avgLatency: string;
    maxLatency: string;
    loading: string;
  };
}) {
  const totalTokens = kpi.total_input_tokens + kpi.total_output_tokens;
  return (
    <div className="grid gap-3 grid-cols-1 sm:grid-cols-2 xl:grid-cols-6">
      <MetricCard
        label={labels.requests}
        value={loading ? labels.loading : formatInteger(kpi.total_requests)}
      />
      <MetricCard
        label={labels.successRate}
        value={loading ? labels.loading : formatSuccessRate(kpi)}
      />
      <MetricCard label={labels.cost} value={loading ? labels.loading : formatCost(kpi.total_cost)} />
      <MetricCard
        label={labels.tokens}
        value={loading ? labels.loading : formatInteger(totalTokens)}
      />
      <MetricCard
        label={labels.avgLatency}
        value={loading ? labels.loading : formatLatency(kpi.avg_latency_ms)}
      />
      <MetricCard
        label={labels.maxLatency}
        value={loading ? labels.loading : formatLatency(kpi.max_latency_ms)}
      />
    </div>
  );
}
