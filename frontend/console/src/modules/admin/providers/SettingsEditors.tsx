import { useState } from "react";

import { Button, Input } from "../../../components/ui";
import {
  ANTHROPIC_REFERENCE_BETA_HEADERS,
  CLAUDECODE_OAUTH_BETA,
  CLAUDE_AGENT_SDK_PRELUDE,
  CLAUDE_CODE_PRELUDE,
  RECOMMENDED_CACHE_TEMPLATE,
  SANITIZE_TEMPLATES,
  parseBetaHeaders,
  parseCacheBreakpoints,
  parseSanitizeRules,
  type CacheBreakpointRule,
  type SanitizeRule,
} from "./channel-constants";

type TranslateFn = (key: string, params?: Record<string, string | number>) => string;

/// Collapsible section header — matches the dispatch table pattern.
function CollapsibleSection({
  title,
  summary,
  expanded,
  onToggle,
  expandLabel,
  collapseLabel,
  actions,
  children,
}: {
  title: string;
  summary: string;
  expanded: boolean;
  onToggle: () => void;
  expandLabel: string;
  collapseLabel: string;
  actions?: React.ReactNode;
  children: React.ReactNode;
}) {
  return (
    <div className="panel-shell space-y-4">
      <div className="flex flex-wrap items-start justify-between gap-3">
        <div>
          <div className="text-sm font-semibold text-text">{title}</div>
          {!expanded ? <p className="mt-1 text-sm text-muted">{summary}</p> : null}
        </div>
        <div className="flex flex-wrap gap-2">
          <Button variant="neutral" onClick={onToggle}>
            {expanded ? collapseLabel : expandLabel}
          </Button>
          {expanded ? actions : null}
        </div>
      </div>
      {expanded ? children : null}
    </div>
  );
}

// ---------------------------------------------------------------------------
// Cache Breakpoints Editor
// ---------------------------------------------------------------------------

