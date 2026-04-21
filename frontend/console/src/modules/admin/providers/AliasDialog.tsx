import { useState } from "react";

import { Button, Input, Label } from "../../../components/ui";
import type { MemoryModelRow } from "../../../lib/types/admin";

export type AliasDialogLabels = {
  aliasDialogTitle: string;
  aliasDialogHint: string;
  aliasName: string;
  aliasPreview: string;
  aliasConfirm: string;
  cancel: string;
};

/// Modal dialog for creating a plain alias: a new model row whose name the user
/// picks freely, plus one rewrite rule that maps `body.model` back to the base
/// real model. No parameter-injection presets — that's what SuffixVariantDialog
/// is for.
export function AliasDialog({
  base,
  labels,
  onConfirm,
  onClose,
}: {
  base: MemoryModelRow;
  labels: AliasDialogLabels;
  onConfirm: (base: MemoryModelRow, aliasName: string) => void;
  onClose: () => void;
}) {
  const [aliasName, setAliasName] = useState<string>(`${base.model_id}-`);

  const trimmed = aliasName.trim();
  const valid = trimmed.length > 0 && trimmed !== base.model_id;

  const confirm = () => {
    if (!valid) return;
    onConfirm(base, trimmed);
  };

  return (
    <div
      className="fixed inset-0 z-50 flex items-center justify-center bg-black/50 p-4"
      onClick={onClose}
    >
      <div
        className="card-shell w-full max-w-lg max-h-[85vh] overflow-y-auto p-6 space-y-4"
        onClick={(e) => e.stopPropagation()}
      >
        <h3 className="text-lg font-semibold">{labels.aliasDialogTitle}</h3>
        <p className="text-xs text-muted">
          {labels.aliasDialogHint.replace("{model}", base.model_id)}
        </p>

        <div>
          <Label>{labels.aliasName}</Label>
          <Input value={aliasName} onChange={setAliasName} />
        </div>

        <div className="rounded border border-border bg-panel-muted p-3 text-xs">
          <div className="text-muted mb-1">{labels.aliasPreview}</div>
          <div className="font-mono">{trimmed || base.model_id}</div>
          <div className="mt-2 text-muted">
            <span className="text-text">model</span> = {JSON.stringify(base.model_id)}
          </div>
        </div>

        <div className="flex gap-2 justify-end">
          <Button variant="neutral" onClick={onClose}>
            {labels.cancel}
          </Button>
          <Button onClick={confirm} disabled={!valid}>
            {labels.aliasConfirm}
          </Button>
        </div>
      </div>
    </div>
  );
}
