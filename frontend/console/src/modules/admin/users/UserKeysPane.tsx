import { useI18n } from "../../../app/i18n";
import { Badge, Button } from "../../../components/ui";
import type { MemoryUserKeyRow, MemoryUserRow } from "../../../lib/types/admin";

export function UserKeysPane({
  selectedUser,
  keyRows,
  onGenerateKey,
  onRefreshKeys,
  onDeleteKey,
}: {
  selectedUser: MemoryUserRow | null;
  keyRows: MemoryUserKeyRow[];
  onGenerateKey: () => void;
  onRefreshKeys: () => void;
  onDeleteKey: (id: number) => void;
}) {
  const { t } = useI18n();
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
      <div className="record-list mt-4">
        {keyRows.map((row) => (
          <div key={row.id} className="record-item">
            <div className="flex items-start justify-between gap-2">
              <div>
                <div className="flex flex-wrap items-center gap-2">
                  <div className="font-semibold text-text">#{row.id}</div>
                  <Badge variant={row.enabled ? "success" : "danger"}>
                    {row.enabled ? t("common.enabled") : t("common.disabled")}
                  </Badge>
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
