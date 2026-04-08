import { useMemo, useState } from "react";

import { useI18n } from "../../app/i18n";
import { Button, Card, Input, Label } from "../../components/ui";
import { apiJson, apiVoid } from "../../lib/api";
import { authHeaders } from "../../lib/auth";
import type { UsageQueryRow } from "../../lib/types/shared";

export function UsageModule({
  sessionToken,
  notify,
}: {
  sessionToken: string;
  notify: (kind: "success" | "error" | "info", message: string) => void;
}) {
  const { t } = useI18n();
  const headers = useMemo(() => authHeaders(sessionToken), [sessionToken]);
  const [limit, setLimit] = useState("50");
  const [rows, setRows] = useState<UsageQueryRow[]>([]);

  const query = async () => {
    try {
      const data = await apiJson<UsageQueryRow[]>("/admin/usages/query", {
        method: "POST",
        headers,
        body: JSON.stringify({ ...(limit.trim() ? { limit: Number(limit) } : {}) }),
      });
      setRows(data);
    } catch (error) {
      notify("error", error instanceof Error ? error.message : String(error));
    }
  };

  const deleteFirst = async () => {
    if (rows.length === 0) return;
    try {
      await apiVoid("/admin/usages/batch-delete", {
        method: "POST",
        headers,
        body: JSON.stringify(rows.slice(0, 5).map((row) => row.trace_id)),
      });
      notify("success", t("usages.deleted"));
      await query();
    } catch (error) {
      notify("error", error instanceof Error ? error.message : String(error));
    }
  };

  return (
    <Card title={t("usages.title")}>
      <div className="grid gap-4 lg:grid-cols-2">
        <div>
          <Label>{t("common.limit")}</Label>
          <Input value={limit} onChange={setLimit} />
        </div>
      </div>
      <div className="mt-4 flex gap-2">
        <Button onClick={() => void query()}>{t("common.query")}</Button>
        <Button variant="danger" onClick={() => void deleteFirst()}>{t("common.deleteFirst5")}</Button>
      </div>
      <div className="mt-4 space-y-2">
        {rows.map((row) => (
          <div key={row.trace_id} className="card-shell">
            <div className="font-semibold">{row.model ?? row.operation}</div>
            <div className="text-xs text-muted">
              {t("usages.rowMeta", { trace: row.trace_id, protocol: row.protocol })}
            </div>
          </div>
        ))}
      </div>
    </Card>
  );
}
