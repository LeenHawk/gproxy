import { useI18n } from "../app/i18n";
import { Button } from "./ui";

export function BatchActionBar({
  batchMode,
  selectedCount,
  pending,
  onEnter,
  onExit,
  onSelectAll,
  onClear,
  onDelete,
}: {
  batchMode: boolean;
  selectedCount: number;
  pending: boolean;
  onEnter: () => void;
  onExit: () => void;
  onSelectAll: () => void;
  onClear: () => void;
  onDelete: () => void;
}) {
  const { t } = useI18n();

  if (!batchMode) {
    return (
      <Button variant="neutral" onClick={onEnter}>
        {t("batch.enter")}
      </Button>
    );
  }

  const deleteDisabled = pending || selectedCount === 0;

  return (
    <div className="flex flex-wrap items-center gap-2">
      <Button variant="neutral" onClick={onSelectAll} disabled={pending}>
        {t("batch.selectAll")}
      </Button>
      <Button variant="neutral" onClick={onClear} disabled={pending}>
        {t("batch.clear")}
      </Button>
      <Button variant="danger" onClick={onDelete} disabled={deleteDisabled}>
        {t("batch.delete", { count: selectedCount })}
      </Button>
      <Button variant="neutral" onClick={onExit} disabled={pending}>
        {t("batch.exit")}
      </Button>
    </div>
  );
}
