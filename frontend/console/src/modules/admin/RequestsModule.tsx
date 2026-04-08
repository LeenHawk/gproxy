import { useEffect, useMemo, useState } from "react";

import { useI18n } from "../../app/i18n";
import { Button, Card, Label, SearchableSelect, Select } from "../../components/ui";
import { apiJson, apiVoid } from "../../lib/api";
import { authHeaders } from "../../lib/auth";
import type {
  CredentialRow,
  DownstreamRequestQueryRow,
  MemoryUserKeyRow,
  MemoryUserRow,
  ProviderRow,
  UpstreamRequestQueryRow,
} from "../../lib/types/admin";
import type { CountResponse } from "../../lib/types/shared";
import {
  buildDownstreamDeleteAllQuery,
  buildDownstreamRequestQuery,
  buildUpstreamDeleteAllQuery,
  buildUpstreamRequestQuery,
  KNOWN_DOWNSTREAM_REQUEST_PATHS,
  KNOWN_UPSTREAM_REQUEST_TARGETS,
} from "./requests-filter";

const PAGE_SIZE = 50;

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
  const [includeBody, setIncludeBody] = useState(false);
  const [downstreamRows, setDownstreamRows] = useState<DownstreamRequestQueryRow[]>([]);
  const [upstreamRows, setUpstreamRows] = useState<UpstreamRequestQueryRow[]>([]);
  const [totalCount, setTotalCount] = useState(0);
  const [page, setPage] = useState(1);
  const [users, setUsers] = useState<MemoryUserRow[]>([]);
  const [userKeys, setUserKeys] = useState<MemoryUserKeyRow[]>([]);
  const [providers, setProviders] = useState<ProviderRow[]>([]);
  const [credentials, setCredentials] = useState<CredentialRow[]>([]);
  const [selectedUserId, setSelectedUserId] = useState("");
  const [selectedUserKeyId, setSelectedUserKeyId] = useState("");
  const [selectedProviderId, setSelectedProviderId] = useState("");
  const [selectedCredentialId, setSelectedCredentialId] = useState("");
  const [selectedTraceIds, setSelectedTraceIds] = useState<number[]>([]);

  useEffect(() => {
    void Promise.all([
      apiJson<MemoryUserRow[]>("/admin/users/query", {
        method: "POST",
        headers,
        body: JSON.stringify({}),
      }),
      apiJson<MemoryUserKeyRow[]>("/admin/user-keys/query", {
        method: "POST",
        headers,
        body: JSON.stringify({}),
      }),
      apiJson<ProviderRow[]>("/admin/providers/query", {
        method: "POST",
        headers,
        body: JSON.stringify({}),
      }),
      apiJson<CredentialRow[]>("/admin/credentials/query", {
        method: "POST",
        headers,
        body: JSON.stringify({}),
      }),
    ])
      .then(([userRows, userKeyRows, providerRows, credentialRows]) => {
        setUsers(userRows);
        setUserKeys(userKeyRows);
        setProviders(providerRows);
        setCredentials(credentialRows);
      })
      .catch((error) => notify("error", error instanceof Error ? error.message : String(error)));
  }, [headers, notify]);

  const downstreamPathOptions = useMemo(
    () => KNOWN_DOWNSTREAM_REQUEST_PATHS.map((value) => ({ value, label: value })),
    [],
  );
  const upstreamPathOptions = useMemo(
    () => KNOWN_UPSTREAM_REQUEST_TARGETS.map((value) => ({ value, label: value })),
    [],
  );
  const userOptions = useMemo(
    () => [
      { value: "", label: `${t("common.all")} ${t("common.user")}` },
      ...users.map((user) => ({ value: String(user.id), label: `${user.name} (#${user.id})` })),
    ],
    [t, users],
  );
  const userKeyOptions = useMemo(
    () => [
      { value: "", label: `${t("common.all")} ${t("app.nav.myKeys")}` },
      ...userKeys
        .filter((row) => !selectedUserId || String(row.user_id) === selectedUserId)
        .map((row) => ({ value: String(row.id), label: `#${row.id} · user #${row.user_id}` })),
    ],
    [selectedUserId, t, userKeys],
  );
  const providerOptions = useMemo(
    () => [
      { value: "", label: `${t("common.all")} ${t("common.provider")}` },
      ...providers.map((provider) => ({ value: String(provider.id), label: `${provider.name} (#${provider.id})` })),
    ],
    [providers, t],
  );
  const credentialOptions = useMemo(
    () => [
      { value: "", label: `${t("common.all")} ${t("providers.tab.credentials")}` },
      ...credentials
        .filter((row) => {
          if (!selectedProviderId) {
            return true;
          }
          const provider = providers.find((item) => item.name === row.provider);
          return provider ? String(provider.id) === selectedProviderId : false;
        })
        .map((row) => ({ value: String(row.id), label: `${row.provider} #${row.index}` })),
    ],
    [credentials, providers, selectedProviderId, t],
  );

  const query = async () => {
    await loadPage(1);
  };

  const loadPage = async (nextPage: number) => {
    try {
      const safePage = Math.max(1, nextPage);
      const offset = (safePage - 1) * PAGE_SIZE;
      if (tab === "downstream") {
        const [rows, count] = await Promise.all([
          apiJson<DownstreamRequestQueryRow[]>("/admin/requests/downstream/query", {
            method: "POST",
            headers,
            body: JSON.stringify(
              buildDownstreamRequestQuery({
                user_id: selectedUserId,
                user_key_id: selectedUserKeyId,
                request_path_contains: pathFilter,
                limit: String(PAGE_SIZE),
                offset,
                include_body: includeBody,
              }),
            ),
          }),
          apiJson<CountResponse>("/admin/requests/downstream/count", {
            method: "POST",
            headers,
            body: JSON.stringify(
              buildDownstreamDeleteAllQuery({
                user_id: selectedUserId,
                user_key_id: selectedUserKeyId,
                request_path_contains: pathFilter,
                limit: "",
                offset: 0,
                include_body: includeBody,
              }),
            ),
          }),
        ]);
        setDownstreamRows(rows);
        setTotalCount(count.count);
        setPage(Math.min(safePage, Math.max(1, Math.ceil(count.count / PAGE_SIZE))));
      } else {
        const [rows, count] = await Promise.all([
          apiJson<UpstreamRequestQueryRow[]>("/admin/requests/upstream/query", {
            method: "POST",
            headers,
            body: JSON.stringify(
              buildUpstreamRequestQuery({
                provider_id: selectedProviderId,
                credential_id: selectedCredentialId,
                request_url_contains: pathFilter,
                limit: String(PAGE_SIZE),
                offset,
                include_body: includeBody,
              }),
            ),
          }),
          apiJson<CountResponse>("/admin/requests/upstream/count", {
            method: "POST",
            headers,
            body: JSON.stringify(
              buildUpstreamDeleteAllQuery({
                provider_id: selectedProviderId,
                credential_id: selectedCredentialId,
                request_url_contains: pathFilter,
                limit: "",
                offset: 0,
                include_body: includeBody,
              }),
            ),
          }),
        ]);
        setUpstreamRows(rows);
        setTotalCount(count.count);
        setPage(Math.min(safePage, Math.max(1, Math.ceil(count.count / PAGE_SIZE))));
      }
      setSelectedTraceIds([]);
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
      notify("success", t("requests.deleted"));
      setSelectedTraceIds([]);
      await query();
    } catch (error) {
      notify("error", error instanceof Error ? error.message : String(error));
    }
  };

  const rows = tab === "downstream" ? downstreamRows : upstreamRows;
  const pageCount = Math.max(1, Math.ceil(totalCount / PAGE_SIZE));
  const allVisibleSelected = rows.length > 0 && rows.every((row) => selectedTraceIds.includes(row.trace_id));

  const toggleTraceId = (traceId: number, checked: boolean) => {
    setSelectedTraceIds((current) =>
      checked ? Array.from(new Set([...current, traceId])) : current.filter((id) => id !== traceId),
    );
  };

  const toggleSelectAllVisible = (checked: boolean) => {
    const visibleIds = rows.map((row) => row.trace_id);
    setSelectedTraceIds((current) =>
      checked ? Array.from(new Set([...current, ...visibleIds])) : current.filter((id) => !visibleIds.includes(id)),
    );
  };

  const deleteAll = async () => {
    try {
      const traceIds =
        tab === "downstream"
          ? (
              await apiJson<DownstreamRequestQueryRow[]>("/admin/requests/downstream/query", {
                method: "POST",
                headers,
                body: JSON.stringify(
                  buildDownstreamDeleteAllQuery({
                    user_id: selectedUserId,
                    user_key_id: selectedUserKeyId,
                    request_path_contains: pathFilter,
                    limit: "",
                    offset: 0,
                    include_body: includeBody,
                  }),
                ),
              })
            ).map((row) => row.trace_id)
          : (
              await apiJson<UpstreamRequestQueryRow[]>("/admin/requests/upstream/query", {
                method: "POST",
                headers,
                body: JSON.stringify(
                  buildUpstreamDeleteAllQuery({
                    provider_id: selectedProviderId,
                    credential_id: selectedCredentialId,
                    request_url_contains: pathFilter,
                    limit: "",
                    offset: 0,
                    include_body: includeBody,
                  }),
                ),
              })
            ).map((row) => row.trace_id);
      await deleteSelected(traceIds);
    } catch (error) {
      notify("error", error instanceof Error ? error.message : String(error));
    }
  };

  return (
    <Card title={t("requests.title")}>
      <div className="toolbar-shell">
        <div className="flex flex-wrap gap-2">
          <Button variant={tab === "downstream" ? "primary" : "neutral"} onClick={() => setTab("downstream")}>
            {t("common.downstream")}
          </Button>
          <Button variant={tab === "upstream" ? "primary" : "neutral"} onClick={() => setTab("upstream")}>
            {t("common.upstream")}
          </Button>
        </div>
        <div className="grid gap-4 lg:grid-cols-[minmax(0,1fr)_auto] lg:items-end">
          <div className="grid gap-4 md:grid-cols-2">
            {tab === "downstream" ? (
              <>
                <div>
                  <Label>{t("common.user")}</Label>
                  <Select value={selectedUserId} onChange={setSelectedUserId} options={userOptions} />
                </div>
                <div>
                  <Label>{t("app.nav.myKeys")}</Label>
                  <Select value={selectedUserKeyId} onChange={setSelectedUserKeyId} options={userKeyOptions} />
                </div>
              </>
            ) : (
              <>
                <div>
                  <Label>{t("common.provider")}</Label>
                  <Select value={selectedProviderId} onChange={setSelectedProviderId} options={providerOptions} />
                </div>
                <div>
                  <Label>{t("providers.tab.credentials")}</Label>
                  <Select value={selectedCredentialId} onChange={setSelectedCredentialId} options={credentialOptions} />
                </div>
              </>
            )}
            <div className="md:col-span-2">
              <Label>{tab === "downstream" ? t("requests.requestPathContains") : t("requests.pathFilter")}</Label>
              <SearchableSelect
                value={pathFilter}
                onChange={setPathFilter}
                options={tab === "downstream" ? downstreamPathOptions : upstreamPathOptions}
              />
            </div>
          </div>
          <label className="flex h-[42px] items-center gap-2 text-sm text-muted">
            <input type="checkbox" checked={includeBody} onChange={(event) => setIncludeBody(event.target.checked)} />
            {t("requests.includeBody")}
          </label>
        </div>
        <div className="toolbar-actions">
          <Button onClick={() => void query()}>{t("common.query")}</Button>
          <Button variant="neutral" onClick={() => void loadPage(page - 1)} disabled={page <= 1}>
            {t("common.previousPage")}
          </Button>
          <Button variant="neutral" onClick={() => void loadPage(page + 1)} disabled={page >= pageCount}>
            {t("common.nextPage")}
          </Button>
          <Button variant="danger" onClick={() => void deleteSelected(selectedTraceIds)} disabled={selectedTraceIds.length === 0}>
            {t("common.deleteSelected")}
          </Button>
          <Button variant="danger" onClick={() => void deleteAll()} disabled={rows.length === 0}>
            {t("common.deleteAll")}
          </Button>
        </div>
        <div className="text-sm text-muted">{t("common.pageSummary", { page, pageCount, total: totalCount })}</div>
      </div>
      <div className="record-list mt-4">
        {rows.length === 0 ? <p className="text-sm text-muted">{t("common.noData")}</p> : null}
        {rows.length > 0 ? (
          <label className="flex items-center gap-2 text-sm text-muted">
            <input
              type="checkbox"
              checked={allVisibleSelected}
              onChange={(event) => toggleSelectAllVisible(event.target.checked)}
            />
            {t("common.selectVisible")}
          </label>
        ) : null}
        {rows.map((row) => (
          <div key={row.trace_id} className="record-item">
            <div className="flex items-start gap-3">
              <input
                type="checkbox"
                checked={selectedTraceIds.includes(row.trace_id)}
                onChange={(event) => toggleTraceId(row.trace_id, event.target.checked)}
              />
              <div className="min-w-0">
                <div className="font-semibold text-text">trace #{row.trace_id}</div>
                <div className="mt-1 text-xs text-muted">
                  {t("requests.rowMeta", {
                    target: "request_path" in row ? row.request_path : row.request_url ?? "—",
                    status: row.response_status ?? "—",
                  })}
                </div>
              </div>
            </div>
          </div>
        ))}
      </div>
    </Card>
  );
}
