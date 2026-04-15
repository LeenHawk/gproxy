import { useMemo, useState } from "react";

import { useI18n } from "../../../app/i18n";
import { BatchActionBar } from "../../../components/BatchActionBar";
import { Button, Card } from "../../../components/ui";
import { useBatchSelection } from "../../../components/useBatchSelection";
import { parseRewriteRules, type RewriteRule } from "./channel-constants";
import type { ProviderFormState } from "./index";
import { RewriteRuleEditor, serializeActionValue } from "./RewriteRuleEditor";

const EMPTY_RULE: RewriteRule = {
  path: "",
  action: { type: "Set", value: null },
};

export function RewriteRulesTab({
  form,
  onChange,
  onSave,
  modelNames,
  notify,
}: {
  form: ProviderFormState;
  onChange: (patch: Partial<ProviderFormState>) => void;
  onSave: () => void;
  /// Known model names (including aliases) for the current provider, used to
  /// populate the model_pattern autocomplete dropdown.
  modelNames?: string[];
  notify: (kind: "success" | "error" | "info", message: string) => void;
}) {
  const { t } = useI18n();
  // `selectedIdx = null` means no existing rule is selected.
  // `draft != null` means we're editing a new rule that hasn't been committed
  // to the list yet (like the Models / Credentials tabs, the new entry only
  // appears in the list after Save).
  const [selectedIdx, setSelectedIdx] = useState<number | null>(null);
  const [draft, setDraft] = useState<RewriteRule | null>(null);

  const rules = useMemo(
    () => parseRewriteRules(form.settings.rewrite_rules ?? "[]"),
    [form.settings.rewrite_rules],
  );

  const commit = (next: RewriteRule[]) => {
    onChange({
      settings: { ...form.settings, rewrite_rules: JSON.stringify(next) },
    });
  };

  const batch = useBatchSelection<RewriteRule, string>({
    rows: rules,
    getKey: (_row, idx) => String(idx),
    onBatchDelete: async (keys) => {
      const keySet = new Set(keys);
      const next = rules.filter((_, idx) => !keySet.has(String(idx)));
      commit(next);
      setSelectedIdx(null);
      setDraft(null);
    },
    onSuccess: (count) => {
      notify("success", t("batch.deleted", { count }));
    },
    onError: (err) => {
      notify("error", err instanceof Error ? err.message : String(err));
    },
    confirmMessage: (count) => t("batch.confirm", { count }),
  });

  const beginCreate = () => {
    setDraft({ ...EMPTY_RULE });
    setSelectedIdx(null);
  };

  const remove = (idx: number) => {
    commit(rules.filter((_, i) => i !== idx));
    if (selectedIdx === idx) setSelectedIdx(null);
    else if (selectedIdx != null && selectedIdx > idx) setSelectedIdx(selectedIdx - 1);
  };

  /// Current rule being edited (either draft or an existing one).
  const editing: RewriteRule | null =
    draft ?? (selectedIdx != null ? rules[selectedIdx] ?? null : null);
  const isDraft = draft != null;

  /// Patch the current rule. If editing a draft, mutate local draft state;
  /// otherwise patch the persisted rule in-place (auto-saves to form state).
  const updateEditing = (patcher: (rule: RewriteRule) => RewriteRule) => {
    if (isDraft && draft) {
      setDraft(patcher(draft));
      return;
    }
    if (selectedIdx == null) return;
    const next = [...rules];
    next[selectedIdx] = patcher(next[selectedIdx]);
    commit(next);
  };

  /// Save: if editing a draft, commit it to the list first, then save provider.
  /// If editing an existing rule, just save provider.
  const save = () => {
    if (isDraft && draft) {
      const next = [...rules, draft];
      commit(next);
      // After this render cycle, `rules` will include the new entry and we
      // want the list to highlight it. Use a timeout so the commit propagates
      // through the parent and our `rules` memo re-runs with the new array.
      const newIdx = next.length - 1;
      setDraft(null);
      setSelectedIdx(newIdx);
    }
    onSave();
  };

  const cancelDraft = () => {
    setDraft(null);
  };

  const trailingActions = isDraft ? (
    <Button variant="neutral" onClick={cancelDraft}>
      {t("common.cancel")}
    </Button>
  ) : selectedIdx != null ? (
    <Button variant="danger" onClick={() => remove(selectedIdx)}>
      {t("common.delete")}
    </Button>
  ) : null;

  return (
    <div className="grid gap-4 lg:grid-cols-[320px_minmax(0,1fr)]">
      <Card
        title={t("providers.rewrite.title")}
        action={
          <div className="flex flex-wrap gap-2">
            <BatchActionBar
              batchMode={batch.batchMode}
              selectedCount={batch.selectedCount}
              pending={batch.pending}
              onEnter={batch.enterBatch}
              onExit={batch.exitBatch}
              onSelectAll={batch.selectAll}
              onClear={batch.clear}
              onDelete={() => void batch.deleteSelected()}
            />
            <Button variant="neutral" onClick={beginCreate}>
              + {t("providers.rewrite.add")}
            </Button>
          </div>
        }
      >
        <div className="max-h-128 overflow-y-auto space-y-2 pr-1">
          {rules.length === 0 ? (
            <p className="text-sm text-muted">{t("providers.rewrite.empty")}</p>
          ) : null}
          {rules.map((rule, idx) => {
            const title = rule.path.trim() || t("providers.rewrite.empty_path");
            const subtitle =
              rule.action.type === "Remove"
                ? t("providers.rewrite.action.remove")
                : `${t("providers.rewrite.action.set")} · ${serializeActionValue(rule.action.value).slice(0, 40)}`;
            return (
              <button
                key={idx}
                type="button"
                className={`nav-item w-full ${
                  !isDraft && idx === selectedIdx ? "nav-item-active" : ""
                }`}
                onClick={() => {
                  if (batch.batchMode) {
                    batch.toggle(String(idx));
                    return;
                  }
                  setDraft(null);
                  setSelectedIdx(idx);
                }}
              >
                <div className="flex items-center gap-2">
                  {batch.batchMode ? (
                    <input
                      type="checkbox"
                      checked={batch.isSelected(String(idx))}
                      onChange={() => batch.toggle(String(idx))}
                      onClick={(event) => event.stopPropagation()}
                    />
                  ) : null}
                  <div className="font-semibold truncate">{title}</div>
                </div>
                <div className="text-xs text-muted truncate">{subtitle}</div>
              </button>
            );
          })}
        </div>
      </Card>
      <Card title={editing ? t("providers.rewrite.title") : t("common.noSelection")}>
        {editing ? (
          <RewriteRuleEditor
            editing={editing}
            modelNames={modelNames}
            onUpdateEditing={updateEditing}
            onSave={save}
            trailingActions={trailingActions}
          />
        ) : (
          <p className="text-sm text-muted">{t("providers.rewrite.selectPrompt")}</p>
        )}
      </Card>
    </div>
  );
}
