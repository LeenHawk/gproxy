import { useEffect, useMemo, useState } from "react";

import { useI18n } from "../../app/i18n";
import { apiRequest, formatError } from "../../lib/api";
import { parseRequiredI64 } from "../../lib/form";
import { scopeAll, scopeEq } from "../../lib/scope";
import type { UserKeyQueryRow, UserQueryRow } from "../../lib/types";
import { Button, Card } from "../../components/ui";
import { UserKeysPane } from "./users/UserKeysPane";
import { UserListPane } from "./users/UserListPane";
import type { UserFormState } from "./users/types";

type GeneratedUserKey = {
  id: number;
  user_id: number;
  api_key: string;
};

type UpsertEntityAck = {
  ok: boolean;
  id: number;
};

export function UsersModule({
  apiKey,
  notify
}: {
  apiKey: string;
  notify: (kind: "success" | "error" | "info", message: string) => void;
}) {
  const { t } = useI18n();
  const [rows, setRows] = useState<UserQueryRow[]>([]);
  const [selectedUserId, setSelectedUserId] = useState<number | null>(null);
  const [keyRows, setKeyRows] = useState<UserKeyQueryRow[]>([]);
  const [showUserEditor, setShowUserEditor] = useState(false);

  const [form, setForm] = useState<UserFormState>({
    id: "",
    name: "",
    password: "",
    enabled: true
  });

  const selectedUser = useMemo(
    () => rows.find((row) => row.id === selectedUserId) ?? null,
    [rows, selectedUserId]
  );

  const loadUsers = async () => {
    try {
      const data = await apiRequest<UserQueryRow[]>("/admin/users/query", {
        apiKey,
        method: "POST",
        body: {
          id: scopeAll<number>(),
          name: scopeAll<string>()
        }
      });
      const sorted = [...data].sort((a, b) => a.id - b.id);
      setRows(sorted);
      setSelectedUserId((prev) => {
        if (prev !== null && sorted.some((row) => row.id === prev)) {
          return prev;
        }
        return sorted[0]?.id ?? null;
      });
    } catch (error) {
      notify("error", formatError(error));
    }
  };

  const loadUserKeys = async (userId: number | null) => {
    if (userId === null) {
      setKeyRows([]);
      return;
    }
    try {
      const data = await apiRequest<UserKeyQueryRow[]>("/admin/user-keys/query", {
        apiKey,
        method: "POST",
        body: {
          id: scopeAll<number>(),
          user_id: scopeEq(userId),
          api_key: scopeAll<string>()
        }
      });
      setKeyRows([...data].sort((a, b) => a.id - b.id));
    } catch (error) {
      notify("error", formatError(error));
    }
  };

  useEffect(() => {
    void loadUsers();
  }, [apiKey]);

  useEffect(() => {
    void loadUserKeys(selectedUserId);
  }, [selectedUserId, apiKey]);

  const upsert = async () => {
    try {
      const id = form.id.trim() === "" ? null : parseRequiredI64(form.id, "id");
      const saved = await apiRequest<UpsertEntityAck>("/admin/users/upsert", {
        apiKey,
        method: "POST",
        body: {
          ...(id === null ? {} : { id }),
          name: form.name.trim(),
          password: form.password.trim(),
          enabled: form.enabled
        }
      });
      notify("success", t("users.saved"));
      setShowUserEditor(false);
      setSelectedUserId(saved.id);
      await loadUsers();
    } catch (error) {
      notify("error", formatError(error));
    }
  };

  const remove = async (id: number) => {
    try {
      await apiRequest("/admin/users/delete", {
        apiKey,
        method: "POST",
        body: { id }
      });
      notify("success", t("users.deleted", { id }));
      if (selectedUserId === id) {
        setSelectedUserId(null);
      }
      await loadUsers();
    } catch (error) {
      notify("error", formatError(error));
    }
  };

  const editUser = (row: UserQueryRow) => {
    setForm({
      id: String(row.id),
      name: row.name,
      password: row.password,
      enabled: row.enabled
    });
    setShowUserEditor(true);
    setSelectedUserId(row.id);
  };

  const toggleUserEnabled = async (row: UserQueryRow) => {
    const nextEnabled = !row.enabled;
    setRows((prev) => prev.map((item) => (item.id === row.id ? { ...item, enabled: nextEnabled } : item)));
    if (selectedUserId === row.id) {
      setForm((prev) => ({ ...prev, enabled: nextEnabled }));
    }
    try {
      await apiRequest("/admin/users/upsert", {
        apiKey,
        method: "POST",
        body: {
          id: row.id,
          name: row.name,
          password: row.password,
          enabled: nextEnabled
        }
      });
      notify("success", t("users.saved"));
      window.setTimeout(() => {
        void loadUsers();
      }, 200);
    } catch (error) {
      setRows((prev) => prev.map((item) => (item.id === row.id ? { ...item, enabled: row.enabled } : item)));
      if (selectedUserId === row.id) {
        setForm((prev) => ({ ...prev, enabled: row.enabled }));
      }
      notify("error", formatError(error));
    }
  };

  const generateUserKey = async () => {
    if (selectedUserId === null) {
      notify("error", t("users.needUser"));
      return;
    }
    try {
      const generated = await apiRequest<GeneratedUserKey>("/admin/user-keys/generate", {
        apiKey,
        method: "POST",
        body: {
          user_id: selectedUserId
        }
      });
      notify("success", t("users.keyGenerated", { key: generated.api_key }));
      await loadUserKeys(selectedUserId);
    } catch (error) {
      notify("error", formatError(error));
    }
  };

  const removeUserKey = async (id: number) => {
    if (selectedUserId === null) {
      return;
    }
    try {
      await apiRequest("/admin/user-keys/delete", {
        apiKey,
        method: "POST",
        body: { id }
      });
      notify("success", t("users.keyDeleted", { id }));
      await loadUserKeys(selectedUserId);
    } catch (error) {
      notify("error", formatError(error));
    }
  };

  return (
    <div className="space-y-4">
      <Card
        title={t("users.title")}
        subtitle={t("users.subtitle")}
        action={
          <Button variant="neutral" onClick={() => void loadUsers()}>
            {t("common.refresh")}
          </Button>
        }
      >
        <div className="grid gap-4 xl:grid-cols-[380px_minmax(0,1fr)]">
          <div className="space-y-4">
            <UserListPane
              rows={rows}
              selectedUserId={selectedUserId}
              showUserEditor={showUserEditor}
              form={form}
              onToggleEditor={() => {
                if (!showUserEditor) {
                  setForm({ id: "", name: "", password: "", enabled: true });
                }
                setShowUserEditor((prev) => !prev);
              }}
              onChangeForm={(patch) => setForm((prev) => ({ ...prev, ...patch }))}
              onSubmit={() => void upsert()}
              onSelectUser={setSelectedUserId}
              onEditUser={editUser}
              onRemoveUser={(id) => void remove(id)}
              onToggleUserEnabled={(row) => void toggleUserEnabled(row)}
            />
          </div>

          <div className="space-y-4">
            <UserKeysPane
              selectedUser={selectedUser}
              selectedUserId={selectedUserId}
              keyRows={keyRows}
              notify={notify}
              onGenerateKey={() => void generateUserKey()}
              onRefreshKeys={() => void loadUserKeys(selectedUserId)}
              onDeleteKey={(id) => void removeUserKey(id)}
            />
          </div>
        </div>
      </Card>
    </div>
  );
}
