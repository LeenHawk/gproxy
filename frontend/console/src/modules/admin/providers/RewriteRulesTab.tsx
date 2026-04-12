import { useMemo, useState } from "react";

import { useI18n } from "../../../app/i18n";
import { Button, Card, Input, Select, TextArea } from "../../../components/ui";
import {
  parseRewriteRules,
  type RewriteFilter,
  type RewriteRule,
} from "./channel-constants";
import type { ProviderFormState } from "./index";

const REWRITE_OPERATION_OPTIONS = [
  "generate_content",
  "stream_generate_content",
  "model_list",
  "model_get",
  "count_tokens",
  "compact",
  "create_image",
  "embeddings",
];

const REWRITE_PROTOCOL_OPTIONS = [
  "openai",
  "claude",
  "gemini",
  "openai_chat_completions",
  "gemini_ndjson",
  "openai_response",
];

function serializeActionValue(value: unknown): string {
  if (value === null || value === undefined) return "null";
  if (typeof value === "string") return JSON.stringify(value);
  return JSON.stringify(value, null, 2);
}

function parseActionValue(input: string): unknown {
  const trimmed = input.trim();
  if (trimmed === "") return null;
  try {
    return JSON.parse(trimmed);
  } catch {
    return trimmed;
  }
}

const EMPTY_RULE: RewriteRule = {
  path: "",
  action: { type: "Set", value: null },
};

