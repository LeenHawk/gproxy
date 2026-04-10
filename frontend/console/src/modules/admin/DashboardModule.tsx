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

  useEffect(() => {
    void apiJson<HealthResponse>("/admin/health", { method: "GET", headers }).then(setHealth);
  }, [headers]);

  /// Version + short commit rendered as bordered pills next to the card
  /// title, matching the build-info strip in the sample gproxy About card.
  const buildInfoAction = (
    <div className="flex flex-wrap items-center gap-3 text-xs text-muted">
      <span>
        {t("dashboard.metric.version")}:{" "}
        <code className="rounded border border-border px-1.5 py-0.5 font-mono text-[12px] text-text">
          {APP_BUILD_INFO.version}
        </code>
      </span>
      <span>
        {t("dashboard.metric.commit")}:{" "}
        <code className="rounded border border-border px-1.5 py-0.5 font-mono text-[12px] text-text">
          {APP_BUILD_INFO.commit}
        </code>
      </span>
    </div>
  );

  return (
    <Card title={t("dashboard.title")} action={buildInfoAction}>
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
      </div>
    </Card>
  );
}
