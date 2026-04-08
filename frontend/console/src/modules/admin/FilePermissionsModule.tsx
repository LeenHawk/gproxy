import { useEffect, useMemo, useState } from "react";

import { useI18n } from "../../app/i18n";
import { Button, Card, Input, Label, Select } from "../../components/ui";
import { apiJson, apiVoid } from "../../lib/api";
import { authHeaders } from "../../lib/auth";
import { parseRequiredI64 } from "../../lib/form";
import type { MemoryFilePermissionRow, MemoryUserRow, ProviderRow, UserFilePermissionWrite } from "../../lib/types/admin";

export function FilePermissionsModule({
  sessionToken,
  notify,
}: {
  sessionToken: string;
  notify: (kind: "success" | "error" | "info", message: string) => void;
}) {
  const { t } = useI18n();
  const headers = useMemo(() => authHeaders(sessionToken), [sessionToken]);
  const [users, setUsers] = useState<MemoryUserRow[]>([]);
  const [providers, setProviders] = useState<ProviderRow[]>([]);
  const [rows, setRows] = useState<MemoryFilePermissionRow[]>([]);
  const [selectedId, setSelectedId] = useState<number | null>(null);
  const [form, setForm] = useState({ id: "", user_id: "", provider_id: "" });

  const load = async () => {
    const [userRows, providerRows, permissionRows] = await Promise.all([
      apiJson<MemoryUserRow[]>("/admin/users/query", { method: "POST", headers, body: JSON.stringify({}) }),
      apiJson<ProviderRow[]>("/admin/providers/query", { method: "POST", headers, body: JSON.stringify({}) }),
      apiJson<MemoryFilePermissionRow[]>("/admin/user-file-permissions/query", { method: "POST", headers, body: JSON.stringify({}) }),
    ]);
    setUsers(userRows);
    setProviders(providerRows);
    setRows(permissionRows);
  };

  useEffect(() => {
    void load().catch((error) => notify("error", error instanceof Error ? error.message : String(error)));
  }, []);

  const save = async () => {
    try {
      const payload: UserFilePermissionWrite = {
        id: parseRequiredI64(form.id, "id"),
        user_id: parseRequiredI64(form.user_id, "user_id"),
        provider_id: parseRequiredI64(form.provider_id, "provider_id"),
      };
      await apiJson("/admin/user-file-permissions/upsert", { method: "POST", headers, body: JSON.stringify(payload) });
      notify("success", "File permission saved");
      await load();
    } catch (error) {
      notify("error", error instanceof Error ? error.message : String(error));
    }
  };

  const remove = async (id: number) => {
    try {
      await apiVoid("/admin/user-file-permissions/delete", { method: "POST", headers, body: JSON.stringify({ id }) });
      notify("success", "File permission deleted");
      await load();
    } catch (error) {
      notify("error", error instanceof Error ? error.message : String(error));
    }
  };

  return (
    <Card title={t("filePermissions.title")}>
      <div className="grid gap-4 xl:grid-cols-[360px_minmax(0,1fr)]">
        <div className="space-y-2">
          {rows.map((row) => (
            <div
              key={row.id}
              className={`card-shell cursor-pointer ${row.id === selectedId ? "nav-item-active" : ""}`}
              onClick={() => {
                setSelectedId(row.id);
                setForm({ id: String(row.id), user_id: String(row.user_id), provider_id: String(row.provider_id) });
              }}
            >
              <div className="font-semibold">#{row.id}</div>
              <div className="text-xs text-muted">user #{row.user_id} · provider #{row.provider_id}</div>
            </div>
          ))}
        </div>
        <div className="card-shell space-y-3">
          <div>
            <Label>ID</Label>
            <Input value={form.id} onChange={(value) => setForm((current) => ({ ...current, id: value }))} />
          </div>
          <div>
            <Label>User</Label>
            <Select value={form.user_id} onChange={(value) => setForm((current) => ({ ...current, user_id: value }))} options={users.map((user) => ({ value: String(user.id), label: `${user.name} (#${user.id})` }))} />
          </div>
          <div>
            <Label>Provider</Label>
            <Select value={form.provider_id} onChange={(value) => setForm((current) => ({ ...current, provider_id: value }))} options={providers.map((provider) => ({ value: String(provider.id), label: provider.name }))} />
          </div>
          <div className="flex gap-2">
            <Button onClick={() => void save()}>Save</Button>
            {selectedId ? <Button variant="danger" onClick={() => void remove(selectedId)}>Delete</Button> : null}
          </div>
        </div>
      </div>
    </Card>
  );
}
