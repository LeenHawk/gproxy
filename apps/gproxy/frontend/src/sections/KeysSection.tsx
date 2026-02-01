import React, { useCallback, useEffect, useMemo, useState } from "react";
import Panel from "../components/Panel";
import { apiErrorMessage, apiRequest } from "../lib/api";
import { formatTimestamp, maskValue } from "../lib/format";
import type { ApiKey, User } from "../lib/types";

type FormState = {
  userId: string;
  keyValue: string;
  label: string;
  enabled: boolean;
};

const emptyForm: FormState = {
  userId: "",
  keyValue: "",
  label: "",
  enabled: true
};

export default function KeysSection({
  adminKey,
  notify
}: {
  adminKey: string;
  notify: (toast: { type: "success" | "error" | "info" | "warning"; message: string }) => void;
}) {
  const [keys, setKeys] = useState<ApiKey[]>([]);
  const [users, setUsers] = useState<User[]>([]);
  const [form, setForm] = useState<FormState>(emptyForm);
  const [loading, setLoading] = useState(false);

  const loadKeys = useCallback(async () => {
    setLoading(true);
    try {
      const data = await apiRequest<ApiKey[]>("/admin/keys", { adminKey });
      setKeys(Array.isArray(data) ? data : []);
    } catch (error) {
      notify({ type: "error", message: apiErrorMessage(error) });
    } finally {
      setLoading(false);
    }
  }, [adminKey, notify]);

  const loadUsers = useCallback(async () => {
    try {
      const data = await apiRequest<User[]>("/admin/users", { adminKey });
      const items = Array.isArray(data) ? data : [];
      setUsers(items);
      if (!form.userId && items.length > 0) {
        setForm((prev) => ({ ...prev, userId: String(items[0].id) }));
      }
    } catch (error) {
      notify({ type: "error", message: apiErrorMessage(error) });
    }
  }, [adminKey, form.userId, notify]);

  useEffect(() => {
    loadKeys();
    loadUsers();
  }, [loadKeys, loadUsers]);

  const userMap = useMemo(() => new Map(users.map((user) => [user.id, user])), [users]);

  const handleCreate = async () => {
    try {
      if (!form.userId) {
        throw new Error("User is required.");
      }
      if (!form.keyValue.trim()) {
        throw new Error("Key value is required.");
      }
      const payload = {
        user_id: Number(form.userId),
        key_value: form.keyValue.trim(),
        label: form.label.trim() ? form.label.trim() : null,
        enabled: form.enabled
      };
      await apiRequest("/admin/keys", { method: "POST", body: payload, adminKey });
      setForm((prev) => ({ ...prev, keyValue: "", label: "", enabled: true }));
      notify({ type: "success", message: "Key created." });
      await loadKeys();
    } catch (error) {
      notify({ type: "error", message: apiErrorMessage(error) });
    }
  };

  const handleDelete = async (key: ApiKey) => {
    if (!confirm(`Delete key ${key.id}?`)) {
      return;
    }
    try {
      await apiRequest(`/admin/keys/${key.id}`, { method: "DELETE", adminKey });
      notify({ type: "success", message: "Key deleted." });
      await loadKeys();
    } catch (error) {
      notify({ type: "error", message: apiErrorMessage(error) });
    }
  };

  const handleDisable = async (key: ApiKey) => {
    try {
      await apiRequest(`/admin/keys/${key.id}/disable`, { method: "PUT", adminKey });
      notify({ type: "success", message: "Key disabled." });
      await loadKeys();
    } catch (error) {
      notify({ type: "error", message: apiErrorMessage(error) });
    }
  };

  return (
    <div className="space-y-6">
      <Panel title="Create key" subtitle="Generate a new API key for a user.">
        <div className="grid gap-4 md:grid-cols-3">
          <div>
            <label className="label">User</label>
            <select
              className="select"
              value={form.userId}
              onChange={(event) => setForm((prev) => ({ ...prev, userId: event.target.value }))}
            >
              {users.map((user) => (
                <option key={user.id} value={user.id}>
                  {user.name || "Unnamed"} (#{user.id})
                </option>
              ))}
            </select>
          </div>
          <div>
            <label className="label">Label</label>
            <input
              className="input"
              value={form.label}
              onChange={(event) => setForm((prev) => ({ ...prev, label: event.target.value }))}
              placeholder="Optional label"
            />
          </div>
          <div>
            <label className="label">Key value</label>
            <input
              className="input"
              value={form.keyValue}
              onChange={(event) => setForm((prev) => ({ ...prev, keyValue: event.target.value }))}
              placeholder="sk-..."
            />
          </div>
        </div>
        <label className="flex items-center gap-2 text-sm text-slate-600">
          <input
            type="checkbox"
            checked={form.enabled}
            onChange={(event) => setForm((prev) => ({ ...prev, enabled: event.target.checked }))}
          />
          Enabled
        </label>
        <button className="btn btn-primary" type="button" onClick={handleCreate}>
          Create key
        </button>
      </Panel>

      <Panel
        title="API keys"
        subtitle="Keys mapped to users and access status."
        action={
          <button className="btn btn-ghost" type="button" onClick={loadKeys}>
            Refresh
          </button>
        }
      >
        {loading ? (
          <div className="text-sm text-slate-500">Loading keys...</div>
        ) : keys.length === 0 ? (
          <div className="text-sm text-slate-500">No keys created.</div>
        ) : (
          <div className="space-y-3">
            {keys.map((key) => {
              const user = userMap.get(key.user_id);
              return (
                <div key={key.id} className="rounded-2xl border border-slate-200 bg-white/90 p-4">
                  <div className="flex flex-wrap items-start justify-between gap-3">
                    <div>
                      <div className="text-sm font-semibold text-slate-800">
                        {user?.name || "User"} · #{key.user_id}
                      </div>
                      <div className="mt-1 text-xs text-slate-400">{maskValue(key.key_value)}</div>
                    </div>
                    <div className="flex flex-wrap items-center gap-2">
                      <span
                        className={`badge ${
                          key.enabled
                            ? "border-emerald-200 text-emerald-600"
                            : "border-slate-200 text-slate-500"
                        }`}
                      >
                        {key.enabled ? "Enabled" : "Disabled"}
                      </span>
                      {key.enabled ? (
                        <button className="btn btn-ghost" type="button" onClick={() => handleDisable(key)}>
                          Disable
                        </button>
                      ) : null}
                      <button className="btn btn-danger" type="button" onClick={() => handleDelete(key)}>
                        Delete
                      </button>
                    </div>
                  </div>
                  <div className="mt-3 grid gap-2 text-xs text-slate-500 md:grid-cols-2">
                    <div>Label</div>
                    <div className="text-right text-slate-700">{key.label || "-"}</div>
                    <div>Created</div>
                    <div className="text-right text-slate-700">{formatTimestamp(key.created_at)}</div>
                    <div>Last used</div>
                    <div className="text-right text-slate-700">
                      {formatTimestamp(key.last_used_at ?? undefined)}
                    </div>
                  </div>
                </div>
              );
            })}
          </div>
        )}
      </Panel>
    </div>
  );
}
