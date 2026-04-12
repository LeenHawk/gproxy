import { useEffect, useMemo, useState } from "react";

import { useI18n } from "../../app/i18n";
import { Card } from "../../components/ui";
import { useBatchSelection } from "../../components/useBatchSelection";
import { apiJson, apiVoid } from "../../lib/api";
import { authHeaders } from "../../lib/auth";
import type { GenerateUserKeyResponse, MemoryUserKeyRow, MemoryUserQuotaRow, MemoryUserRow } from "../../lib/types/admin";
import { UserKeysPane } from "./users/UserKeysPane";
import { UserListPane } from "./users/UserListPane";
import { buildUserWritePayload, type UserFormState } from "./users/types";
import { buildQuotaIncrementPayload } from "./users/quota";

export function UsersModule({
  sessionToken,
  notify,
}: {
  sessionToken: string;
  notify: (kind: "success" | "error" | "info", message: string) => void;
}) {
  const { t } = useI18n();
  const headers = useMemo(() => authHeaders(sessionToken), [sessionToken]);
  const [rows, setRows] = useState<MemoryUserRow[]>([]);
  const [selectedUserId, setSelectedUserId] = useState<number | null>(null);
  const [keyRows, setKeyRows] = useState<MemoryUserKeyRow[]>([]);
  const [selectedUserQuota, setSelectedUserQuota] = useState<MemoryUserQuotaRow | null>(null);
  const [showUserEditor, setShowUserEditor] = useState(false);
  const [form, setForm] = useState<UserFormState>({
    id: "",
    name: "",
    password: "",
    enabled: true,
    is_admin: false,
  });
  const [quotaIncrement, setQuotaIncrement] = useState("");

  const selectedUser = rows.find((row) => row.id === selectedUserId) ?? null;

  const loadUsers = async () => {
    const data = await apiJson<MemoryUserRow[]>("/admin/users/query", {
      method: "POST",
      headers,
      body: JSON.stringify({}),
    });
    const sorted = [...data].sort((a, b) => a.id - b.id);
    setRows(sorted);
    setSelectedUserId((prev) => prev ?? sorted[0]?.id ?? null);
  };

  const loadUserKeys = async (userId: number | null) => {
    if (userId === null) {
      setKeyRows([]);
      return;
    }
    const data = await apiJson<MemoryUserKeyRow[]>("/admin/user-keys/query", {
      method: "POST",
      headers,
      body: JSON.stringify({ user_id: { Eq: userId } }),
    });
    setKeyRows([...data].sort((a, b) => a.id - b.id));
  };

  const loadUserQuota = async (userId: number | null) => {
    if (userId === null) {
      setSelectedUserQuota(null);
      setQuotaIncrement("");
      return;
    }
    const data = await apiJson<MemoryUserQuotaRow[]>("/admin/user-quotas/query", {
      method: "POST",
      headers,
      body: JSON.stringify({ user_id: { Eq: userId }, limit: 1 }),
    });
    const row = data[0] ?? {
      user_id: userId,
      quota: 0,
      cost_used: 0,
      remaining: 0,
    };
    setSelectedUserQuota(row);
    setQuotaIncrement("");
  };

  useEffect(() => {
    void loadUsers().catch((error) => notify("error", error instanceof Error ? error.message : String(error)));
  }, []);

  useEffect(() => {
    void Promise.all([loadUserKeys(selectedUserId), loadUserQuota(selectedUserId)]).catch((error) =>
      notify("error", error instanceof Error ? error.message : String(error)),
    );
  }, [selectedUserId]);

  const saveUser = async () => {
    try {
      await apiJson("/admin/users/upsert", {
        method: "POST",
        headers,
        body: JSON.stringify(buildUserWritePayload(form)),
      });
      notify("success", t("users.saved"));
      setShowUserEditor(false);
      await loadUsers();
      setSelectedUserId(Number(form.id));
    } catch (error) {
      notify("error", error instanceof Error ? error.message : String(error));
    }
  };

  const deleteUser = async (id: number) => {
    try {
      await apiVoid("/admin/users/delete", {
        method: "POST",
        headers,
        body: JSON.stringify({ id }),
      });
      notify("success", t("users.deleted"));
      if (selectedUserId === id) {
        setSelectedUserId(null);
      }
      await loadUsers();
    } catch (error) {
      notify("error", error instanceof Error ? error.message : String(error));
    }
  };

  const usersBatch = useBatchSelection<MemoryUserRow, number>({
    rows,
    getKey: (row) => row.id,
    onBatchDelete: async (ids) => {
      await apiVoid("/admin/users/batch-delete", {
        method: "POST",
        headers,
        body: JSON.stringify(ids),
      });
    },
    onSuccess: (count) => {
      notify("success", t("batch.deleted", { count }));
      if (selectedUserId != null && usersBatch.selectedKeys.has(selectedUserId)) {
        setSelectedUserId(null);
      }
      void loadUsers();
    },
    onError: (err) => {
      notify("error", err instanceof Error ? err.message : String(err));
    },
    confirmMessage: (count) => t("batch.confirm", { count }),
  });

  const userKeysBatch = useBatchSelection<MemoryUserKeyRow, number>({
    rows: keyRows,
    getKey: (row) => row.id,
    onBatchDelete: async (ids) => {
      await apiVoid("/admin/user-keys/batch-delete", {
        method: "POST",
        headers,
        body: JSON.stringify(ids),
      });
    },
    onSuccess: (count) => {
      notify("success", t("batch.deleted", { count }));
      void loadUserKeys(selectedUserId);
    },
    onError: (err) => {
      notify("error", err instanceof Error ? err.message : String(err));
    },
    confirmMessage: (count) => t("batch.confirm", { count }),
  });

  const editUser = (row: MemoryUserRow) => {
    setForm({
      id: String(row.id),
      name: row.name,
      password: "",
      enabled: row.enabled,
      is_admin: row.is_admin,
    });
    setShowUserEditor(true);
    setSelectedUserId(row.id);
  };

  const toggleUserEnabled = async (row: MemoryUserRow) => {
    try {
      await apiJson("/admin/users/upsert", {
        method: "POST",
        headers,
        body: JSON.stringify({
          id: row.id,
          name: row.name,
          password: "",
          enabled: !row.enabled,
          is_admin: row.is_admin,
        }),
      });
      notify("success", t("users.saved"));
      await loadUsers();
      if (selectedUserId === row.id) {
        setForm((current) => ({ ...current, enabled: !row.enabled }));
      }
    } catch (error) {
      notify("error", error instanceof Error ? error.message : String(error));
    }
  };

  const generateKey = async () => {
    if (!selectedUserId) {
      return;
    }
    try {
      const generated = await apiJson<GenerateUserKeyResponse>("/admin/user-keys/generate", {
        method: "POST",
        headers,
        body: JSON.stringify({ user_id: selectedUserId }),
      });
      notify("success", `${t("users.keyGenerated")}: ${generated.api_key}`);
      await loadUserKeys(selectedUserId);
    } catch (error) {
      notify("error", error instanceof Error ? error.message : String(error));
    }
  };

  const deleteUserKey = async (id: number) => {
    try {
      await apiVoid("/admin/user-keys/delete", {
        method: "POST",
        headers,
        body: JSON.stringify({ id }),
      });
      notify("success", t("users.keyDeleted"));
      await loadUserKeys(selectedUserId);
    } catch (error) {
      notify("error", error instanceof Error ? error.message : String(error));
    }
  };

  const toggleUserKeyEnabled = async (row: MemoryUserKeyRow) => {
    try {
      await apiJson("/admin/user-keys/update-enabled", {
        method: "POST",
        headers,
        body: JSON.stringify({ id: row.id, enabled: !row.enabled }),
      });
      notify("success", t("users.saved"));
      await loadUserKeys(selectedUserId);
    } catch (error) {
      notify("error", error instanceof Error ? error.message : String(error));
    }
  };

  const addUserQuota = async (increment: string | number) => {
    if (!selectedUserQuota) {
      return;
    }
    try {
      await apiJson("/admin/user-quotas/upsert", {
        method: "POST",
        headers,
        body: JSON.stringify(buildQuotaIncrementPayload(selectedUserQuota, increment)),
      });
      notify("success", t("users.quotaSaved"));
      await loadUserQuota(selectedUserQuota.user_id);
    } catch (error) {
      notify("error", error instanceof Error ? error.message : String(error));
    }
  };

  return (
    <Card title={t("users.title")} subtitle={t("users.subtitle")}>
      <div className="grid gap-4 lg:grid-cols-[340px_minmax(0,1fr)]">
        <UserListPane
          rows={rows}
          selectedUserId={selectedUserId}
          showUserEditor={showUserEditor}
          form={form}
          onToggleEditor={() => {
            if (!showUserEditor) {
              const nextId = rows.reduce((max, row) => Math.max(max, row.id), 0) + 1;
              setForm({ id: String(nextId), name: "", password: "", enabled: true, is_admin: false });
            }
            setShowUserEditor((prev) => !prev);
          }}
          onChangeForm={(patch) => setForm((current) => ({ ...current, ...patch }))}
          onSubmit={() => void saveUser()}
          onSelectUser={setSelectedUserId}
          onEditUser={editUser}
          onToggleUserEnabled={(row) => void toggleUserEnabled(row)}
          onRemoveUser={(id) => void deleteUser(id)}
          batch={{
            batchMode: usersBatch.batchMode,
            selectedCount: usersBatch.selectedCount,
            pending: usersBatch.pending,
            isSelected: usersBatch.isSelected,
            onEnter: usersBatch.enterBatch,
            onExit: usersBatch.exitBatch,
            onSelectAll: usersBatch.selectAll,
            onClear: usersBatch.clear,
            onDelete: () => void usersBatch.deleteSelected(),
            onToggleRow: usersBatch.toggle,
          }}
        />
        <UserKeysPane
          selectedUser={selectedUser}
          selectedUserQuota={selectedUserQuota}
          quotaIncrement={quotaIncrement}
          keyRows={keyRows}
          onChangeQuotaIncrement={setQuotaIncrement}
          onAddQuickQuota={() => void addUserQuota(100)}
          onAddCustomQuota={() => void addUserQuota(quotaIncrement)}
          onGenerateKey={() => void generateKey()}
          onRefreshKeys={() => void loadUserKeys(selectedUserId)}
          onToggleKeyEnabled={(row) => void toggleUserKeyEnabled(row)}
          onDeleteKey={(id) => void deleteUserKey(id)}
          notify={notify}
          batch={{
            batchMode: userKeysBatch.batchMode,
            selectedCount: userKeysBatch.selectedCount,
            pending: userKeysBatch.pending,
            isSelected: userKeysBatch.isSelected,
            onEnter: userKeysBatch.enterBatch,
            onExit: userKeysBatch.exitBatch,
            onSelectAll: userKeysBatch.selectAll,
            onClear: userKeysBatch.clear,
            onDelete: () => void userKeysBatch.deleteSelected(),
            onToggleRow: userKeysBatch.toggle,
          }}
        />
      </div>
    </Card>
  );
}
