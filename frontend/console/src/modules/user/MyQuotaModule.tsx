import { useEffect, useMemo, useState } from "react";

import { useI18n } from "../../app/i18n";
import { Card } from "../../components/ui";
import { apiJson } from "../../lib/api";
import { authHeaders } from "../../lib/auth";
import type { QuotaResponse } from "../../lib/types/user";

export function MyQuotaModule({ sessionToken }: { sessionToken: string }) {
  const { t } = useI18n();
  const [quota, setQuota] = useState<QuotaResponse | null>(null);
  const headers = useMemo(() => authHeaders(sessionToken, false), [sessionToken]);

  useEffect(() => {
    void apiJson<QuotaResponse>("/user/quota", {
      method: "GET",
      headers,
    }).then(setQuota);
  }, [headers]);

  return (
    <Card title={t("myQuota.title")} subtitle={t("myQuota.subtitle")}>
      <div className="metric-grid">
        <div className="metric-card">
          <div className="metric-label">{t("common.userId")}</div>
          <div className="metric-value">{quota?.user_id ?? "—"}</div>
        </div>
        <div className="metric-card">
          <div className="metric-label">{t("common.quota")}</div>
          <div className="metric-value">{quota?.quota ?? "—"}</div>
        </div>
        <div className="metric-card">
          <div className="metric-label">{t("common.costUsed")}</div>
          <div className="metric-value">{quota?.cost_used ?? "—"}</div>
        </div>
        <div className="metric-card">
          <div className="metric-label">{t("common.remaining")}</div>
          <div className="metric-value">{quota?.remaining ?? "—"}</div>
        </div>
      </div>
    </Card>
  );
}
