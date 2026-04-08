import { useEffect, useMemo, useState } from "react";

import { useI18n } from "../../app/i18n";
import { Button, Card, Input, Label, Select } from "../../components/ui";
import { apiJson, apiVoid } from "../../lib/api";
import { authHeaders } from "../../lib/auth";
import { parseRequiredI64 } from "../../lib/form";
import type { MemoryPermissionRow, MemoryUserRow, ProviderRow, UserModelPermissionWrite } from "../../lib/types/admin";

export function PermissionsModule({
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
  const [rows, setRows] = useState<MemoryPermissionRow[]>([]);
  const [selectedId, setSelectedId] = useState<number | null>(null);
  const [form, setForm] = useState({ id: "", user_id: "", provider_id: "", model_pattern: "" });
  const nextId = useMemo(
    () => rows.reduce((max, row) => Math.max(max, row.id), 0) + 1,
    [rows],
  );

  const beginCreate = () => {
    setSelectedId(null);
    setForm({
      id: String(nextId),
      user_id: users[0] ? String(users[0].id) : "",
      provider_id: "",
      model_pattern: "",
    });
  };

  const load = async () => {
    const [userRows, providerRows, permissionRows] = await Promise.all([
      apiJson<MemoryUserRow[]>("/admin/users/query", { method: "POST", headers, body: JSON.stringify({}) }),
      apiJson<ProviderRow[]>("/admin/providers/query", { method: "POST", headers, body: JSON.stringify({}) }),
      apiJson<MemoryPermissionRow[]>("/admin/user-permissions/query", { method: "POST", headers, body: JSON.stringify({}) }),
    ]);
    setUsers(userRows);
    setProviders(providerRows);
    setRows(permissionRows);
  };

  useEffect(() => {
    void load().catch((error) => notify("error", error instanceof Error ? error.message : String(error)));
  }, []);

  useEffect(() => {
    if (!selectedId && !form.id) {
      beginCreate();
    }
  }, [form.id, nextId, selectedId, users]);

  const save = async () => {
    try {
      const payload: UserModelPermissionWrite = {
        id: parseRequiredI64(form.id, "id"),
        user_id: parseRequiredI64(form.user_id, "user_id"),
        provider_id: form.provider_id ? parseRequiredI64(form.provider_id, "provider_id") : null,
        model_pattern: form.model_pattern.trim(),
      };
      await apiJson("/admin/user-permissions/upsert", { method: "POST", headers, body: JSON.stringify(payload) });
      notify("success", t("permissions.saved"));
      await load();
    } catch (error) {
      notify("error", error instanceof Error ? error.message : String(error));
    }
  };

  const remove = async (id: number) => {
    try {
      await apiVoid("/admin/user-permissions/delete", { method: "POST", headers, body: JSON.stringify({ id }) });
      notify("success", t("permissions.deleted"));
      await load();
    } catch (error) {
      notify("error", error instanceof Error ? error.message : String(error));
    }
  };

  return (
    <Card title={t("permissions.title")}>
      <div className="grid gap-4 xl:grid-cols-[360px_minmax(0,1fr)]">
        <div className="space-y-2">
          {rows.map((row) => (
            <div
              key={row.id}
              className={`card-shell cursor-pointer ${row.id === selectedId ? "nav-item-active" : ""}`}
              onClick={() => {
                setSelectedId(row.id);
                setForm({
                  id: String(row.id),
                  user_id: String(row.user_id),
                  provider_id: row.provider_id?.toString() ?? "",
                  model_pattern: row.model_pattern,
                });
              }}
            >
              <div className="font-semibold">{row.model_pattern}</div>
              <div className="text-xs text-muted">user #{row.user_id} · provider #{row.provider_id ?? "all"}</div>
            </div>
          ))}
        </div>
        <div className="card-shell space-y-3">
          <div className="flex justify-end">
            <Button variant="neutral" onClick={beginCreate}>{t("common.create")}</Button>
          </div>
          <div>
            <Label>{t("common.user")}</Label>
            <Select value={form.user_id} onChange={(value) => setForm((current) => ({ ...current, user_id: value }))} options={users.map((user) => ({ value: String(user.id), label: `${user.name} (#${user.id})` }))} />
          </div>
          <div>
            <Label>{t("common.provider")}</Label>
            <Select value={form.provider_id} onChange={(value) => setForm((current) => ({ ...current, provider_id: value }))} options={[{ value: "", label: t("common.allProviders") }, ...providers.map((provider) => ({ value: String(provider.id), label: provider.name }))]} />
          </div>
          <div>
            <Label>{t("common.modelPattern")}</Label>
            <Input value={form.model_pattern} onChange={(value) => setForm((current) => ({ ...current, model_pattern: value }))} />
          </div>
          <div className="flex gap-2">
            <Button onClick={() => void save()}>{t("common.save")}</Button>
            {selectedId ? <Button variant="danger" onClick={() => void remove(selectedId)}>{t("common.delete")}</Button> : null}
          </div>
        </div>
      </div>
    </Card>
  );
}
