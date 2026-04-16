import { useState, type ReactNode } from "react";

import { useI18n } from "../../../app/i18n";
import { Button, Input, Select, TextArea } from "../../../components/ui";
import type { RewriteFilter, RewriteRule } from "./channel-constants";

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

/// Exported so the list view in RewriteRulesTab can render the same short
/// preview string next to each rule without duplicating the logic.
export function serializeActionValue(value: unknown): string {
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

type ValueType = "string" | "number" | "boolean" | "null" | "array" | "object";

function detectValueType(value: unknown): ValueType {
  if (value === null || value === undefined) return "null";
  if (typeof value === "string") return "string";
  if (typeof value === "number") return "number";
  if (typeof value === "boolean") return "boolean";
  if (Array.isArray(value)) return "array";
  return "object";
}

function defaultValueForType(type: ValueType): unknown {
  switch (type) {
    case "string":
      return "";
    case "number":
      return 0;
    case "boolean":
      return true;
    case "null":
      return null;
    case "array":
      return [];
    case "object":
      return {};
  }
}

/// Right-hand editing panel for a single rewrite rule. Internal UX state
/// (the model-pattern autocomplete focus flag) lives here; the parent owns
/// the draft/persisted distinction and hands us a `onUpdateEditing` patcher.
export function RewriteRuleEditor({
  editing,
  modelNames,
  onUpdateEditing,
  onSave,
  trailingActions,
}: {
  editing: RewriteRule;
  modelNames?: string[];
  onUpdateEditing: (patcher: (rule: RewriteRule) => RewriteRule) => void;
  onSave: () => void;
  /// Extra buttons shown alongside Save (e.g. Cancel for a draft, Delete for
  /// an existing rule). Parent composes whichever is appropriate.
  trailingActions?: ReactNode;
}) {
  const { t } = useI18n();
  const [patternFocused, setPatternFocused] = useState(false);

  const updatePath = (path: string) => onUpdateEditing((r) => ({ ...r, path }));

  const updateActionType = (type: "set" | "remove") =>
    onUpdateEditing((r) => ({
      ...r,
      action:
        type === "remove"
          ? { type: "remove" as const }
          : { type: "set" as const, value: null },
    }));

  const updateActionValue = (raw: string) =>
    onUpdateEditing((r) => ({
      ...r,
      action: { type: "set" as const, value: parseActionValue(raw) },
    }));

  const updateFilter = (filter: RewriteFilter | undefined) =>
    onUpdateEditing((r) => {
      const next = { ...r };
      if (filter) next.filter = filter;
      else delete next.filter;
      return next;
    });

  const toggleFilterChip = (dimension: "operations" | "protocols", val: string) => {
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

  return (
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
        <label className="text-xs text-muted">{t("providers.rewrite.action")}</label>
        <Select
          value={editing.action.type}
          onChange={(v) => updateActionType(v as "set" | "remove")}
          options={[
            { value: "set", label: t("providers.rewrite.action.set") },
            { value: "remove", label: t("providers.rewrite.action.remove") },
          ]}
        />
      </div>
      {editing.action.type === "set" ? (
        <>
          {(() => {
            const valueType = detectValueType(editing.action.value);
            return (
              <>
                <div>
                  <label className="text-xs text-muted">
                    {t("providers.rewrite.value_type")}
                  </label>
                  <Select
                    value={valueType}
                    onChange={(v) => {
                      const next = v as ValueType;
                      onUpdateEditing((r) => ({
                        ...r,
                        action: { type: "set" as const, value: defaultValueForType(next) },
                      }));
                    }}
                    options={[
                      { value: "string", label: t("providers.rewrite.value_type.string") },
                      { value: "number", label: t("providers.rewrite.value_type.number") },
                      { value: "boolean", label: t("providers.rewrite.value_type.boolean") },
                      { value: "null", label: t("providers.rewrite.value_type.null") },
                      { value: "array", label: t("providers.rewrite.value_type.array") },
                      { value: "object", label: t("providers.rewrite.value_type.object") },
                    ]}
                  />
                </div>
                <div>
                  <label className="text-xs text-muted">{t("providers.rewrite.value")}</label>
                  {valueType === "string" ? (
                    <Input
                      value={typeof editing.action.value === "string" ? editing.action.value : ""}
                      onChange={(v) =>
                        onUpdateEditing((r) => ({
                          ...r,
                          action: { type: "set" as const, value: v },
                        }))
                      }
                    />
                  ) : valueType === "number" ? (
                    <Input
                      value={
                        typeof editing.action.value === "number"
                          ? String(editing.action.value)
                          : ""
                      }
                      onChange={(raw) => {
                        const n = Number(raw);
                        onUpdateEditing((r) => ({
                          ...r,
                          action: {
                            type: "set" as const,
                            value: Number.isFinite(n) ? n : 0,
                          },
                        }));
                      }}
                    />
                  ) : valueType === "boolean" ? (
                    <Select
                      value={editing.action.value === true ? "true" : "false"}
                      onChange={(v) =>
                        onUpdateEditing((r) => ({
                          ...r,
                          action: { type: "set" as const, value: v === "true" },
                        }))
                      }
                      options={[
                        { value: "true", label: t("providers.rewrite.value_bool.true") },
                        { value: "false", label: t("providers.rewrite.value_bool.false") },
                      ]}
                    />
                  ) : valueType === "null" ? (
                    <p className="text-xs text-muted">
                      {t("providers.rewrite.value_null_hint")}
                    </p>
                  ) : (
                    <TextArea
                      value={serializeActionValue(editing.action.value)}
                      onChange={updateActionValue}
                      rows={4}
                      placeholder={t("providers.rewrite.value_json_placeholder")}
                    />
                  )}
                </div>
              </>
            );
          })()}
        </>
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
        <Button onClick={onSave}>{t("common.save")}</Button>
        {trailingActions}
      </div>
    </div>
  );
}
