import { Button, Input, Select } from "../../../components/ui";
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

// ---------------------------------------------------------------------------
// Cache Breakpoints Editor — 4 fixed slots with selects
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

  return (
    <div className="card-shell space-y-3">
      <div className="flex flex-wrap items-center justify-between gap-2">
        <h3 className="text-sm font-semibold text-text">{t("providers.cacheBreakpoints.title")}</h3>
        <Button variant="neutral" onClick={() => commit(RECOMMENDED_CACHE_TEMPLATE)}>
          {t("providers.cacheBreakpoints.template")}
        </Button>
      </div>
      <p className="text-xs text-muted">{t("providers.cacheBreakpoints.hint")}</p>
      <div className="grid gap-3 sm:grid-cols-2">
        {slots.map((rule, idx) => (
          <div
            key={idx}
            className="rounded-xl border border-border bg-panel-muted px-3 py-2.5"
          >
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
                <Select
                  value={rule.target}
                  onChange={(v) =>
                    updateSlot(idx, {
                      target: v as CacheBreakpointRule["target"],
                      ...(v === "top_level"
                        ? { position: "nth" as const, index: 1, content_position: undefined, content_index: undefined }
                        : {}),
                      ...(v !== "messages"
                        ? { content_position: undefined, content_index: undefined }
                        : {}),
                    })
                  }
                  options={[
                    { value: "top_level", label: "top_level" },
                    { value: "tools", label: "tools" },
                    { value: "system", label: "system" },
                    { value: "messages", label: "messages" },
                  ]}
                />
                {rule.target !== "top_level" ? (
                  <div className="grid grid-cols-2 gap-2">
                    <Select
                      value={rule.position}
                      onChange={(v) =>
                        updateSlot(idx, { position: v as CacheBreakpointRule["position"] })
                      }
                      options={[
                        { value: "nth", label: "nth" },
                        { value: "last_nth", label: "last_nth" },
                      ]}
                    />
                    <Input
                      value={String(rule.index)}
                      onChange={(v) =>
                        updateSlot(idx, { index: Math.max(1, Number.parseInt(v, 10) || 1) })
                      }
                    />
                  </div>
                ) : null}
                {rule.target === "messages" ? (
                  <div className="grid grid-cols-2 gap-2">
                    <Select
                      value={rule.content_position ?? ""}
                      onChange={(v) =>
                        updateSlot(idx, {
                          content_position: v ? (v as CacheBreakpointRule["position"]) : undefined,
                          content_index: v ? (rule.content_index ?? 1) : undefined,
                        })
                      }
                      options={[
                        { value: "", label: "— content —" },
                        { value: "nth", label: "content nth" },
                        { value: "last_nth", label: "content last_nth" },
                      ]}
                    />
                    {rule.content_position ? (
                      <Input
                        value={String(rule.content_index ?? 1)}
                        onChange={(v) =>
                          updateSlot(idx, {
                            content_index: Math.max(1, Number.parseInt(v, 10) || 1),
                          })
                        }
                      />
                    ) : null}
                  </div>
                ) : null}
                <Select
                  value={rule.ttl}
                  onChange={(v) => updateSlot(idx, { ttl: v as CacheBreakpointRule["ttl"] })}
                  options={[
                    { value: "auto", label: "auto (ephemeral)" },
                    { value: "5m", label: "5 min" },
                    { value: "1h", label: "1 hour" },
                  ]}
                />
              </div>
            ) : (
              <button
                type="button"
                className="flex w-full items-center justify-center rounded-lg border border-dashed border-border py-8 text-sm text-muted transition hover:border-text hover:text-text"
                onClick={() =>
                  updateSlot(idx, { target: "messages", position: "last_nth", index: 1, ttl: "auto" })
                }
              >
                + {t("providers.sanitize.add")}
              </button>
            )}
          </div>
        ))}
      </div>
    </div>
  );
}

// ---------------------------------------------------------------------------
// Beta Headers Editor — toggle button chips
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
  const selected = parseBetaHeaders(value);

  const toggle = (beta: string) => {
    const exists = selected.some((s) => s.toLowerCase() === beta.toLowerCase());
    const next = exists
      ? selected.filter((s) => s.toLowerCase() !== beta.toLowerCase())
      : [...selected, beta];
    onChange(JSON.stringify(next));
  };

  return (
    <div className="card-shell space-y-3">
      <div className="flex flex-wrap items-center justify-between gap-2">
        <h3 className="text-sm font-semibold text-text">{t("providers.betaHeaders.title")}</h3>
        <div className="flex items-center gap-2">
          {isClaudeCode ? (
            <span className="rounded-full border border-border px-2 py-0.5 text-[10px] font-bold uppercase tracking-wider text-muted">
              {CLAUDECODE_OAUTH_BETA} always
            </span>
          ) : null}
          <button
            type="button"
            className="text-xs text-muted hover:text-text"
            onClick={() => onChange("[]")}
          >
            {t("providers.betaHeaders.clear")}
          </button>
        </div>
      </div>
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
    </div>
  );
}

// ---------------------------------------------------------------------------
// Prelude Text Editor — textarea + template buttons
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
  const templates = [
    { key: "none", label: t("common.none"), text: "" },
    { key: "code", label: "Claude Code", text: CLAUDE_CODE_PRELUDE },
    { key: "agent", label: "Agent SDK", text: CLAUDE_AGENT_SDK_PRELUDE },
  ];

  return (
    <div className="card-shell space-y-3">
      <h3 className="text-sm font-semibold text-text">{t("providers.prelude.title")}</h3>
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
    </div>
  );
}

// ---------------------------------------------------------------------------
// Sanitize Rules Editor — template toggles + custom rule rows
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
  const rules = parseSanitizeRules(value);

  const commit = (next: SanitizeRule[]) => {
    onChange(JSON.stringify(next));
  };

  const add = () => {
    commit([...rules, { pattern: "", replacement: "" }]);
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
  };

  const isTemplateActive = (templateKey: string) => {
    const template = SANITIZE_TEMPLATES.find((tmpl) => tmpl.key === templateKey);
    if (!template) return false;
    return template.rules.every((tr) =>
      rules.some((r) => r.pattern === tr.pattern && r.replacement === tr.replacement),
    );
  };

  return (
    <div className="card-shell space-y-3">
      <div className="flex flex-wrap items-center justify-between gap-2">
        <h3 className="text-sm font-semibold text-text">{t("providers.sanitize.title")}</h3>
        <Button variant="neutral" onClick={add}>
          + {t("providers.sanitize.add")}
        </Button>
      </div>
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
            {tmpl.label} ({tmpl.rules.length})
          </button>
        ))}
      </div>

      {/* Rule rows */}
      {rules.length > 0 ? (
        <div className="space-y-2">
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
    </div>
  );
}
