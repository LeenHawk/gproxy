import { useEffect, useMemo, useState } from "react";

import { useI18n } from "../../app/i18n";
import { Card } from "../../components/ui";
import { apiJson } from "../../lib/api";
import { authHeaders } from "../../lib/auth";
import type { UsageQueryRow } from "../../lib/types/shared";

export function MyUsageModule({ sessionToken }: { sessionToken: string }) {
  const { t } = useI18n();
  const [rows, setRows] = useState<UsageQueryRow[]>([]);
  const headers = useMemo(() => authHeaders(sessionToken), [sessionToken]);

  useEffect(() => {
    void apiJson<UsageQueryRow[]>("/user/usages/query", {
      method: "POST",
      headers,
      body: JSON.stringify({ limit: 50 }),
    }).then(setRows);
  }, [headers]);

  return (
    <Card title={t("myUsage.title")} subtitle={t("myUsage.subtitle")}>
      <div className="record-list">
        {rows.map((row) => (
          <div key={row.trace_id} className="record-item">
            <div className="font-semibold text-text">{row.model ?? row.operation}</div>
            <div className="mt-1 text-xs text-muted">
              trace={row.trace_id} · protocol={row.protocol} · input={row.input_tokens ?? 0} · output={row.output_tokens ?? 0}
            </div>
          </div>
        ))}
      </div>
    </Card>
  );
}