export function CacheBreakpointsEditor({
  value,
  onChange,
  t,
}: {
  value: string;
  onChange: (value: string) => void;
  t: TranslateFn;
}) {
  const [expanded, setExpanded] = useState(false);
  const rules = parseCacheBreakpoints(value);
  const slots: Array<CacheBreakpointRule | null> = [
    rules[0] ?? null,
    rules[1] ?? null,
    rules[2] ?? null,
    rules[3] ?? null,
  ];

  const commit = (nextSlots: Array<CacheBreakpointRule | null>) => {
    onChange(JSON.stringify(nextSlots.filter((r): r is CacheBreakpointRule => r !== null)));
  };

  const updateSlot = (idx: number, patch: Partial<CacheBreakpointRule>) => {
    const next = [...slots];
    const current = next[idx] ?? { target: "messages", position: "nth", index: 1, ttl: "auto" };
    next[idx] = { ...current, ...patch };
    commit(next);
  };

  const clearSlot = (idx: number) => {
    const next = [...slots];
    next[idx] = null;
    commit(next);
  };

  // Example cards
  const exampleCards: Array<{ label: string; rule: CacheBreakpointRule }> = [
    { label: t("providers.cacheBreakpoints.example.topLevel"), rule: { target: "top_level", position: "nth", index: 1, ttl: "auto" } },
    { label: t("providers.cacheBreakpoints.example.systemLast"), rule: { target: "system", position: "last_nth", index: 1, ttl: "auto" } },
    { label: t("providers.cacheBreakpoints.example.messagesLast11"), rule: { target: "messages", position: "last_nth", index: 11, ttl: "auto" } },
    { label: t("providers.cacheBreakpoints.example.messagesLast1"), rule: { target: "messages", position: "last_nth", index: 1, ttl: "5m" } },
  ];

  const fillFirstEmptySlot = (rule: CacheBreakpointRule) => {
    const emptyIdx = slots.findIndex((s) => s === null);
    if (emptyIdx >= 0) {
      updateSlot(emptyIdx, rule);
    }
  };

  return (
    <CollapsibleSection
      title={t("providers.cacheBreakpoints.title")}
      summary={t("providers.cacheBreakpoints.summary", { count: rules.length })}
      expanded={expanded}
      onToggle={() => setExpanded((v) => !v)}
      expandLabel={t("common.show")}
      collapseLabel={t("providers.dispatch.collapse")}
      actions={
        <Button variant="neutral" onClick={() => commit(RECOMMENDED_CACHE_TEMPLATE)}>
          {t("providers.cacheBreakpoints.template")}
        </Button>
      }
    >
      <p className="text-xs text-muted">{t("providers.cacheBreakpoints.hint")}</p>

      {/* Example cards — click to fill first empty slot */}
      <div className="mb-3">
        <div className="mb-1.5 text-xs text-muted">{t("providers.cacheBreakpoints.examples")}</div>
        <div className="grid grid-cols-2 gap-2 xl:grid-cols-4">
          {exampleCards.map((card, i) => (
            <button
              key={i}
              type="button"
              className="rounded-lg border border-dashed border-border px-2 py-2.5 text-center text-xs font-medium text-muted transition hover:border-text hover:text-text"
              onClick={() => fillFirstEmptySlot(card.rule)}
            >
              {card.label}
            </button>
          ))}
        </div>
      </div>
      <div className="grid gap-3 sm:grid-cols-2">
        {slots.map((rule, idx) => (
          <div key={idx} className="rounded-xl border border-border bg-panel-muted px-3 py-2.5">
            <div className="mb-2 flex items-center justify-between">
              <span className="text-xs font-semibold uppercase tracking-[0.08em] text-muted">
                {t("providers.cacheBreakpoints.slot", { index: idx + 1 })}
              </span>
              {rule ? (
                <button
                  type="button"
                  className="text-xs text-muted hover:text-text"
                  onClick={() => clearSlot(idx)}
                >
                  {t("common.delete")}
                </button>
              ) : null}
            </div>
            {rule ? (
              <div className="space-y-2">
                {/* Target: segmented buttons */}
                <div className="flex flex-wrap gap-1">
                  {([
                    { value: "top_level" as const, label: t("providers.cacheBreakpoints.target.topLevel") },
                    { value: "tools" as const, label: t("providers.cacheBreakpoints.target.tools") },
                    { value: "system" as const, label: t("providers.cacheBreakpoints.target.system") },
                    { value: "messages" as const, label: t("providers.cacheBreakpoints.target.messages") },
                  ]).map((item) => (
                    <button
                      key={item.value}
                      type="button"
                      className={`btn rounded-full px-2.5 py-1 text-[11px] font-semibold transition ${
                        rule.target === item.value ? "btn-primary" : "btn-neutral"
                      }`}
                      onClick={() =>
                        updateSlot(idx, {
                          target: item.value,
                          ...(item.value === "top_level"
                            ? { position: "nth" as const, index: 1, content_position: undefined, content_index: undefined }
                            : {}),
                          ...(item.value !== "messages"
                            ? { content_position: undefined, content_index: undefined }
                            : {}),
                        })
                      }
                    >
                      {item.label}
                    </button>
                  ))}
                </div>

                {/* Position + index (non-top_level) */}
                {rule.target !== "top_level" ? (
                  <div className="flex items-center gap-1">
                    <button
                      type="button"
                      className={`btn rounded-full px-2 py-0.5 text-[11px] font-semibold transition ${
                        rule.position === "nth" ? "btn-primary" : "btn-neutral"
                      }`}
                      onClick={() => updateSlot(idx, { position: rule.position === "nth" ? "last_nth" : "nth" })}
                    >
                      {rule.position === "last_nth" ? t("providers.cacheBreakpoints.lastNth") : t("providers.cacheBreakpoints.nth")}
                    </button>
                    <Input
                      value={String(rule.index)}
                      onChange={(v) =>
                        updateSlot(idx, { index: Math.max(1, Number.parseInt(v, 10) || 1) })
                      }
                    />
                    <span className="text-[11px] text-muted">{t("providers.cacheBreakpoints.nthSuffix")}</span>
                  </div>
                ) : null}

                {/* TTL: segmented buttons */}
                <div className="flex gap-1">
                  {(["auto", "5m", "1h"] as const).map((ttl) => (
                    <button
                      key={ttl}
                      type="button"
                      className={`btn rounded-full px-2.5 py-1 text-[11px] font-semibold transition ${
                        rule.ttl === ttl ? "btn-primary" : "btn-neutral"
                      }`}
                      onClick={() => updateSlot(idx, { ttl })}
                    >
                      {ttl === "auto" ? "auto" : ttl}
                    </button>
                  ))}
                </div>
              </div>
            ) : (
              <button
                type="button"
                className="flex w-full items-center justify-center rounded-lg border border-dashed border-border py-8 text-sm text-muted transition hover:border-text hover:text-text"
                onClick={() =>
                  updateSlot(idx, { target: "messages", position: "last_nth", index: 1, ttl: "auto" })
                }
              >
                +
              </button>
            )}
          </div>
        ))}
      </div>
    </CollapsibleSection>
  );
}