export function RewriteRulesTab({
  form,
  onChange,
  onSave,
  modelNames,
}: {
  form: ProviderFormState;
  onChange: (patch: Partial<ProviderFormState>) => void;
  onSave: () => void;
  /// Known model names (including aliases) for the current provider, used to
  /// populate the model_pattern autocomplete dropdown.
  modelNames?: string[];
}) {
  const { t } = useI18n();
  // `selectedIdx = null` means no existing rule is selected.
  // `draft != null` means we're editing a new rule that hasn't been committed
  // to the list yet (like the Models / Credentials tabs, the new entry only
  // appears in the list after Save).
  const [selectedIdx, setSelectedIdx] = useState<number | null>(null);
  const [draft, setDraft] = useState<RewriteRule | null>(null);
  const [patternFocused, setPatternFocused] = useState(false);

  const rules = useMemo(
    () => parseRewriteRules(form.settings.rewrite_rules ?? "[]"),
    [form.settings.rewrite_rules],
  );

  const commit = (next: RewriteRule[]) => {
    onChange({
      settings: { ...form.settings, rewrite_rules: JSON.stringify(next) },
    });
  };

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
  const updateEditing = (patch: (rule: RewriteRule) => RewriteRule) => {
    if (isDraft && draft) {
      setDraft(patch(draft));
      return;
    }
    if (selectedIdx == null) return;
    const next = [...rules];
    next[selectedIdx] = patch(next[selectedIdx]);
    commit(next);
  };

  const updatePath = (path: string) => updateEditing((r) => ({ ...r, path }));

  const updateActionType = (type: "Set" | "Remove") =>
    updateEditing((r) => ({
      ...r,
      action:
        type === "Remove"
          ? { type: "Remove" as const }
          : { type: "Set" as const, value: null },
    }));

  const updateActionValue = (raw: string) =>
    updateEditing((r) => ({
      ...r,
      action: { type: "Set" as const, value: parseActionValue(raw) },
    }));

  const updateFilter = (filter: RewriteFilter | undefined) =>
    updateEditing((r) => {
      const next = { ...r };
      if (filter) next.filter = filter;
      else delete next.filter;
      return next;
    });

  const toggleFilterChip = (dimension: "operations" | "protocols", val: string) => {
    if (!editing) return;
    const current = editing.filter ?? {};
    const arr = current[dimension] ?? [];
    const nextArr = arr.includes(val) ? arr.filter((v) => v !== val) : [...arr, val];
    const nextFilter: RewriteFilter = {
      ...current,
      [dimension]: nextArr.length > 0 ? nextArr : undefined,
    };
    if (!nextFilter.model_pattern && !nextFilter.operations && !nextFilter.protocols) {
      updateFilter(undefined);
    } else {
      updateFilter(nextFilter);
    }
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

  return (
    <div className="grid gap-4 xl:grid-cols-[360px_minmax(0,1fr)]">
      <Card
        title={t("providers.rewrite.title")}
        action={
          <Button variant="neutral" onClick={beginCreate}>
            + {t("providers.rewrite.add")}
          </Button>
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
                ? "Remove"
                : `Set · ${serializeActionValue(rule.action.value).slice(0, 40)}`;
            return (
              <button
                key={idx}
                type="button"
                className={`nav-item w-full ${
                  !isDraft && idx === selectedIdx ? "nav-item-active" : ""
                }`}
                onClick={() => {
                  setDraft(null);
                  setSelectedIdx(idx);
                }}
              >
                <div className="font-semibold truncate">{title}</div>
                <div className="text-xs text-muted truncate">{subtitle}</div>
              </button>
            );
          })}
        </div>
      </Card>
      <Card title={editing ? t("providers.rewrite.title") : t("common.noSelection")}>
        {editing ? (
          <div className="space-y-4">
            <p className="text-xs text-muted">{t("providers.rewrite.hint")}</p>
            <div>
              <label className="text-xs text-muted">
                {t("providers.rewrite.path_placeholder")}
              </label>
              <Input
                value={editing.path}
                onChange={updatePath}
                placeholder={t("providers.rewrite.path_placeholder")}
              />
            </div>
            <div>
              <label className="text-xs text-muted">Action</label>
              <Select
                value={editing.action.type}
                onChange={(v) => updateActionType(v as "Set" | "Remove")}
                options={[
                  { value: "Set", label: "Set" },
                  { value: "Remove", label: "Remove" },
                ]}
              />
            </div>
            {editing.action.type === "Set" ? (
              <div>
                <label className="text-xs text-muted">
                  {t("providers.rewrite.value_placeholder")}
                </label>
                <TextArea
                  value={serializeActionValue(editing.action.value)}
                  onChange={updateActionValue}
                  rows={4}
                  placeholder={t("providers.rewrite.value_placeholder")}
                />
              </div>
            ) : null}

            {/* Filter */}
            <div className="space-y-2 rounded border border-border/50 bg-panel p-3">
              <div className="text-xs font-semibold">{t("providers.rewrite.filter")}</div>
              <div>
                <label className="text-[11px] text-muted">
                  {t("providers.rewrite.model_pattern")}
                </label>
                <div className="relative">
                  <Input
                    value={editing.filter?.model_pattern ?? ""}
                    onChange={(v) => {
                      const current = editing.filter ?? {};
                      const next: RewriteFilter = {
                        ...current,
                        model_pattern: v || undefined,
                      };
                      if (!next.model_pattern && !next.operations && !next.protocols) {
                        updateFilter(undefined);
                      } else {
                        updateFilter(next);
                      }
                    }}
                    onFocus={() => setPatternFocused(true)}
                    onBlur={() => {
                      setTimeout(() => setPatternFocused(false), 150);
                    }}
                    placeholder="gpt-4*, claude-*"
                  />
                  {patternFocused && modelNames && modelNames.length > 0
                    ? (() => {
                        const pattern = (editing.filter?.model_pattern ?? "").toLowerCase();
                        const matches = modelNames
                          .filter((name) =>
                            pattern === "" ? true : name.toLowerCase().includes(pattern),
                          )
                          .slice(0, 20);
                        if (matches.length === 0) return null;
                        return (
                          <div
                            className="absolute left-0 right-0 top-full z-50 mt-1 max-h-60 overflow-y-auto rounded border border-border shadow-lg"
                            style={{ background: "var(--bg-base)" }}
                          >
                            {matches.map((name) => (
                              <button
                                key={name}
                                type="button"
                                className="block w-full text-left px-2 py-1 text-xs hover:opacity-80"
                                style={{ background: "var(--bg-base)" }}
                                onMouseDown={(e) => {
                                  e.preventDefault();
                                }}
                                onClick={() => {
                                  const current = editing.filter ?? {};
                                  updateFilter({
                                    ...current,
                                    model_pattern: name,
                                  });
                                  setPatternFocused(false);
                                }}
                              >
                                {name}
                              </button>
                            ))}
                          </div>
                        );
                      })()
                    : null}
                </div>
              </div>
              <div>
                <label className="text-[11px] text-muted">
                  {t("providers.rewrite.operations")}
                </label>
                <div className="mt-1 flex flex-wrap gap-1">
                  {REWRITE_OPERATION_OPTIONS.map((op) => (
                    <button
                      key={op}
                      type="button"
                      className={`btn rounded-full px-2 py-0.5 text-[10px] font-semibold transition ${
                        editing.filter?.operations?.includes(op) ? "btn-primary" : "btn-neutral"
                      }`}
                      onClick={() => toggleFilterChip("operations", op)}
                    >
                      {op}
                    </button>
                  ))}
                </div>
              </div>
              <div>
                <label className="text-[11px] text-muted">
                  {t("providers.rewrite.protocols")}
                </label>
                <div className="mt-1 flex flex-wrap gap-1">
                  {REWRITE_PROTOCOL_OPTIONS.map((proto) => (
                    <button
                      key={proto}
                      type="button"
                      className={`btn rounded-full px-2 py-0.5 text-[10px] font-semibold transition ${
                        editing.filter?.protocols?.includes(proto) ? "btn-primary" : "btn-neutral"
                      }`}
                      onClick={() => toggleFilterChip("protocols", proto)}
                    >
                      {proto}
                    </button>
                  ))}
                </div>
              </div>
            </div>

            <div className="flex gap-2">
              <Button onClick={save}>{t("common.save")}</Button>
              {isDraft ? (
                <Button variant="neutral" onClick={cancelDraft}>
                  {t("common.cancel")}
                </Button>
              ) : selectedIdx != null ? (
                <Button variant="danger" onClick={() => remove(selectedIdx)}>
                  {t("common.delete")}
                </Button>
              ) : null}
            </div>
          </div>
        ) : (
          <p className="text-sm text-muted">{t("providers.rewrite.selectPrompt")}</p>
        )}
      </Card>
    </div>
  );
}
