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

  useEffect(() => {
    void apiJson<HealthResponse>("/admin/health", { method: "GET", headers }).then(setHealth);
  }, [headers]);

  return (
    <Card title={t("dashboard.title")}>
      <div className="grid gap-3 md:grid-cols-4">
        <div className="card-shell">
          <div className="text-xs text-muted">status</div>
          <div className="mt-1 text-xl font-semibold">{health?.status ?? "—"}</div>
        </div>
        <div className="card-shell">
          <div className="text-xs text-muted">providers</div>
          <div className="mt-1 text-xl font-semibold">{health?.provider_count ?? "—"}</div>
        </div>
        <div className="card-shell">
          <div className="text-xs text-muted">users</div>
          <div className="mt-1 text-xl font-semibold">{health?.user_count ?? "—"}</div>
        </div>
        <div className="card-shell">
          <div className="text-xs text-muted">timestamp</div>
          <div className="mt-1 text-xl font-semibold">{health?.timestamp_epoch ?? "—"}</div>
        </div>
      </div>
    </Card>
  );
}
