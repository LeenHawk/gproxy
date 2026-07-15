import { useTranslation } from "react-i18next";
import type { Usage } from "@/api/usage";
import { Badge } from "@/components/ui/badge";

export function formatUsageTimestamp(unixSecs: number): string {
  return new Date(unixSecs * 1000).toLocaleString(undefined, {
    month: "short",
    day: "numeric",
    hour: "2-digit",
    minute: "2-digit",
    second: "2-digit",
  });
}

function count(value: number): string {
  return value.toLocaleString();
}

interface UsageMobileCardProps {
  usage: Usage;
  providerLabel?: string;
}

export function UsageMobileCard({ usage, providerLabel }: UsageMobileCardProps) {
  const { t } = useTranslation("observability");

  return (
    <div className="grid gap-3">
      <div className="grid gap-1">
        <div className="flex items-center justify-between gap-2">
          <span className="font-mono text-xs">
            {usage.operation}
            {usage.kind && usage.kind !== usage.operation && (
              <span className="text-muted-foreground"> / {usage.kind}</span>
            )}
          </span>
          <span className="tabular-nums text-xs">
            ${parseFloat(usage.cost || "0").toFixed(5)}
          </span>
        </div>
        <div className="flex flex-wrap gap-1 text-xs text-muted-foreground">
          <span>{formatUsageTimestamp(usage.at)}</span>
          {usage.model && <span>· {usage.model}</span>}
          {providerLabel && <span>· {providerLabel}</span>}
          <span>· {usage.latency_ms}ms</span>
        </div>
      </div>

      <dl className="grid grid-cols-2 gap-x-4 gap-y-2 rounded-md bg-muted/40 px-3 py-2 text-xs">
        <div>
          <dt className="text-muted-foreground">{t("usage.columns.inputTokens")}</dt>
          <dd className="font-medium tabular-nums">{count(usage.input_tokens)}</dd>
        </div>
        <div>
          <dt className="text-muted-foreground">{t("usage.columns.outputTokens")}</dt>
          <dd className="font-medium tabular-nums">{count(usage.output_tokens)}</dd>
        </div>
        <div className="col-span-2">
          <dt className="text-muted-foreground">{t("usage.columns.cacheWrite")}</dt>
          <dd className="flex flex-wrap gap-x-3 font-medium tabular-nums">
            <span>5m {count(usage.cache_creation_5m_tokens)}</span>
            <span>30m {count(usage.cache_creation_30m_tokens)}</span>
            <span>1h {count(usage.cache_creation_1h_tokens)}</span>
          </dd>
        </div>
        <div className="col-span-2">
          <dt className="text-muted-foreground">{t("usage.columns.cacheRead")}</dt>
          <dd className="font-medium tabular-nums">{count(usage.cache_read_tokens)}</dd>
        </div>
      </dl>

      <div className="flex gap-1">
        <Badge variant="outline" className="text-xs">{usage.usage_source}</Badge>
        <Badge variant="secondary" className="text-xs">{usage.ended}</Badge>
      </div>
    </div>
  );
}
