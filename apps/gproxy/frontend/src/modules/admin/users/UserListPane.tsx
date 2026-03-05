import { useEffect, useMemo, useState } from "react";

import { useI18n } from "../../../app/i18n";
import type { UserQueryRow } from "../../../lib/types";
import { Button, Input, Label, Select } from "../../../components/ui";
import type { UserFormState } from "./types";

type UserListPaneProps = {
  rows: UserQueryRow[];
  selectedUserId: number | null;
  showUserEditor: boolean;
  form: UserFormState;
  onToggleEditor: () => void;
  onChangeForm: (patch: Partial<UserFormState>) => void;
  onSubmit: () => void;
  onSelectUser: (id: number) => void;
  onEditUser: (row: UserQueryRow) => void;
  onRemoveUser: (id: number) => void;
  onToggleUserEnabled: (row: UserQueryRow) => void;
};

function defaultPageSizeByViewport(): number {
  if (typeof window === "undefined") {
    return 20;
  }
  if (window.innerWidth < 640) {
    return 5;
  }
  if (window.innerWidth < 1024) {
    return 10;
  }
  if (window.innerWidth < 1600) {
    return 20;
  }
  return 50;
}

export function UserListPane({
  rows,
  selectedUserId,
  showUserEditor,
  form,
  onToggleEditor,
  onChangeForm,
  onSubmit,
  onSelectUser,
  onEditUser,
  onRemoveUser,
  onToggleUserEnabled
}: UserListPaneProps) {
  const { t } = useI18n();
  const [pageSize, setPageSize] = useState<number>(() => defaultPageSizeByViewport());
  const [page, setPage] = useState(1);

  useEffect(() => {
    setPage(1);
  }, [pageSize]);

  const totalPages = Math.max(1, Math.ceil(rows.length / pageSize));

  useEffect(() => {
    if (page > totalPages) {
      setPage(totalPages);
    }
  }, [page, totalPages]);

  const pagedRows = useMemo(() => {
    const start = (page - 1) * pageSize;
    return rows.slice(start, start + pageSize);
  }, [rows, page, pageSize]);

  return (
    <div className="space-y-3">
      <div className="flex items-center justify-between gap-2">
        <div className="text-sm font-semibold text-text">{t("users.section")}</div>
        <Button variant={showUserEditor ? "neutral" : "primary"} onClick={onToggleEditor}>
          {showUserEditor ? t("common.cancel") : t("users.addUser")}
        </Button>
      </div>

      {showUserEditor ? (
        <div className="provider-card space-y-3">
          <div className="grid gap-3">
            <div>
              <Label>{t("field.id")}</Label>
              <Input value={form.id} onChange={() => {}} disabled />
            </div>
            <div>
              <Label>{t("field.name")}</Label>
              <Input value={form.name} onChange={(value) => onChangeForm({ name: value })} />
            </div>
            <div>
              <Label>{t("field.password")}</Label>
              <Input
                type="password"
                value={form.password}
                onChange={(value) => onChangeForm({ password: value })}
              />
            </div>
            <div className="flex items-end gap-2 pb-1">
              <input
                id="user-enabled"
                type="checkbox"
                checked={form.enabled}
                onChange={(event) => onChangeForm({ enabled: event.target.checked })}
              />
              <label htmlFor="user-enabled" className="text-sm text-muted">
                {t("common.enabled")}
              </label>
            </div>
          </div>
          <Button onClick={onSubmit}>{t("common.save")}</Button>
        </div>
      ) : null}

      {rows.length === 0 ? (
        <div className="provider-card text-sm text-muted">{t("users.empty")}</div>
      ) : (
        <>
          {pagedRows.map((row) => {
            const active = row.id === selectedUserId;
            return (
              <div key={row.id} className={`provider-card ${active ? "provider-card-active" : ""}`}>
                <div className="flex items-start justify-between gap-2">
                  <button
                    type="button"
                    className="min-w-0 flex-1 text-left"
                    onClick={() => onSelectUser(row.id)}
                  >
                    <div className="truncate text-sm font-semibold text-text">{row.name}</div>
                    <div className="text-xs text-muted">{t("users.userMeta", { id: row.id })}</div>
                  </button>
                  <button
                    type="button"
                    className={`badge ${row.enabled ? "badge-active" : ""} cursor-pointer`}
                    onClick={() => onToggleUserEnabled(row)}
                  >
                    {row.enabled ? t("common.enabled") : t("common.disabled")}
                  </button>
                </div>
                <div className="mt-3 flex flex-wrap gap-2">
                  <Button variant="neutral" onClick={() => onEditUser(row)}>
                    {t("common.edit")}
                  </Button>
                  <Button variant="danger" onClick={() => onRemoveUser(row.id)}>
                    {t("common.delete")}
                  </Button>
                </div>
              </div>
            );
          })}
          <div className="flex flex-wrap items-center justify-between gap-2 text-xs text-muted">
            <div>
              {t("common.pager.stats", {
                shown: pagedRows.length,
                total: rows.length
              })}
            </div>
            <div className="flex items-center gap-2">
              <span>{t("common.show")}</span>
              <div className="w-20">
                <Select
                  value={String(pageSize)}
                  onChange={(value) => setPageSize(Number(value))}
                  options={[
                    { value: "5", label: "5" },
                    { value: "10", label: "10" },
                    { value: "20", label: "20" },
                    { value: "50", label: "50" }
                  ]}
                />
              </div>
              <Button
                variant="neutral"
                disabled={page <= 1}
                onClick={() => setPage((prev) => Math.max(1, prev - 1))}
              >
                {t("users.pager.prev")}
              </Button>
              <span>{t("common.pager.page", { current: page, total: totalPages })}</span>
              <Button
                variant="neutral"
                disabled={page >= totalPages}
                onClick={() => setPage((prev) => Math.min(totalPages, prev + 1))}
              >
                {t("users.pager.next")}
              </Button>
            </div>
          </div>
        </>
      )}
    </div>
  );
}
