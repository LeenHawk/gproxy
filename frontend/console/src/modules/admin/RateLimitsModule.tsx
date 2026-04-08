import { useEffect, useMemo, useState } from "react";

import { useI18n } from "../../app/i18n";
import { Button, Card, Input, Label, Select } from "../../components/ui";
import { apiJson, apiVoid } from "../../lib/api";
import { authHeaders } from "../../lib/auth";
import { parseRequiredI64 } from "../../lib/form";
import type { MemoryRateLimitRow, MemoryUserRow, UserRateLimitWrite } from "../../lib/types/admin";

export function RateLimitsModule({
  sessionToken,
  notify,
}: {
  sessionToken: string;
  notify: (kind: "success" | "error" | "info", message: string) => void;
}) {
  const { t } = useI18n();
  const headers = useMemo(() => authHeaders(sessionToken), [sessionToken]);
  const [users, setUsers] = useState<MemoryUserRow[]>([]);
  const [rows, setRows] = useState<MemoryRateLimitRow[]>([]);
  const [selectedKey, setSelectedKey] = useState<string | null>(null);
  const [form, setForm] = useState({
    id: "",
    user_id: "",
    model_pattern: "",
    rpm: "",
    rpd: "",
    total_tokens: "",
  });
  const nextId = useMemo(
    () => rows.reduce((max, row) => Math.max(max, row.id), 0) + 1,
    [rows],
  );

  const beginCreate = () => {
    setSelectedKey(null);
    setForm({
      id: String(nextId),
      user_id: users[0] ? String(users[0].id) : "",
      model_pattern: "",
      rpm: "",
      rpd: "",
      total_tokens: "",
    });
  };

  const load = async () => {
    const [userRows, limitRows] = await Promise.all([
      apiJson<MemoryUserRow[]>("/admin/users/query", { method: "POST", headers, body: JSON.stringify({}) }),
      apiJson<MemoryRateLimitRow[]>("/admin/user-rate-limits/query", { method: "POST", headers, body: JSON.stringify({}) }),
    ]);
    setUsers(userRows);
    setRows(limitRows);
  };

  useEffect(() => {
    void load().catch((error) => notify("error", error instanceof Error ? error.message : String(error)));
  }, []);

  useEffect(() => {
    if (!selectedKey && !form.id && users.length > 0) {
      beginCreate();
    }
  }, [form.id, selectedKey, users]);

  const save = async () => {
    try {
      const payload: UserRateLimitWrite = {
        id: parseRequiredI64(form.id, "id"),
        user_id: parseRequiredI64(form.user_id, "user_id"),
        model_pattern: form.model_pattern.trim(),
        rpm: form.rpm.trim() ? Number(form.rpm) : null,
        rpd: form.rpd.trim() ? Number(form.rpd) : null,
        total_tokens: form.total_tokens.trim() ? Number(form.total_tokens) : null,
      };
      await apiJson("/admin/user-rate-limits/upsert", { method: "POST", headers, body: JSON.stringify(payload) });
      notify("success", t("rateLimits.saved"));
      await load();
    } catch (error) {
      notify("error", error instanceof Error ? error.message : String(error));
    }
  };

  const remove = async (userId: string, modelPattern: string) => {
    try {
      await apiVoid("/admin/user-rate-limits/delete", {
        method: "POST",
        headers,
        body: JSON.stringify({
          user_id: parseRequiredI64(userId, "user_id"),
          model_pattern: modelPattern,
        }),
      });
      notify("success", t("rateLimits.deleted"));
      await load();
    } catch (error) {
      notify("error", error instanceof Error ? error.message : String(error));
    }
  };

  return (
    <Card title={t("rateLimits.title")}>
      <div className="grid gap-4 xl:grid-cols-[360px_minmax(0,1fr)]">
        <div className="space-y-2">
          {rows.map((row) => {
            const key = `${row.user_id}:${row.model_pattern}`;
            return (
              <div
                key={key}
                className={`card-shell cursor-pointer ${key === selectedKey ? "nav-item-active" : ""}`}
                onClick={() => {
                  setSelectedKey(key);
                  setForm({
                    id: String(row.id),
                    user_id: String(row.user_id),
                    model_pattern: row.model_pattern,
                    rpm: row.rpm?.toString() ?? "",
                    rpd: row.rpd?.toString() ?? "",
                    total_tokens: row.total_tokens?.toString() ?? "",
                  });
                }}
              >
                <div className="font-semibold">{row.model_pattern}</div>
                <div className="text-xs text-muted">user #{row.user_id}</div>
              </div>
            );
          })}
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
            <Label>{t("common.modelPattern")}</Label>
            <Input value={form.model_pattern} onChange={(value) => setForm((current) => ({ ...current, model_pattern: value }))} />
          </div>
          <div className="grid gap-3 lg:grid-cols-3">
            <div>
              <Label>rpm</Label>
              <Input value={form.rpm} onChange={(value) => setForm((current) => ({ ...current, rpm: value }))} />
            </div>
            <div>
              <Label>rpd</Label>
              <Input value={form.rpd} onChange={(value) => setForm((current) => ({ ...current, rpd: value }))} />
            </div>
            <div>
              <Label>total_tokens</Label>
              <Input value={form.total_tokens} onChange={(value) => setForm((current) => ({ ...current, total_tokens: value }))} />
            </div>
          </div>
          <div className="flex gap-2">
            <Button onClick={() => void save()}>{t("common.save")}</Button>
            {selectedKey ? (
              <Button variant="danger" onClick={() => void remove(form.user_id, form.model_pattern)}>
                {t("common.delete")}
              </Button>
            ) : null}
          </div>
        </div>
      </div>
    </Card>
  );
}
