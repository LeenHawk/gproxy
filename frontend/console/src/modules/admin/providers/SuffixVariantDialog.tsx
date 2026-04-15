import { useMemo, useState } from "react";

import { Button, Label, Select } from "../../../components/ui";
import type { MemoryModelRow } from "../../../lib/types/admin";
import {
  SUFFIX_GROUPS_BY_PROTOCOL,
  SUFFIX_PROTOCOL_LABELS,
  suffixProtocolForChannel,
  type SuffixActionSetBody,
  type SuffixProtocol,
} from "./suffix-presets";

export type SuffixVariantDialogLabels = {
  suffixDialogTitle: string;
  suffixDialogHint: string;
  suffixProtocol: string;
  suffixNone: string;
  suffixPreview: string;
  suffixConfirm: string;
  cancel: string;
};

/// Modal dialog that lets the user pick a protocol + group of suffix presets
/// and attach them to a base (real) model as an alias. All suffix-picker state
/// is local; the parent only deals in open/close and the confirmation payload.
export function SuffixVariantDialog({
  base,
  providerChannel,
  labels,
  onConfirm,
  onClose,
}: {
  base: MemoryModelRow;
  providerChannel?: string;
  labels: SuffixVariantDialogLabels;
  onConfirm: (base: MemoryModelRow, suffix: string, actions: SuffixActionSetBody[]) => void;
  onClose: () => void;
}) {
  const [suffixProtocol, setSuffixProtocol] = useState<SuffixProtocol>(() =>
    suffixProtocolForChannel(providerChannel),
  );
  /// Map of group key → selected suffix entry index (as string, "" = none).
  const [suffixSelections, setSuffixSelections] = useState<Record<string, string>>({});

  const suffixGroups = SUFFIX_GROUPS_BY_PROTOCOL[suffixProtocol];

  const { combinedSuffix, combinedActions } = useMemo(() => {
    let suffix = "";
    const actions: SuffixActionSetBody[] = [];
    for (const group of suffixGroups) {
      const picked = suffixSelections[group.key];
      if (!picked || picked === "") continue;
      const entry = group.entries[Number(picked)];
      if (!entry) continue;
      suffix += entry.suffix;
      actions.push(...entry.actions);
    }
    return { combinedSuffix: suffix, combinedActions: actions };
  }, [suffixGroups, suffixSelections]);

  const confirm = () => {
    if (!combinedSuffix) return;
    onConfirm(base, combinedSuffix, combinedActions);
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
        <h3 className="text-lg font-semibold">{labels.suffixDialogTitle}</h3>
        <p className="text-xs text-muted">
          {labels.suffixDialogHint.replace("{model}", base.model_id)}
        </p>

        <div>
          <Label>{labels.suffixProtocol}</Label>
          <Select
            value={suffixProtocol}
            onChange={(v) => {
              setSuffixProtocol(v as SuffixProtocol);
              setSuffixSelections({});
            }}
            options={(Object.keys(SUFFIX_PROTOCOL_LABELS) as SuffixProtocol[]).map((p) => ({
              value: p,
              label: SUFFIX_PROTOCOL_LABELS[p],
            }))}
          />
        </div>

        {suffixGroups.map((group) => (
          <div key={group.key}>
            <Label>{group.label}</Label>
            <Select
              value={suffixSelections[group.key] ?? ""}
              onChange={(v) => setSuffixSelections((prev) => ({ ...prev, [group.key]: v }))}
              options={[
                { value: "", label: labels.suffixNone },
                ...group.entries.map((e, i) => ({
                  value: String(i),
                  label: `${e.suffix} — ${e.label}`,
                })),
              ]}
            />
          </div>
        ))}

        <div className="rounded border border-border bg-panel-muted p-3 text-xs">
          <div className="text-muted mb-1">{labels.suffixPreview}</div>
          <div className="font-mono">
            {combinedSuffix ? `${base.model_id}${combinedSuffix}` : base.model_id}
          </div>
          {combinedActions.length > 0 ? (
            <div className="mt-2 space-y-1">
              {combinedActions.map((a, i) => (
                <div key={i} className="text-muted">
                  <span className="text-text">{a.path}</span> = {JSON.stringify(a.value)}
                </div>
              ))}
            </div>
          ) : null}
        </div>

        <div className="flex gap-2 justify-end">
          <Button variant="neutral" onClick={onClose}>
            {labels.cancel}
          </Button>
          <Button onClick={confirm} disabled={!combinedSuffix}>
            {labels.suffixConfirm}
          </Button>
        </div>
      </div>
    </div>
  );
}
