import { useState } from "react";
import { useTranslation } from "react-i18next";
import { ConfirmDangerous } from "@/components/confirm-dangerous";
import { Button } from "@/components/ui/button";

interface WorkspaceBatchBarProps {
  count: number;
  pending: boolean;
  onEnable: () => void;
  onDisable: () => void;
  onDelete: () => void;
}

/** Compact batch controls sized for a workspace master pane. */
export function WorkspaceBatchBar({
  count,
  pending,
  onEnable,
  onDisable,
  onDelete,
}: WorkspaceBatchBarProps) {
  const { t } = useTranslation("common");
  const [confirmOpen, setConfirmOpen] = useState(false);
  const disabled = pending || count === 0;

  return (
    <>
      <p className="mb-2 text-xs text-muted-foreground">{t("batch.selected", { count })}</p>
      <div className="grid grid-cols-3 gap-1">
        <Button variant="outline" size="xs" disabled={disabled} onClick={onEnable}>
          {t("batch.enable")}
        </Button>
        <Button variant="outline" size="xs" disabled={disabled} onClick={onDisable}>
          {t("batch.disable")}
        </Button>
        <Button variant="destructive" size="xs" disabled={disabled} onClick={() => setConfirmOpen(true)}>
          {t("batch.delete")}
        </Button>
      </div>
      <ConfirmDangerous
        open={confirmOpen}
        onOpenChange={setConfirmOpen}
        title={t("batch.deleteTitle")}
        description={t("batch.deleteConfirm", { count })}
        confirmLabel={t("batch.delete")}
        onConfirm={() => {
          setConfirmOpen(false);
          onDelete();
        }}
        pending={pending}
      />
    </>
  );
}
