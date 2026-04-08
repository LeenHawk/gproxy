import { useMemo, useState } from "react";

import { useI18n } from "../../app/i18n";
import { Button, Card, Input, Label } from "../../components/ui";
import { apiJson, apiVoid } from "../../lib/api";
import { authHeaders } from "../../lib/auth";
import type { DownstreamRequestQueryRow, UpstreamRequestQueryRow } from "../../lib/types/admin";
import { buildDownstreamRequestQuery } from "./requests-filter";

export function RequestsModule({
  sessionToken,
  notify,
}: {
  sessionToken: string;
  notify: (kind: "success" | "error" | "info", message: string) => void;
}) {
  const { t } = useI18n();
  const headers = useMemo(() => authHeaders(sessionToken), [sessionToken]);
  const [tab, setTab] = useState<"downstream" | "upstream">("downstream");
  const [pathFilter, setPathFilter] = useState("");
  const [limit, setLimit] = useState("50");
  const [includeBody, setIncludeBody] = useState(false);
  const [downstreamRows, setDownstreamRows] = useState<DownstreamRequestQueryRow[]>([]);
  const [upstreamRows, setUpstreamRows] = useState<UpstreamRequestQueryRow[]>([]);

  const query = async () => {
    try {
      if (tab === "downstream") {
        const rows = await apiJson<DownstreamRequestQueryRow[]>("/admin/requests/downstream/query", {
          method: "POST",
          headers,
          body: JSON.stringify(
            buildDownstreamRequestQuery({
              request_path_contains: pathFilter,
              limit,
              include_body: includeBody,
            }),
          ),
        });
        setDownstreamRows(rows);
      } else {
        const rows = await apiJson<UpstreamRequestQueryRow[]>("/admin/requests/upstream/query", {
          method: "POST",
          headers,
          body: JSON.stringify({
            ...(limit.trim() ? { limit: Number(limit) } : {}),
            include_body: includeBody,
          }),
        });
        setUpstreamRows(rows);
      }
    } catch (error) {
      notify("error", error instanceof Error ? error.message : String(error));
    }
  };

  const deleteSelected = async (traceIds: number[]) => {
    if (traceIds.length === 0) return;
    try {
      const path =
        tab === "downstream"
          ? "/admin/requests/downstream/batch-delete"
          : "/admin/requests/upstream/batch-delete";
      await apiVoid(path, {
        method: "POST",
        headers,
        body: JSON.stringify(traceIds),
      });
      notify("success", "Requests deleted");
      await query();
    } catch (error) {
      notify("error", error instanceof Error ? error.message : String(error));
    }
  };

  const rows = tab === "downstream" ? downstreamRows : upstreamRows;

  return (
    <Card title={t("requests.title")}>
      <div className="flex flex-wrap gap-2">
        <Button variant={tab === "downstream" ? "primary" : "neutral"} onClick={() => setTab("downstream")}>Downstream</Button>
        <Button variant={tab === "upstream" ? "primary" : "neutral"} onClick={() => setTab("upstream")}>Upstream</Button>
      </div>
      <div className="mt-4 grid gap-4 lg:grid-cols-3">
        <div>
          <Label>{tab === "downstream" ? "request_path_contains" : "path filter"}</Label>
          <Input value={pathFilter} onChange={setPathFilter} />
        </div>
        <div>
          <Label>limit</Label>
          <Input value={limit} onChange={setLimit} />
        </div>
        <label className="flex items-center gap-2 text-sm text-muted">
          <input type="checkbox" checked={includeBody} onChange={(event) => setIncludeBody(event.target.checked)} />
          include_body
        </label>
      </div>
      <div className="mt-4 flex gap-2">
        <Button onClick={() => void query()}>Query</Button>
        <Button variant="danger" onClick={() => void deleteSelected(rows.slice(0, 5).map((row) => row.trace_id))}>Delete First 5</Button>
      </div>
      <div className="mt-4 space-y-2">
        {rows.map((row) => (
          <div key={row.trace_id} className="card-shell">
            <div className="font-semibold">trace #{row.trace_id}</div>
            <div className="text-xs text-muted">
              {"request_path" in row ? row.request_path : row.request_url} · status={row.response_status ?? "—"}
            </div>
          </div>
        ))}
      </div>
    </Card>
  );
}
