import { useState } from "react";

import { BatchActionBar } from "../../../components/BatchActionBar";
import { Button, Card, Input, Label } from "../../../components/ui";
import { lookupDefaultPricing } from "../../../lib/default-model-pricing";
import type { MemoryModelRow } from "../../../lib/types/admin";
import { AliasDialog } from "./AliasDialog";
import { PricingEditor, type PricingEditorLabels } from "./PricingEditor";
import { usePullModelsPanel } from "./PullModelsPanel";
import { SuffixVariantDialog } from "./SuffixVariantDialog";
import type { SuffixActionSetBody } from "./suffix-presets";

export type ModelFormState = {
  id: string;
  model_id: string;
  display_name: string;
  enabled: boolean;
  pricing_json: string;
};

export type ModelsBatchProps = {
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

type ModelsTabLabels = {
  title: string;
  empty: string;
  create: string;
  save: string;
  delete: string;
  cancel: string;
  modelId: string;
  displayName: string;
  enabled: string;
  pricingJsonHint: string;
  applyDefaultPricing: string;
  pull: string;
  pullLoading: string;
  pullEmpty: string;
  pullFound: string;
  pullImport: string;
  pullSelectAll: string;
  pullDeselectAll: string;
  addSuffixVariant: string;
  suffixDialogTitle: string;
  suffixDialogHint: string;
  suffixProtocol: string;
  suffixNone: string;
  suffixPreview: string;
  suffixConfirm: string;
  addAlias: string;
  aliasDialogTitle: string;
  aliasDialogHint: string;
  aliasName: string;
  aliasPreview: string;
  aliasConfirm: string;
  pricingEditor: PricingEditorLabels;
};

export function ModelsTab({
  rows,
  selectedId,
  form,
  onSelect,
  onCreate,
  onChangeForm,
  onSave,
  onDelete,
  onPull,
  onImport,
  onAddSuffixVariant,
  onAddAlias,
  providerChannel,
  labels,
  batch,
}: {
  rows: MemoryModelRow[];
  selectedId: number | null;
  form: ModelFormState;
  onSelect: (row: MemoryModelRow) => void;
  onCreate: () => void;
  onChangeForm: (patch: Partial<ModelFormState>) => void;
  onSave: () => void;
  onDelete: (id: number) => void;
  onPull?: () => Promise<string[]>;
  onImport?: (models: string[]) => void;
  /// Called when the user confirms a suffix variant dialog. Receives the base
  /// real model, the combined suffix string, and the rewrite rule actions to
  /// attach (all with model_pattern = base.model_id + suffix).
  onAddSuffixVariant?: (
    base: MemoryModelRow,
    suffix: string,
    actions: SuffixActionSetBody[],
  ) => void;
  /// Called when the user confirms the plain-alias dialog. Receives the base
  /// real model and the user-picked alias name (free-form, not derived from a
  /// preset suffix).
  onAddAlias?: (base: MemoryModelRow, aliasName: string) => void;
  /// Current provider's channel — used to pick a default suffix protocol.
  providerChannel?: string;
  labels: ModelsTabLabels;
  batch: ModelsBatchProps;
}) {
  const selected = rows.find((row) => row.id === selectedId) ?? null;

  const pullModels = usePullModelsPanel({
    rows,
    onPull,
    onImport,
    labels: {
      pull: labels.pull,
      pullLoading: labels.pullLoading,
      pullEmpty: labels.pullEmpty,
      pullFound: labels.pullFound,
      pullImport: labels.pullImport,
      pullSelectAll: labels.pullSelectAll,
      pullDeselectAll: labels.pullDeselectAll,
      cancel: labels.cancel,
    },
  });

  // Suffix variant dialog: `null` means closed, otherwise holds the base model
  // being varied. All picker state lives inside the dialog component itself.
  const [suffixDialogBase, setSuffixDialogBase] = useState<MemoryModelRow | null>(null);
  const [aliasDialogBase, setAliasDialogBase] = useState<MemoryModelRow | null>(null);

  return (
    <div className="grid gap-4 lg:grid-cols-[320px_minmax(0,1fr)]">
      <Card title={labels.title} action={pullModels.trigger}>
        <div className="space-y-3">
          <div className="flex flex-wrap items-center gap-2">
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
          </div>
          <div className="max-h-128 overflow-y-auto space-y-2 pr-1">
            {rows.length === 0 ? (
              <p className="text-sm text-muted">{labels.empty}</p>
            ) : null}
            {rows.map((row) => {
              return (
                <button
                  key={row.id}
                  type="button"
                  className={`nav-item w-full ${row.id === selectedId ? "nav-item-active" : ""}`}
                  onClick={() => {
                    if (batch.batchMode) {
                      batch.onToggleRow(row.id);
                    } else {
                      onSelect(row);
                    }
                  }}
                >
                  <div className="flex items-center gap-2">
                    {batch.batchMode ? (
                      <input
                        type="checkbox"
                        checked={batch.isSelected(row.id)}
                        onChange={() => batch.onToggleRow(row.id)}
                        onClick={(event) => event.stopPropagation()}
                      />
                    ) : null}
                    <div className="font-semibold">{row.display_name?.trim() || row.model_id}</div>
                  </div>
                  <div className="text-xs text-muted">{row.display_name?.trim() ? row.model_id : "—"}</div>
                </button>
              );
            })}
          </div>
        </div>
      </Card>
      <Card
        title={selected ? labels.title : labels.create}
        action={
          <Button variant="neutral" onClick={onCreate}>
            {labels.create}
          </Button>
        }
      >
        {pullModels.isActive ? (
          pullModels.panel
        ) : (
          <div className="space-y-4">
            <div>
              <Label>{labels.modelId}</Label>
              <Input value={form.model_id} onChange={(value) => onChangeForm({ model_id: value })} />
            </div>
            <div>
              <Label>{labels.displayName}</Label>
              <Input
                value={form.display_name}
                onChange={(value) => onChangeForm({ display_name: value })}
              />
            </div>
            <label className="flex items-center gap-2 text-sm text-muted">
              <input
                type="checkbox"
                checked={form.enabled}
                onChange={(event) => onChangeForm({ enabled: event.target.checked })}
              />
              {labels.enabled}
            </label>
            <div>
              <PricingEditor
                value={form.pricing_json}
                onChange={(value) => onChangeForm({ pricing_json: value })}
                labels={labels.pricingEditor}
              />
              <p className="mt-1 text-xs text-muted">{labels.pricingJsonHint}</p>
              <Button
                variant="neutral"
                onClick={() => {
                  const result = lookupDefaultPricing(form.model_id);
                  if (result) onChangeForm({ pricing_json: result });
                }}
                disabled={!lookupDefaultPricing(form.model_id)}
              >
                {labels.applyDefaultPricing}
              </Button>
            </div>
            <div className="flex gap-2">
              <Button onClick={onSave}>{labels.save}</Button>
              {selected && onAddAlias ? (
                <Button variant="neutral" onClick={() => setAliasDialogBase(selected)}>
                  + {labels.addAlias}
                </Button>
              ) : null}
              {selected && onAddSuffixVariant ? (
                <Button variant="neutral" onClick={() => setSuffixDialogBase(selected)}>
                  + {labels.addSuffixVariant}
                </Button>
              ) : null}
              {selected ? (
                <Button variant="danger" onClick={() => onDelete(selected.id)}>
                  {labels.delete}
                </Button>
              ) : null}
            </div>
          </div>
        )}
      </Card>

      {suffixDialogBase && onAddSuffixVariant ? (
        <SuffixVariantDialog
          base={suffixDialogBase}
          providerChannel={providerChannel}
          labels={{
            suffixDialogTitle: labels.suffixDialogTitle,
            suffixDialogHint: labels.suffixDialogHint,
            suffixProtocol: labels.suffixProtocol,
            suffixNone: labels.suffixNone,
            suffixPreview: labels.suffixPreview,
            suffixConfirm: labels.suffixConfirm,
            cancel: labels.cancel,
          }}
          onConfirm={(base, suffix, actions) => {
            onAddSuffixVariant(base, suffix, actions);
            setSuffixDialogBase(null);
          }}
          onClose={() => setSuffixDialogBase(null)}
        />
      ) : null}

      {aliasDialogBase && onAddAlias ? (
        <AliasDialog
          base={aliasDialogBase}
          labels={{
            aliasDialogTitle: labels.aliasDialogTitle,
            aliasDialogHint: labels.aliasDialogHint,
            aliasName: labels.aliasName,
            aliasPreview: labels.aliasPreview,
            aliasConfirm: labels.aliasConfirm,
            cancel: labels.cancel,
          }}
          onConfirm={(base, aliasName) => {
            onAddAlias(base, aliasName);
            setAliasDialogBase(null);
          }}
          onClose={() => setAliasDialogBase(null)}
        />
      ) : null}
    </div>
  );
}