// ---------------------------------------------------------------------------
// Beta Headers Editor
// ---------------------------------------------------------------------------

export function BetaHeadersEditor({
  value,
  onChange,
  isClaudeCode,
  t,
}: {
  value: string;
  onChange: (value: string) => void;
  isClaudeCode?: boolean;
  t: TranslateFn;
}) {
  const [expanded, setExpanded] = useState(false);
  const selected = parseBetaHeaders(value);

  const toggle = (beta: string) => {
    const exists = selected.some((s) => s.toLowerCase() === beta.toLowerCase());
    const next = exists
      ? selected.filter((s) => s.toLowerCase() !== beta.toLowerCase())
      : [...selected, beta];
    onChange(JSON.stringify(next));
  };

  return (
    <CollapsibleSection
      title={t("providers.betaHeaders.title")}
      summary={
        selected.length === 0
          ? t("common.none")
          : `${selected.length} beta${selected.length > 1 ? "s" : ""}`
      }
      expanded={expanded}
      onToggle={() => setExpanded((v) => !v)}
      expandLabel={t("common.show")}
      collapseLabel={t("providers.dispatch.collapse")}
      actions={
        <>
          {isClaudeCode ? (
            <span className="badge badge-accent text-[10px]">
              {CLAUDECODE_OAUTH_BETA}
            </span>
          ) : null}
          <button
            type="button"
            className="text-xs text-muted hover:text-text"
            onClick={() => onChange("[]")}
          >
            {t("providers.betaHeaders.clear")}
          </button>
        </>
      }
    >
      <p className="text-xs text-muted">{t("providers.betaHeaders.hint")}</p>
      <div className="flex flex-wrap gap-1.5">
        {ANTHROPIC_REFERENCE_BETA_HEADERS.map((beta) => {
          const active = selected.some((s) => s.toLowerCase() === beta.toLowerCase());
          return (
            <button
              key={beta}
              type="button"
              className={`btn rounded-full px-2.5 py-1 text-[11px] font-semibold transition ${
                active ? "btn-primary" : "btn-neutral"
              }`}
              onClick={() => toggle(beta)}
            >
              {beta}
            </button>
          );
        })}
      </div>
    </CollapsibleSection>
  );
}

// ---------------------------------------------------------------------------
// Prelude Text Editor
// ---------------------------------------------------------------------------

export function PreludeTextEditor({
  value,
  onChange,
  t,
}: {
  value: string;
  onChange: (value: string) => void;
  t: TranslateFn;
}) {
  const [expanded, setExpanded] = useState(false);
  const templates = [
    { key: "none", label: t("common.none"), text: "" },
    { key: "code", label: "Claude Code", text: CLAUDE_CODE_PRELUDE },
    { key: "agent", label: "Agent SDK", text: CLAUDE_AGENT_SDK_PRELUDE },
  ];

  const activeLabel = value
    ? templates.find((tmpl) => tmpl.text === value)?.label ?? `${value.length} chars`
    : t("common.none");

  return (
    <CollapsibleSection
      title={t("providers.prelude.title")}
      summary={activeLabel}
      expanded={expanded}
      onToggle={() => setExpanded((v) => !v)}
      expandLabel={t("common.show")}
      collapseLabel={t("providers.dispatch.collapse")}
    >
      <textarea
        className="textarea"
        rows={5}
        value={value}
        onChange={(e) => onChange(e.target.value)}
      />
      <div className="flex flex-wrap gap-2">
        {templates.map((tmpl) => (
          <Button
            key={tmpl.key}
            variant={value === tmpl.text ? "primary" : "neutral"}
            onClick={() => onChange(tmpl.text)}
          >
            {tmpl.label}
          </Button>
        ))}
      </div>
      <p className="text-xs text-muted">{t("providers.prelude.hint")}</p>
    </CollapsibleSection>
  );
}

