import React, { useCallback, useEffect, useState } from "react";
import Panel from "../components/Panel";
import { apiErrorMessage, apiRequest } from "../lib/api";
import { formatTimestamp } from "../lib/format";
import type { User } from "../lib/types";

export default function UsersSection({
  adminKey,
  notify
}: {
  adminKey: string;
  notify: (toast: { type: "success" | "error" | "info" | "warning"; message: string }) => void;
}) {
  const [users, setUsers] = useState<User[]>([]);
  const [name, setName] = useState("");
  const [loading, setLoading] = useState(false);

  const loadUsers = useCallback(async () => {
    setLoading(true);
    try {
      const data = await apiRequest<User[]>("/admin/users", { adminKey });
      setUsers(Array.isArray(data) ? data : []);
    } catch (error) {
      notify({ type: "error", message: apiErrorMessage(error) });
    } finally {
      setLoading(false);
    }
  }, [adminKey, notify]);

  useEffect(() => {
    loadUsers();
  }, [loadUsers]);

  const handleCreate = async () => {
    try {
      const payload = {
        name: name.trim() ? name.trim() : null
      };
      await apiRequest("/admin/users", { method: "POST", body: payload, adminKey });
      setName("");
      notify({ type: "success", message: "User created." });
      await loadUsers();
    } catch (error) {
      notify({ type: "error", message: apiErrorMessage(error) });
    }
  };

  const handleDelete = async (user: User) => {
    if (!confirm(`Delete user ${user.id}?`)) {
      return;
    }
    try {
      await apiRequest(`/admin/users/${user.id}`, { method: "DELETE", adminKey });
      notify({ type: "success", message: "User deleted." });
      await loadUsers();
    } catch (error) {
      notify({ type: "error", message: apiErrorMessage(error) });
    }
  };

  return (
    <div className="space-y-6">
      <Panel title="Create user" subtitle="Users map to API keys.">
        <div className="grid gap-4 md:grid-cols-3">
          <div className="md:col-span-2">
            <label className="label">User name</label>
            <input
              className="input"
              value={name}
              onChange={(event) => setName(event.target.value)}
              placeholder="Optional label"
            />
          </div>
          <div className="flex items-end">
            <button className="btn btn-primary w-full" type="button" onClick={handleCreate}>
              Create user
            </button>
          </div>
        </div>
      </Panel>

      <Panel
        title="Users"
        subtitle="All registered users with API access."
        action={
          <button className="btn btn-ghost" type="button" onClick={loadUsers}>
            Refresh
          </button>
        }
      >
        {loading ? (
          <div className="text-sm text-slate-500">Loading users...</div>
        ) : users.length === 0 ? (
          <div className="text-sm text-slate-500">No users created.</div>
        ) : (
          <div className="space-y-3">
            {users.map((user) => (
              <div key={user.id} className="rounded-2xl border border-slate-200 bg-white/90 p-4">
                <div className="flex flex-wrap items-start justify-between gap-3">
                  <div>
                    <div className="text-sm font-semibold text-slate-800">
                      {user.name || "Unnamed"}
                    </div>
                    <div className="mt-1 text-xs text-slate-400">ID {user.id}</div>
                  </div>
                  <button className="btn btn-danger" type="button" onClick={() => handleDelete(user)}>
                    Delete
                  </button>
                </div>
                <div className="mt-3 grid gap-2 text-xs text-slate-500 md:grid-cols-2">
                  <div>Created</div>
                  <div className="text-right text-slate-700">
                    {formatTimestamp(user.created_at)}
                  </div>
                  <div>Updated</div>
                  <div className="text-right text-slate-700">
                    {formatTimestamp(user.updated_at)}
                  </div>
                </div>
              </div>
            ))}
          </div>
        )}
      </Panel>
    </div>
  );
}
