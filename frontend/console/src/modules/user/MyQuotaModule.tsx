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
      <div className="grid gap-3 md:grid-cols-4">
        <div className="card-shell">
          <div className="text-xs text-muted">user_id</div>
          <div className="mt-1 text-xl font-semibold">{quota?.user_id ?? "—"}</div>
        </div>
        <div className="card-shell">
          <div className="text-xs text-muted">quota</div>
          <div className="mt-1 text-xl font-semibold">{quota?.quota ?? "—"}</div>
        </div>
        <div className="card-shell">
          <div className="text-xs text-muted">cost_used</div>
          <div className="mt-1 text-xl font-semibold">{quota?.cost_used ?? "—"}</div>
        </div>
        <div className="card-shell">
          <div className="text-xs text-muted">remaining</div>
          <div className="mt-1 text-xl font-semibold">{quota?.remaining ?? "—"}</div>
        </div>
      </div>
    </Card>
  );
}
