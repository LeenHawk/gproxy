import { useTranslation } from "react-i18next";
import type { UsageSummary } from "@/api/usage";
import { Skeleton } from "@/components/ui/skeleton";

function count(value: number): string {
  return value.toLocaleString();
}

function cost(value: string): string {
  return `$${(parseFloat(value) || 0).toFixed(5)}`;
}

interface UsageSummaryBarProps {
  summary?: UsageSummary;
  pending?: boolean;
}

export function UsageSummaryBar({ summary, pending = false }: UsageSummaryBarProps) {
  const { t } = useTranslation("observability");
  const metrics = [
    { label: t("chart.metric.requests"), value: summary && count(summary.requests) },
    {
      label: t("usage.columns.inputTokens"),
      value: summary && count(summary.input_tokens),
    },
    {
      label: t("usage.columns.outputTokens"),
      value: summary && count(summary.output_tokens),
    },
    {
      label: `${t("usage.columns.cacheWrite")} · 5m`,
      value: summary && count(summary.cache_creation_5m_tokens),
    },
    {
      label: `${t("usage.columns.cacheWrite")} · 30m`,
      value: summary && count(summary.cache_creation_30m_tokens),
    },
    {
      label: `${t("usage.columns.cacheWrite")} · 1h`,
      value: summary && count(summary.cache_creation_1h_tokens),
    },
    {
      label: t("usage.columns.cacheRead"),
      value: summary && count(summary.cache_read_tokens),
    },
    { label: t("usage.columns.cost"), value: summary && cost(summary.cost) },
  ];

  return (
    <div
      className="overflow-hidden rounded-lg border bg-card"
      aria-label={t("usage.summary")}
      aria-busy={pending}
    >
      <dl className="-mb-px -mr-px grid grid-cols-2 sm:grid-cols-4 lg:grid-cols-8">
        {metrics.map((metric) => (
          <div key={metric.label} className="min-w-0 border-r border-b px-3 py-2.5">
            <dt className="min-h-8 text-xs leading-4 text-muted-foreground lg:min-h-12 xl:min-h-8">
              {metric.label}
            </dt>
            <dd className="mt-1 min-w-0 font-mono text-sm font-semibold tabular-nums [overflow-wrap:anywhere]">
              {pending ? <Skeleton className="h-5 w-16 max-w-full" /> : (metric.value ?? "—")}
            </dd>
          </div>
        ))}
      </dl>
    </div>
  );
}
