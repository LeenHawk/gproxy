import { useEffect, useMemo, useState } from "react";

import { useI18n } from "../../app/i18n";
import { Card } from "../../components/ui";
import { apiJson } from "../../lib/api";
import { authHeaders } from "../../lib/auth";
import { APP_BUILD_INFO } from "../../lib/build-info";
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
        <div className="metric-card">
          <div className="metric-label">{t("dashboard.metric.version")}</div>
          <div className="metric-value">{APP_BUILD_INFO.version}</div>
        </div>
        <div className="metric-card">
          <div className="metric-label">{t("dashboard.metric.commit")}</div>
          <div className="metric-value font-mono text-[1.15rem]">{APP_BUILD_INFO.commit}</div>
        </div>
      </div>
    </Card>
  );
}
