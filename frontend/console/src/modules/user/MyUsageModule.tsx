import { useEffect, useMemo, useState } from "react";

import { useI18n } from "../../app/i18n";
import { Badge, Button, Card } from "../../components/ui";
import { apiJson } from "../../lib/api";
import { authHeaders } from "../../lib/auth";
import { formatTimestamp } from "../../lib/datetime";
import type { CountResponse } from "../../lib/types/shared";
import type { UsageQueryRow } from "../../lib/types/shared";
import { buildMyUsageQuery, summarizeUsageRows } from "./usage";

export function MyUsageModule({
  sessionToken,
  notify,
}: {
  sessionToken: string;
  notify: (kind: "success" | "error" | "info", message: string) => void;
}) {
  const { t } = useI18n();
  const [rows, setRows] = useState<UsageQueryRow[]>([]);
  const [totalCount, setTotalCount] = useState(0);
  const [loading, setLoading] = useState(false);
  const headers = useMemo(() => authHeaders(sessionToken), [sessionToken]);
  const summary = useMemo(() => summarizeUsageRows(rows), [rows]);

  const load = async () => {
    try {
      setLoading(true);
      const [usageRows, count] = await Promise.all([
        apiJson<UsageQueryRow[]>("/user/usages/query", {
          method: "POST",
          headers,
          body: JSON.stringify(buildMyUsageQuery("50")),
        }),
        apiJson<CountResponse>("/user/usages/count", {
          method: "POST",
          headers,
          body: JSON.stringify(buildMyUsageQuery("")),
        }),
      ]);
      setRows(usageRows);
      setTotalCount(count.count);
    } catch (error) {
      notify("error", error instanceof Error ? error.message : String(error));
    } finally {
      setLoading(false);
    }
  };

  useEffect(() => {
    void load();
  }, [headers]);

  return (
    <Card title={t("myUsage.title")} subtitle={t("myUsage.subtitle")}>
      <div className="toolbar-shell">
        <div className="toolbar-actions">
          <Button variant="neutral" onClick={() => void load()}>
            {loading ? t("common.loading") : t("common.refresh")}
          </Button>
        </div>
      </div>
      <div className="metric-grid mt-4">
        <div className="metric-card">
          <div className="metric-label">{t("common.records")}</div>
          <div className="metric-value">{totalCount}</div>
        </div>
        <div className="metric-card">
          <div className="metric-label">{t("common.displayed")}</div>
          <div className="metric-value">{summary.displayed}</div>
        </div>
        <div className="metric-card">
          <div className="metric-label">{t("common.inputTokens")}</div>
          <div className="metric-value">{summary.inputTokens}</div>
        </div>
        <div className="metric-card">
          <div className="metric-label">{t("common.outputTokens")}</div>
          <div className="metric-value">{summary.outputTokens}</div>
        </div>
        <div className="metric-card">
          <div className="metric-label">{t("common.cacheTokens")}</div>
          <div className="metric-value">{summary.cachedTokens}</div>
        </div>
      </div>
      <div className="record-list mt-4">
        {rows.length === 0 ? <p className="text-sm text-muted">{t("common.noData")}</p> : null}
        {rows.map((row) => (
          <div key={row.trace_id} className="record-item">
            <div className="flex flex-col gap-3 lg:flex-row lg:items-start lg:justify-between">
              <div className="min-w-0">
                <div className="flex flex-wrap items-center gap-2">
                  <div className="font-semibold text-text">{row.model ?? row.operation}</div>
                  <Badge variant="neutral">#{row.trace_id}</Badge>
                  <Badge variant="accent">{row.protocol}</Badge>
                  {row.provider_channel ? <Badge variant="neutral">{row.provider_channel}</Badge> : null}
                  <Badge variant="neutral">{formatTimestamp(row.at)}</Badge>
                </div>
                <div className="mt-2 flex flex-wrap items-center gap-2">
                  {row.provider_id !== null && row.provider_id !== undefined ? (
                    <Badge variant="neutral">provider #{row.provider_id}</Badge>
                  ) : null}
                  {row.credential_id !== null && row.credential_id !== undefined ? (
                    <Badge variant="neutral">credential #{row.credential_id}</Badge>
                  ) : null}
                  {row.user_key_id !== null && row.user_key_id !== undefined ? (
                    <Badge variant="neutral">key #{row.user_key_id}</Badge>
                  ) : null}
                </div>
              </div>
              <div className="grid gap-2 sm:grid-cols-3 lg:min-w-[320px]">
                <div className="rounded-xl border border-border px-3 py-2">
                  <div className="text-[11px] font-semibold uppercase tracking-[0.08em] text-muted">
                    {t("common.inputTokens")}
                  </div>
                  <div className="mt-1 font-semibold text-text">{row.input_tokens ?? 0}</div>
                </div>
                <div className="rounded-xl border border-border px-3 py-2">
                  <div className="text-[11px] font-semibold uppercase tracking-[0.08em] text-muted">
                    {t("common.outputTokens")}
                  </div>
                  <div className="mt-1 font-semibold text-text">{row.output_tokens ?? 0}</div>
                </div>
                <div className="rounded-xl border border-border px-3 py-2">
                  <div className="text-[11px] font-semibold uppercase tracking-[0.08em] text-muted">
                    {t("common.cacheTokens")}
                  </div>
                  <div className="mt-1 font-semibold text-text">
                    {(row.cache_read_input_tokens ?? 0) +
                      (row.cache_creation_input_tokens ?? 0) +
                      (row.cache_creation_input_tokens_5min ?? 0) +
                      (row.cache_creation_input_tokens_1h ?? 0)}
                  </div>
                </div>
              </div>
            </div>
          </div>
        ))}
      </div>
    </Card>
  );
}
