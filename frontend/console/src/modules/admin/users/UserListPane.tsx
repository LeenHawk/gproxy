import { useI18n } from "../../../app/i18n";
import type { MemoryUserRow } from "../../../lib/types/admin";
import { Badge, Button, Input, Label } from "../../../components/ui";
import type { UserFormState } from "./types";

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
}) {
  const { t } = useI18n();
  return (
    <div className="panel-shell space-y-4">
      <div className="flex items-center justify-between gap-2">
        <div className="text-sm font-semibold text-text">{t("users.section")}</div>
        <Button variant={showUserEditor ? "neutral" : "primary"} onClick={onToggleEditor}>
          {showUserEditor ? t("common.cancel") : t("users.addUser")}
        </Button>
      </div>
      {showUserEditor ? (
        <div className="panel-shell panel-shell-compact space-y-4">
          <div>
            <Label>{t("common.id")}</Label>
            <Input value={form.id} onChange={(value) => onChangeForm({ id: value })} />
          </div>
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
          onClick={() => onSelectUser(row.id)}
          role="button"
          tabIndex={0}
          onKeyDown={(event) => {
            if (event.key === "Enter" || event.key === " ") {
              event.preventDefault();
              onSelectUser(row.id);
            }
          }}
        >
          <div className="flex items-start justify-between gap-2">
            <div>
              <div className="flex flex-wrap items-center gap-2">
                <div className="font-semibold text-text">{row.name}</div>
                <Badge variant="neutral">#{row.id}</Badge>
                <Badge variant={row.enabled ? "success" : "danger"}>
                  {row.enabled ? t("common.enabled") : t("common.disabled")}
                </Badge>
                <Badge variant={row.is_admin ? "accent" : "neutral"}>
                  {row.is_admin ? t("common.admin") : t("common.user")}
                </Badge>
              </div>
            </div>
            <div className="flex gap-2">
              <Button
                variant={row.enabled ? "neutral" : "primary"}
                onClick={() => onToggleUserEnabled(row)}
              >
                {row.enabled ? t("common.disabled") : t("common.enabled")}
              </Button>
              <Button variant="neutral" onClick={() => onEditUser(row)}>
                {t("common.edit")}
              </Button>
              <Button variant="danger" onClick={() => onRemoveUser(row.id)}>
                {t("common.delete")}
              </Button>
            </div>
          </div>
        </div>
      ))}
    </div>
  );
}
