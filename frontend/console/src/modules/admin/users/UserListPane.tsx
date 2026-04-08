import type { MemoryUserRow } from "../../../lib/types/admin";
import { Button, Input, Label } from "../../../components/ui";
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
  onRemoveUser: (id: number) => void;
}) {
  return (
    <div className="space-y-3">
      <div className="flex items-center justify-between gap-2">
        <div className="text-sm font-semibold text-text">User List</div>
        <Button variant={showUserEditor ? "neutral" : "primary"} onClick={onToggleEditor}>
          {showUserEditor ? "Cancel" : "Add User"}
        </Button>
      </div>
      {showUserEditor ? (
        <div className="card-shell space-y-3">
          <div>
            <Label>ID</Label>
            <Input value={form.id} onChange={(value) => onChangeForm({ id: value })} />
          </div>
          <div>
            <Label>Name</Label>
            <Input value={form.name} onChange={(value) => onChangeForm({ name: value })} />
          </div>
          <div>
            <Label>Password</Label>
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
            Enabled
          </label>
          <label className="flex items-center gap-2 text-sm text-muted">
            <input
              type="checkbox"
              checked={form.is_admin}
              onChange={(event) => onChangeForm({ is_admin: event.target.checked })}
            />
            Admin
          </label>
          <Button onClick={onSubmit}>Save</Button>
        </div>
      ) : null}
      {rows.map((row) => (
        <div
          key={row.id}
          className={`card-shell cursor-pointer ${row.id === selectedUserId ? "nav-item-active" : ""}`}
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
              <div className="font-semibold">{row.name}</div>
              <div className="text-xs text-muted">
                #{row.id} · enabled={String(row.enabled)} · admin={String(row.is_admin)}
              </div>
            </div>
            <div className="flex gap-2">
              <Button variant="neutral" onClick={() => onEditUser(row)}>
                Edit
              </Button>
              <Button variant="danger" onClick={() => onRemoveUser(row.id)}>
                Delete
              </Button>
            </div>
          </div>
        </div>
      ))}
    </div>
  );
}
