import { useI18n } from "../../../app/i18n";
import type { MemoryUserRow } from "../../../lib/types/admin";
import { BatchActionBar } from "../../../components/BatchActionBar";
import { Badge, Button, Input, Label } from "../../../components/ui";
import type { UserFormState } from "./types";

export type UserBatchProps = {
  batchMode: boolean;
  selectedCount: number;
  pending: boolean;
  isSelected: (id: number) => boolean;
  onEnter: () => void;
  onExit: () => void;
  onSelectAll: () => void;
  onClear: () => void;
  onDelete: () => void;
  onToggleRow: (id: number) => void;
};

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
  onToggleUserEnabled,
  onRemoveUser,
  batch,
}: {
  rows: MemoryUserRow[];
  selectedUserId: number | null;
  showUserEditor: boolean;
  form: UserFormState;
  onToggleEditor: () => void;
  onChangeForm: (patch: Partial<UserFormState>) => void;
  onSubmit: () => void;
  onSelectUser: (id: number) => void;
  onEditUser: (row: MemoryUserRow) => void;
  onToggleUserEnabled: (row: MemoryUserRow) => void;
  onRemoveUser: (id: number) => void;
  batch: UserBatchProps;
}) {
  const { t } = useI18n();
  return (
    <div className="panel-shell space-y-4">
      <div className="flex items-center justify-between gap-2">
        <div className="text-sm font-semibold text-text">{t("users.section")}</div>
        <div className="flex flex-wrap gap-2">
          <BatchActionBar
            batchMode={batch.batchMode}
            selectedCount={batch.selectedCount}
            pending={batch.pending}
            onEnter={batch.onEnter}
            onExit={batch.onExit}
            onSelectAll={batch.onSelectAll}
            onClear={batch.onClear}
            onDelete={batch.onDelete}
          />
          <Button variant={showUserEditor ? "neutral" : "primary"} onClick={onToggleEditor}>
            {showUserEditor ? t("common.cancel") : t("users.addUser")}
          </Button>
        </div>
      </div>
      {showUserEditor ? (
        <div className="panel-shell panel-shell-compact space-y-4">
          <div>
            <Label>{t("common.name")}</Label>
            <Input value={form.name} onChange={(value) => onChangeForm({ name: value })} />
          </div>
          <div>
            <Label>{t("common.password")}</Label>
            <Input
              type="password"
              value={form.password}
              onChange={(value) => onChangeForm({ password: value })}
            />
          </div>
          <label className="flex items-center gap-2 text-sm text-muted">
            <input
              type="checkbox"
              checked={form.enabled}
              onChange={(event) => onChangeForm({ enabled: event.target.checked })}
            />
            {t("common.enabled")}
          </label>
          <label className="flex items-center gap-2 text-sm text-muted">
            <input
              type="checkbox"
              checked={form.is_admin}
              onChange={(event) => onChangeForm({ is_admin: event.target.checked })}
            />
            {t("common.admin")}
          </label>
          <Button onClick={onSubmit}>{t("common.save")}</Button>
        </div>
      ) : null}
      {rows.length === 0 ? <p className="text-sm text-muted">{t("users.empty")}</p> : null}
      {rows.map((row) => (
        <div
          key={row.id}
          className={`record-item cursor-pointer ${row.id === selectedUserId ? "nav-item-active" : ""}`}
          onClick={() => {
            if (batch.batchMode) {
              batch.onToggleRow(row.id);
            } else {
              onSelectUser(row.id);
            }
          }}
          role="button"
          tabIndex={0}
          onKeyDown={(event) => {
            if (event.key === "Enter" || event.key === " ") {
              event.preventDefault();
              if (batch.batchMode) {
                batch.onToggleRow(row.id);
              } else {
                onSelectUser(row.id);
              }
            }
          }}
        >
          <div className="flex items-start justify-between gap-2">
            <div className="flex items-start gap-2">
              {batch.batchMode ? (
                <input
                  type="checkbox"
                  className="mt-1"
                  checked={batch.isSelected(row.id)}
                  onChange={() => batch.onToggleRow(row.id)}
                  onClick={(event) => event.stopPropagation()}
                />
              ) : null}
              <div>
                <div className="flex flex-wrap items-center gap-2">
                  <div className="font-semibold text-text">{row.name}</div>
                  <Badge variant="neutral">#{row.id}</Badge>
                </div>
                <div className="mt-2 flex flex-wrap items-center gap-2">
                  <button
                    type="button"
                    className="badge-button"
                    onClick={() => onToggleUserEnabled(row)}
                  >
                    <Badge variant={row.enabled ? "success" : "danger"}>
                      {row.enabled ? t("common.enabled") : t("common.disabled")}
                    </Badge>
                  </button>
                  <Badge variant={row.is_admin ? "accent" : "neutral"}>
                    {row.is_admin ? t("common.admin") : t("common.user")}
                  </Badge>
                </div>
              </div>
            </div>
            {batch.batchMode ? null : (
              <div className="flex gap-2">
                <Button variant="neutral" onClick={() => onEditUser(row)}>
                  {t("common.edit")}
                </Button>
                <Button variant="danger" onClick={() => onRemoveUser(row.id)}>
                  {t("common.delete")}
                </Button>
              </div>
            )}
          </div>
        </div>
      ))}
    </div>
  );
}
