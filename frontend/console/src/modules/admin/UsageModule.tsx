import { useEffect, useMemo, useState } from "react";

import { useI18n } from "../../app/i18n";
import { Button, Card, Input, Label, SearchableSelect, Select } from "../../components/ui";
import { apiJson, apiVoid } from "../../lib/api";
import { authHeaders } from "../../lib/auth";
import type { CredentialRow, MemoryModelRow, MemoryUserKeyRow, MemoryUserRow, ProviderRow } from "../../lib/types/admin";
import type { UsageQueryRow } from "../../lib/types/shared";
import { buildAdminUsageQuery } from "./requests-filter";

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
  const [users, setUsers] = useState<MemoryUserRow[]>([]);
  const [userKeys, setUserKeys] = useState<MemoryUserKeyRow[]>([]);
  const [providers, setProviders] = useState<ProviderRow[]>([]);
  const [credentials, setCredentials] = useState<CredentialRow[]>([]);
  const [models, setModels] = useState<MemoryModelRow[]>([]);
  const [selectedUserId, setSelectedUserId] = useState("");
  const [selectedUserKeyId, setSelectedUserKeyId] = useState("");
  const [selectedProviderId, setSelectedProviderId] = useState("");
  const [selectedCredentialId, setSelectedCredentialId] = useState("");
  const [selectedChannel, setSelectedChannel] = useState("");
  const [selectedModel, setSelectedModel] = useState("");

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
      apiJson<MemoryModelRow[]>("/admin/models/query", {
        method: "POST",
        headers,
        body: JSON.stringify({}),
      }),
    ])
      .then(([userRows, userKeyRows, providerRows, credentialRows, modelRows]) => {
        setUsers(userRows);
        setUserKeys(userKeyRows);
        setProviders(providerRows);
        setCredentials(credentialRows);
        setModels(modelRows);
      })
      .catch((error) => notify("error", error instanceof Error ? error.message : String(error)));
  }, [headers, notify]);

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
  const channelOptions = useMemo(
    () => [
      { value: "", label: `${t("common.all")} ${t("providers.form.channel")}` },
      ...Array.from(new Set(providers.map((provider) => provider.channel)))
        .sort()
        .map((value) => ({ value, label: value })),
    ],
    [providers, t],
  );
  const modelOptions = useMemo(
      () =>
      Array.from(
        new Set(
          models
            .map((row) => row.model_id)
            .filter(Boolean),
        ),
      )
        .sort()
        .map((value) => ({ value, label: value })),
    [models],
  );

  const query = async () => {
    try {
      const data = await apiJson<UsageQueryRow[]>("/admin/usages/query", {
        method: "POST",
        headers,
        body: JSON.stringify(
          buildAdminUsageQuery({
            provider_id: selectedProviderId,
            credential_id: selectedCredentialId,
            channel: selectedChannel,
            model: selectedModel,
            user_id: selectedUserId,
            user_key_id: selectedUserKeyId,
            limit,
          }),
        ),
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
      <div className="toolbar-shell">
        <div className="grid gap-4 lg:grid-cols-3">
          <div>
            <Label>{t("common.user")}</Label>
            <Select value={selectedUserId} onChange={setSelectedUserId} options={userOptions} />
          </div>
          <div>
            <Label>{t("app.nav.myKeys")}</Label>
            <Select value={selectedUserKeyId} onChange={setSelectedUserKeyId} options={userKeyOptions} />
          </div>
          <div>
            <Label>{t("common.provider")}</Label>
            <Select value={selectedProviderId} onChange={setSelectedProviderId} options={providerOptions} />
          </div>
          <div>
            <Label>{t("providers.tab.credentials")}</Label>
            <Select value={selectedCredentialId} onChange={setSelectedCredentialId} options={credentialOptions} />
          </div>
          <div>
            <Label>{t("providers.form.channel")}</Label>
            <Select value={selectedChannel} onChange={setSelectedChannel} options={channelOptions} />
          </div>
          <div>
            <Label>{t("common.modelId")}</Label>
            <SearchableSelect value={selectedModel} onChange={setSelectedModel} options={modelOptions} />
          </div>
          <div>
            <Label>{t("common.limit")}</Label>
            <Input value={limit} onChange={setLimit} />
          </div>
        </div>
        <div className="toolbar-actions">
          <Button onClick={() => void query()}>{t("common.query")}</Button>
          <Button variant="danger" onClick={() => void deleteFirst()}>{t("common.deleteFirst5")}</Button>
        </div>
      </div>
      <div className="record-list mt-4">
        {rows.length === 0 ? <p className="text-sm text-muted">{t("common.noData")}</p> : null}
        {rows.map((row) => (
          <div key={row.trace_id} className="record-item">
            <div className="font-semibold text-text">{row.model ?? row.operation}</div>
            <div className="mt-1 text-xs text-muted">
              {t("usages.rowMeta", { trace: row.trace_id, protocol: row.protocol })}
            </div>
          </div>
        ))}
      </div>
    </Card>
  );
}
