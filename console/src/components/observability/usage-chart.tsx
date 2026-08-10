import { useId } from "react";
import { useTranslation } from "react-i18next";
import {
  Area,
  AreaChart,
  CartesianGrid,
  ResponsiveContainer,
  Tooltip,
  XAxis,
  YAxis,
} from "recharts";
import { useMediaQuery } from "@/hooks/use-media-query";
import type { ChartPoint } from "@/lib/rollups";

type Metric = "requests" | "input_tokens" | "output_tokens" | "image_output_tokens" | "cache_write_tokens" | "cache_read_tokens" | "cost";

const METRICS: Metric[] = ["requests", "input_tokens", "output_tokens", "image_output_tokens", "cache_write_tokens", "cache_read_tokens", "cost"];

function fmtDate(unixSecs: number): string {
  return new Date(unixSecs * 1000).toLocaleDateString(undefined, {
    month: "short",
    day: "numeric",
  });
}

function fmtMetricValue(value: number, metric: Metric): string {
  if (metric === "cost") return `$${value.toFixed(4)}`;
  return value.toLocaleString();
}

const REDUCED_MOTION_QUERY = "(prefers-reduced-motion: reduce)";

interface UsageChartProps {
  data: ChartPoint[];
  metric: Metric;
  onMetricChange: (m: Metric) => void;
}

export function UsageChart({ data, metric, onMetricChange }: UsageChartProps) {
  const { t } = useTranslation("observability");
  const reducedMotion = useMediaQuery(REDUCED_MOTION_QUERY);
  const gid = useId();

  if (data.length === 0) {
    return (
      <p className="py-10 text-center text-sm text-muted-foreground">
        {t("chart.noData")}
      </p>
    );
  }

  return (
    <div className="space-y-3">
      {/* Metric selector */}
      <div className="flex flex-wrap gap-2">
        {METRICS.map((m) => (
          <button
            key={m}
            type="button"
            onClick={() => onMetricChange(m)}
            className={
              m === metric
                ? "rounded-md bg-primary px-3 py-1 text-xs font-medium text-primary-foreground"
                : "rounded-md border px-3 py-1 text-xs text-muted-foreground hover:bg-accent hover:text-accent-foreground"
            }
          >
            {t(`chart.metric.${m}`)}
          </button>
        ))}
      </div>

      {/* Chart */}
      <div className="h-64">
        <ResponsiveContainer width="100%" height="100%">
          <AreaChart data={data} margin={{ top: 4, right: 8, bottom: 0, left: 8 }}>
            <defs>
              <linearGradient id={gid} x1="0" y1="0" x2="0" y2="1">
                <stop offset="5%" stopColor="var(--primary)" stopOpacity={0.25} />
                <stop offset="95%" stopColor="var(--primary)" stopOpacity={0} />
              </linearGradient>
            </defs>
            <CartesianGrid strokeDasharray="3 3" stroke="var(--border)" />
            <XAxis
              dataKey="t"
              tickFormatter={fmtDate}
              tick={{ fontSize: 11 }}
              stroke="var(--muted-foreground)"
            />
            <YAxis
              tick={{ fontSize: 11 }}
              stroke="var(--muted-foreground)"
              tickFormatter={(v: number) =>
                metric === "cost" ? `$${v.toFixed(2)}` : v.toLocaleString()
              }
              width={60}
            />
            <Tooltip
              contentStyle={{
                background: "var(--popover)",
                border: "1px solid var(--border)",
                borderRadius: "0.5rem",
                fontSize: 12,
              }}
              labelFormatter={(label) => fmtDate(Number(label))}
              formatter={(value) => [fmtMetricValue(Number(value), metric), t(`chart.metric.${metric}`)]}
            />
            <Area
              type="monotone"
              dataKey={metric}
              stroke="var(--primary)"
              strokeWidth={2}
              fill={`url(#${gid})`}
              isAnimationActive={!reducedMotion}
              dot={false}
            />
          </AreaChart>
        </ResponsiveContainer>
      </div>
    </div>
  );
}

export type { Metric };
