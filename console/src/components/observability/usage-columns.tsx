import { useTranslation } from "react-i18next";
import type { Usage } from "@/api/usage";
import type { DataColumn } from "@/components/data-table";
import { Badge } from "@/components/ui/badge";
import { EndpointBadges } from "./endpoint-badges";
import { formatUsageTimestamp } from "./usage-mobile-card";

export function useUsageColumns(
  providerMap: ReadonlyMap<number, string>,
): DataColumn<Usage>[] {
  const { t } = useTranslation("observability");

  return [
    {
      key: "at",
      label: t("usage.columns.at"),
      header: t("usage.columns.at"),
      cell: (usage) => (
        <span className="whitespace-nowrap font-mono text-xs text-muted-foreground">
          {formatUsageTimestamp(usage.at)}
        </span>
      ),
    },
    {
      key: "operation",
      label: t("usage.columns.operation"),
      header: t("usage.columns.operation"),
      cell: (usage) => (
        <EndpointBadges kind={usage.kind} operation={usage.operation} />
      ),
    },
    {
      key: "model",
      label: t("usage.columns.model"),
      header: t("usage.columns.model"),
      cell: (usage) => (
        <span className="font-mono text-xs">{usage.model ?? "—"}</span>
      ),
    },
    {
      key: "provider",
      label: t("usage.columns.provider"),
      header: t("usage.columns.provider"),
      cell: (usage) =>
        usage.provider_id != null
          ? (providerMap.get(usage.provider_id) ?? `#${usage.provider_id}`)
          : "—",
    },
    {
      key: "tokens",
      label: `${t("usage.columns.inputTokens")} / ${t("usage.columns.outputTokens")}`,
      header: `${t("usage.columns.inputTokens")} / ${t("usage.columns.outputTokens")}`,
      cell: (usage) => (
        <span className="tabular-nums text-xs">
          {usage.input_tokens} / {usage.output_tokens}
        </span>
      ),
    },
    {
      key: "imageOutputTokens",
      label: t("usage.columns.imageOutputTokens"),
      header: t("usage.columns.imageOutputTokens"),
      cell: (usage) => (
        <span className="tabular-nums text-xs">{usage.image_output_tokens}</span>
      ),
    },
    {
      key: "cache",
      label: `${t("usage.columns.cacheWrite")} (5m/1h)`,
      header: `${t("usage.columns.cacheWrite")} (5m/1h)`,
      cell: (usage) => (
        <span className="tabular-nums text-xs">
          {usage.cache_creation_5m_tokens} / {usage.cache_creation_1h_tokens}
        </span>
      ),
    },
    {
      key: "cache30m",
      label: `${t("usage.columns.cacheWrite")} (30m)`,
      header: `${t("usage.columns.cacheWrite")} (30m)`,
      cell: (usage) => (
        <span className="tabular-nums text-xs">
          {usage.cache_creation_30m_tokens}
        </span>
      ),
    },
    {
      key: "cacheRead",
      label: t("usage.columns.cacheRead"),
      header: t("usage.columns.cacheRead"),
      cell: (usage) => (
        <span className="tabular-nums text-xs">{usage.cache_read_tokens}</span>
      ),
    },
    {
      key: "cost",
      label: t("usage.columns.cost"),
      header: t("usage.columns.cost"),
      cell: (usage) => (
        <span className="tabular-nums text-xs">
          ${parseFloat(usage.cost || "0").toFixed(5)}
        </span>
      ),
    },
    {
      key: "latency",
      label: t("usage.columns.latency"),
      header: t("usage.columns.latency"),
      cell: (usage) => (
        <span className="tabular-nums text-xs">{usage.latency_ms}ms</span>
      ),
    },
    {
      key: "badges",
      label: `${t("usage.columns.source")} / ${t("usage.columns.ended")}`,
      header: "",
      cell: (usage) => (
        <div className="flex gap-1">
          <Badge variant="outline" className="text-xs">
            {usage.usage_source}
          </Badge>
          <Badge variant="secondary" className="text-xs">
            {usage.ended}
          </Badge>
        </div>
      ),
    },
  ];
}
