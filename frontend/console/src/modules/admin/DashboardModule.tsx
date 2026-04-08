import { useEffect, useMemo, useState } from "react";

import { useI18n } from "../../app/i18n";
import { Card } from "../../components/ui";
import { apiJson } from "../../lib/api";
import { authHeaders } from "../../lib/auth";
import type { HealthResponse } from "../../lib/types/admin";

export function DashboardModule({ sessionToken }: { sessionToken: string }) {
  const { t } = useI18n();
  const [health, setHealth] = useState<HealthResponse | null>(null);
  const headers = useMemo(() => authHeaders(sessionToken, false), [sessionToken]);
  const lastUpdated = health ? new Date(health.timestamp_epoch * 1000).toLocaleString() : "—";

  useEffect(() => {
    void apiJson<HealthResponse>("/admin/health", { method: "GET", headers }).then(setHealth);
  }, [headers]);

  return (
    <Card title={t("dashboard.title")}>
      <div className="metric-grid">
        <div className="metric-card">
          <div className="metric-label">{t("dashboard.metric.status")}</div>
          <div className="metric-value">{health?.status ?? "—"}</div>
        </div>
        <div className="metric-card">
          <div className="metric-label">{t("dashboard.metric.providers")}</div>
          <div className="metric-value">{health?.provider_count ?? "—"}</div>
        </div>
        <div className="metric-card">
          <div className="metric-label">{t("dashboard.metric.users")}</div>
          <div className="metric-value">{health?.user_count ?? "—"}</div>
        </div>
        <div className="metric-card">
          <div className="metric-label">{t("dashboard.metric.timestamp")}</div>
          <div className="metric-value">{lastUpdated}</div>
          <div className="metric-meta">{health ? `epoch ${health.timestamp_epoch}` : t("common.loading")}</div>
        </div>
      </div>
      <div className="panel-shell mt-4">
        <div className="flex flex-col gap-3 lg:flex-row lg:items-end lg:justify-between">
          <div>
            <div className="metric-label">{t("dashboard.metric.timestamp")}</div>
            <div className="mt-2 text-2xl font-semibold tracking-[-0.03em] text-text">{lastUpdated}</div>
            <p className="mt-2 text-sm text-muted">
              {health ? `${t("dashboard.metric.status")}: ${health.status}` : t("common.loading")}
            </p>
          </div>
          <div className="grid gap-3 sm:grid-cols-2 lg:min-w-[320px]">
            <div>
              <div className="metric-label">{t("dashboard.metric.providers")}</div>
              <div className="mt-2 text-lg font-semibold text-text">{health?.provider_count ?? "—"}</div>
            </div>
            <div>
              <div className="metric-label">{t("dashboard.metric.users")}</div>
              <div className="mt-2 text-lg font-semibold text-text">{health?.user_count ?? "—"}</div>
            </div>
          </div>
        </div>
      </div>
    </Card>
  );
}
