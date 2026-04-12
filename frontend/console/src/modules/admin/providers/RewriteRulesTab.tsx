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

export function RewriteRulesTab({
  form,
  onChange,
  onSave,
}: {
  form: ProviderFormState;
  onChange: (patch: Partial<ProviderFormState>) => void;
  onSave: () => void;
}) {
  const { t } = useI18n();
  const [selectedIdx, setSelectedIdx] = useState<number | null>(null);

  const rules = useMemo(
    () => parseRewriteRules(form.settings.rewrite_rules ?? "[]"),
    [form.settings.rewrite_rules],
  );

  const commit = (next: RewriteRule[]) => {
    onChange({
      settings: { ...form.settings, rewrite_rules: JSON.stringify(next) },
    });
  };

  const add = () => {
    const next = [...rules, { path: "", action: { type: "Set" as const, value: null } }];
    commit(next);
    setSelectedIdx(next.length - 1);
  };

  const remove = (idx: number) => {
    commit(rules.filter((_, i) => i !== idx));
    if (selectedIdx === idx) setSelectedIdx(null);
    else if (selectedIdx != null && selectedIdx > idx) setSelectedIdx(selectedIdx - 1);
  };

  const updateRule = (idx: number, patch: Partial<RewriteRule>) => {
    const next = [...rules];
    next[idx] = { ...next[idx], ...patch };
    commit(next);
  };

  const updateActionType = (idx: number, type: "Set" | "Remove") => {
    const rule = rules[idx];
    const action = type === "Remove" ? { type: "Remove" as const } : { type: "Set" as const, value: null };
    updateRule(idx, { ...rule, action });
  };

  const updateActionValue = (idx: number, raw: string) => {
    updateRule(idx, { action: { type: "Set" as const, value: parseActionValue(raw) } });
  };

  const updateFilter = (idx: number, filter: RewriteFilter | undefined) => {
    const next = [...rules];
    next[idx] = { ...next[idx] };
    if (filter) next[idx].filter = filter;
    else delete next[idx].filter;
    commit(next);
  };

  const toggleFilterChip = (
    idx: number,
    dimension: "operations" | "protocols",
    val: string,
  ) => {
    const current = rules[idx].filter ?? {};
    const arr = current[dimension] ?? [];
    const nextArr = arr.includes(val) ? arr.filter((v) => v !== val) : [...arr, val];
    const nextFilter: RewriteFilter = {
      ...current,
      [dimension]: nextArr.length > 0 ? nextArr : undefined,
    };
    if (!nextFilter.model_pattern && !nextFilter.operations && !nextFilter.protocols) {
      updateFilter(idx, undefined);
    } else {
      updateFilter(idx, nextFilter);
    }
  };

  const selected = selectedIdx != null ? rules[selectedIdx] : null;

  return (
    <div className="grid gap-4 xl:grid-cols-[360px_minmax(0,1fr)]">
      <Card
        title={t("providers.rewrite.title")}
        action={
          <Button variant="neutral" onClick={add}>
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
                className={`nav-item w-full ${idx === selectedIdx ? "nav-item-active" : ""}`}
                onClick={() => setSelectedIdx(idx)}
              >
                <div className="font-semibold truncate">{title}</div>
                <div className="text-xs text-muted truncate">{subtitle}</div>
              </button>
            );
          })}
        </div>
      </Card>
      <Card title={selected ? t("providers.rewrite.title") : t("common.noSelection")}>
        {selected && selectedIdx != null ? (
          <div className="space-y-4">
            <p className="text-xs text-muted">{t("providers.rewrite.hint")}</p>
            <div>
              <label className="text-xs text-muted">
                {t("providers.rewrite.path_placeholder")}
              </label>
              <Input
                value={selected.path}
                onChange={(v) => updateRule(selectedIdx, { path: v })}
                placeholder={t("providers.rewrite.path_placeholder")}
              />
            </div>
            <div>
              <label className="text-xs text-muted">Action</label>
              <Select
                value={selected.action.type}
                onChange={(v) => updateActionType(selectedIdx, v as "Set" | "Remove")}
                options={[
                  { value: "Set", label: "Set" },
                  { value: "Remove", label: "Remove" },
                ]}
              />
            </div>
            {selected.action.type === "Set" ? (
              <div>
                <label className="text-xs text-muted">
                  {t("providers.rewrite.value_placeholder")}
                </label>
                <TextArea
                  value={serializeActionValue(selected.action.value)}
                  onChange={(v) => updateActionValue(selectedIdx, v)}
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
                <Input
                  value={selected.filter?.model_pattern ?? ""}
                  onChange={(v) => {
                    const current = selected.filter ?? {};
                    const next: RewriteFilter = {
                      ...current,
                      model_pattern: v || undefined,
                    };
                    if (!next.model_pattern && !next.operations && !next.protocols) {
                      updateFilter(selectedIdx, undefined);
                    } else {
                      updateFilter(selectedIdx, next);
                    }
                  }}
                  placeholder="gpt-4*, claude-*"
                />
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
                        selected.filter?.operations?.includes(op) ? "btn-primary" : "btn-neutral"
                      }`}
                      onClick={() => toggleFilterChip(selectedIdx, "operations", op)}
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
                        selected.filter?.protocols?.includes(proto) ? "btn-primary" : "btn-neutral"
                      }`}
                      onClick={() => toggleFilterChip(selectedIdx, "protocols", proto)}
                    >
                      {proto}
                    </button>
                  ))}
                </div>
              </div>
            </div>

            <div className="flex gap-2">
              <Button onClick={onSave}>{t("common.save")}</Button>
              <Button variant="danger" onClick={() => remove(selectedIdx)}>
                {t("common.delete")}
              </Button>
            </div>
          </div>
        ) : (
          <p className="text-sm text-muted">{t("providers.rewrite.selectPrompt")}</p>
        )}
      </Card>
    </div>
  );
}