// ---------------------------------------------------------------------------
// Sanitize Rules Editor
// ---------------------------------------------------------------------------

export function SanitizeRulesEditor({
  value,
  onChange,
  t,
}: {
  value: string;
  onChange: (value: string) => void;
  t: TranslateFn;
}) {
  const [expanded, setExpanded] = useState(false);
  const rules = parseSanitizeRules(value);

  const commit = (next: SanitizeRule[]) => {
    onChange(JSON.stringify(next));
  };

  const add = () => {
    commit([...rules, { pattern: "", replacement: "" }]);
    if (!expanded) {
      setExpanded(true);
    }
  };

  const remove = (idx: number) => {
    commit(rules.filter((_, i) => i !== idx));
  };

  const update = (idx: number, field: keyof SanitizeRule, val: string) => {
    const next = [...rules];
    next[idx] = { ...next[idx], [field]: val };
    commit(next);
  };

  const toggleTemplate = (templateKey: string) => {
    const template = SANITIZE_TEMPLATES.find((tmpl) => tmpl.key === templateKey);
    if (!template) return;
    const allPresent = template.rules.every((tr) =>
      rules.some((r) => r.pattern === tr.pattern && r.replacement === tr.replacement),
    );
    if (allPresent) {
      commit(
        rules.filter(
          (r) =>
            !template.rules.some(
              (tr) => tr.pattern === r.pattern && tr.replacement === r.replacement,
            ),
        ),
      );
    } else {
      const toAdd = template.rules.filter(
        (tr) => !rules.some((r) => r.pattern === tr.pattern),
      );
      commit([...rules, ...toAdd]);
    }
    if (!expanded) {
      setExpanded(true);
    }
  };

  const isTemplateActive = (templateKey: string) => {
    const template = SANITIZE_TEMPLATES.find((tmpl) => tmpl.key === templateKey);
    if (!template) return false;
    return template.rules.every((tr) =>
      rules.some((r) => r.pattern === tr.pattern && r.replacement === tr.replacement),
    );
  };

  const filledCount = rules.filter((r) => r.pattern.trim() !== "").length;

  return (
    <CollapsibleSection
      title={t("providers.sanitize.title")}
      summary={filledCount === 0 ? t("providers.sanitize.empty") : `${filledCount} rule${filledCount > 1 ? "s" : ""}`}
      expanded={expanded}
      onToggle={() => setExpanded((v) => !v)}
      expandLabel={t("common.show")}
      collapseLabel={t("providers.dispatch.collapse")}
      actions={
        <Button variant="neutral" onClick={add}>
          + {t("providers.sanitize.add")}
        </Button>
      }
    >
      <p className="text-xs text-muted">{t("providers.sanitize.hint")}</p>

      {/* Template toggle chips */}
      <div className="flex flex-wrap gap-1.5">
        {SANITIZE_TEMPLATES.map((tmpl) => (
          <button
            key={tmpl.key}
            type="button"
            className={`btn rounded-full px-2.5 py-1 text-[11px] font-semibold transition ${
              isTemplateActive(tmpl.key) ? "btn-primary" : "btn-neutral"
            }`}
            onClick={() => toggleTemplate(tmpl.key)}
          >
            {tmpl.label}
          </button>
        ))}
      </div>

      {/* Rule rows */}
      {rules.length > 0 ? (
        <div className="max-h-64 space-y-2 overflow-y-auto pr-1">
          {rules.map((rule, idx) => (
            <div
              key={idx}
              className="flex items-start gap-2 rounded-lg border border-border bg-panel-muted px-3 py-2"
            >
              <div className="grid flex-1 gap-2 sm:grid-cols-2">
                <Input
                  value={rule.pattern}
                  onChange={(v) => update(idx, "pattern", v)}
                  placeholder={t("providers.sanitize.pattern")}
                />
                <Input
                  value={rule.replacement}
                  onChange={(v) => update(idx, "replacement", v)}
                  placeholder={t("providers.sanitize.replacement")}
                />
              </div>
              <button
                type="button"
                className="mt-1.5 shrink-0 text-xs text-muted hover:text-text"
                onClick={() => remove(idx)}
              >
                ×
              </button>
            </div>
          ))}
        </div>
      ) : (
        <p className="py-4 text-center text-xs text-muted">{t("providers.sanitize.empty")}</p>
      )}
    </CollapsibleSection>
  );
}

