import { useI18n } from "../../../app/i18n";
import { Badge, Button, Input, Label } from "../../../components/ui";
import type { MemoryUserKeyRow, MemoryUserQuotaRow, MemoryUserRow } from "../../../lib/types/admin";
import type { UserQuotaFormState } from "./quota";

export function UserKeysPane({
  selectedUser,
  selectedUserQuota,
  quotaForm,
  keyRows,
  onChangeQuotaForm,
  onSaveQuota,
  onRefreshQuota,
  onGenerateKey,
  onRefreshKeys,
  onToggleKeyEnabled,
  onDeleteKey,
}: {
  selectedUser: MemoryUserRow | null;
  selectedUserQuota: MemoryUserQuotaRow | null;
  quotaForm: UserQuotaFormState;
  keyRows: MemoryUserKeyRow[];
  onChangeQuotaForm: (patch: Partial<UserQuotaFormState>) => void;
  onSaveQuota: () => void;
  onRefreshQuota: () => void;
  onGenerateKey: () => void;
  onRefreshKeys: () => void;
  onToggleKeyEnabled: (row: MemoryUserKeyRow) => void;
  onDeleteKey: (id: number) => void;
}) {
  const { t } = useI18n();
  const quota = selectedUserQuota ?? {
    user_id: selectedUser?.id ?? 0,
    quota: 0,
    cost_used: 0,
    remaining: 0,
  };

  return (
    <div className="panel-shell">
      <div className="flex flex-wrap items-center justify-between gap-2">
        <div>
          <div className="text-sm font-semibold text-text">{t("users.selectedUserKeys")}</div>
          <div className="text-xs text-muted">
            {selectedUser
              ? t("users.selectedUserMeta", {
                  name: selectedUser.name,
                  id: selectedUser.id,
                })
              : t("users.selectUser")}
          </div>
        </div>
        <div className="flex gap-2">
          <Button disabled={!selectedUser} onClick={onGenerateKey}>
            {t("users.generateKey")}
          </Button>
          <Button variant="neutral" disabled={!selectedUser} onClick={onRefreshKeys}>
            {t("users.refreshKeys")}
          </Button>
        </div>
      </div>
      {selectedUser ? (
        <div className="panel-shell panel-shell-compact mt-4 space-y-4">
          <div className="text-sm font-semibold text-text">{t("common.quota")}</div>
          <div className="metric-grid">
            <div className="metric-card">
              <div className="metric-label">{t("common.quota")}</div>
              <div className="metric-value">{quota.quota}</div>
            </div>
            <div className="metric-card">
              <div className="metric-label">{t("common.costUsed")}</div>
              <div className="metric-value">{quota.cost_used}</div>
            </div>
            <div className="metric-card">
              <div className="metric-label">{t("common.remaining")}</div>
              <div className="metric-value">{quota.remaining}</div>
            </div>
          </div>
          <div className="grid gap-3 lg:grid-cols-2">
            <div>
              <Label>{t("common.quota")}</Label>
              <Input value={quotaForm.quota} onChange={(value) => onChangeQuotaForm({ quota: value })} />
            </div>
            <div>
              <Label>{t("common.costUsed")}</Label>
              <Input
                value={quotaForm.cost_used}
                onChange={(value) => onChangeQuotaForm({ cost_used: value })}
              />
            </div>
          </div>
          <div className="flex gap-2">
            <Button onClick={onSaveQuota}>{t("common.save")}</Button>
            <Button variant="neutral" onClick={onRefreshQuota}>
              {t("common.refresh")}
            </Button>
          </div>
        </div>
      ) : null}
      <div className="record-list mt-4">
        {keyRows.map((row) => (
          <div key={row.id} className="record-item">
            <div className="flex items-start justify-between gap-2">
              <div>
                <div className="flex flex-wrap items-center gap-2">
                  <div className="font-semibold text-text">#{row.id}</div>
                  <button
                    type="button"
                    className="badge-button"
                    onClick={() => onToggleKeyEnabled(row)}
                  >
                    <Badge variant={row.enabled ? "success" : "danger"}>
                      {row.enabled ? t("common.enabled") : t("common.disabled")}
                    </Badge>
                  </button>
                </div>
                <div className="mt-1 font-mono text-xs text-muted">{row.api_key}</div>
              </div>
              <Button variant="danger" onClick={() => onDeleteKey(row.id)}>
                {t("common.delete")}
              </Button>
            </div>
          </div>
        ))}
        {selectedUser && keyRows.length === 0 ? <p className="text-sm text-muted">{t("users.noKeys")}</p> : null}
      </div>
    </div>
  );
}
