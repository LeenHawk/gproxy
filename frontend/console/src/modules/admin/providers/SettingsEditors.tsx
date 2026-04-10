import { Button, Input, Label, Select } from "../../../components/ui";
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
  // Always show 4 slots
  const slots: Array<CacheBreakpointRule | null> = [
    rules[0] ?? null,
    rules[1] ?? null,
    rules[2] ?? null,
    rules[3] ?? null,
  ];

  const commit = (nextSlots: Array<CacheBreakpointRule | null>) => {
    const nextRules = nextSlots.filter((r): r is CacheBreakpointRule => r !== null);
    onChange(JSON.stringify(nextRules));
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

  const applyTemplate = () => {
    commit(RECOMMENDED_CACHE_TEMPLATE);
  };

  return (
    <div className="space-y-3">
      <div className="flex flex-wrap items-center gap-2">
        <Label>{t("providers.cacheBreakpoints.title")}</Label>
        <Button variant="neutral" onClick={applyTemplate}>
          {t("providers.cacheBreakpoints.template")}
        </Button>
      </div>
      <p className="text-xs text-muted">{t("providers.cacheBreakpoints.hint")}</p>
      <div className="grid gap-2 sm:grid-cols-2">
        {slots.map((rule, idx) => (
          <div key={idx} className="rounded border border-border p-2">
            <div className="mb-2 flex items-center justify-between">
              <span className="text-xs text-muted">
                {t("providers.cacheBreakpoints.slot", { index: idx + 1 })}
              </span>
              <Button variant="neutral" onClick={() => clearSlot(idx)}>
                {t("common.delete")}
              </Button>
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
                  <div className="space-y-1">
                    <span className="text-xs text-muted">content block</span>
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
                          { value: "", label: "—" },
                          { value: "nth", label: "nth" },
                          { value: "last_nth", label: "last_nth" },
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
                  </div>
                ) : null}
                <Select
                  value={rule.ttl}
                  onChange={(v) =>
                    updateSlot(idx, { ttl: v as CacheBreakpointRule["ttl"] })
                  }
                  options={[
                    { value: "auto", label: "auto" },
                    { value: "5m", label: "5m" },
                    { value: "1h", label: "1h" },
                  ]}
                />
              </div>
            ) : (
              <button
                type="button"
                className="flex w-full items-center justify-center rounded border border-dashed border-border py-6 text-sm text-muted hover:border-text hover:text-text"
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

  const clear = () => onChange("[]");

  return (
    <div className="space-y-2">
      <div className="flex flex-wrap items-center gap-2">
        <Label>{t("providers.betaHeaders.title")}</Label>
        {isClaudeCode ? (
          <span className="rounded border border-border px-1.5 py-0.5 text-[11px] font-semibold text-muted">
            {CLAUDECODE_OAUTH_BETA} (always)
          </span>
        ) : null}
        <Button variant="neutral" onClick={clear}>
          {t("providers.betaHeaders.clear")}
        </Button>
      </div>
      <p className="text-xs text-muted">{t("providers.betaHeaders.hint")}</p>
      <div className="flex flex-wrap gap-1.5">
        {ANTHROPIC_REFERENCE_BETA_HEADERS.map((beta) => {
          const active = selected.some((s) => s.toLowerCase() === beta.toLowerCase());
          return (
            <button
              key={beta}
              type="button"
              className={`rounded border px-2 py-1 text-xs font-medium transition ${
                active
                  ? "border-accent bg-accent/10 text-text"
                  : "border-border text-muted hover:border-text hover:text-text"
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
    { key: "none", label: t("common.none"), value: "" },
    { key: "code", label: "Claude Code", value: CLAUDE_CODE_PRELUDE },
    { key: "agent", label: "Agent SDK", value: CLAUDE_AGENT_SDK_PRELUDE },
  ];

  return (
    <div className="space-y-2">
      <Label>{t("providers.prelude.title")}</Label>
      <textarea
        className="textarea"
        rows={4}
        value={value}
        onChange={(e) => onChange(e.target.value)}
      />
      <div className="flex flex-wrap gap-2">
        {templates.map((tmpl) => (
          <Button
            key={tmpl.key}
            variant={value === tmpl.value ? "primary" : "neutral"}
            onClick={() => onChange(tmpl.value)}
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
// Sanitize Rules Editor — add/remove {pattern, replacement} rows
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
    onChange(JSON.stringify(next, null, 2));
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
    const template = SANITIZE_TEMPLATES.find((t) => t.key === templateKey);
    if (!template) return;
    // Check if all rules from this template are already present
    const allPresent = template.rules.every((tr) =>
      rules.some((r) => r.pattern === tr.pattern && r.replacement === tr.replacement),
    );
    if (allPresent) {
      // Remove template rules
      commit(
        rules.filter(
          (r) =>
            !template.rules.some(
              (tr) => tr.pattern === r.pattern && tr.replacement === r.replacement,
            ),
        ),
      );
    } else {
      // Add template rules (dedup)
      const toAdd = template.rules.filter(
        (tr) => !rules.some((r) => r.pattern === tr.pattern),
      );
      commit([...rules, ...toAdd]);
    }
  };

  const isTemplateActive = (templateKey: string) => {
    const template = SANITIZE_TEMPLATES.find((t) => t.key === templateKey);
    if (!template) return false;
    return template.rules.every((tr) =>
      rules.some((r) => r.pattern === tr.pattern && r.replacement === tr.replacement),
    );
  };

  return (
    <div className="space-y-2">
      <div className="flex flex-wrap items-center gap-2">
        <Label>{t("providers.sanitize.title")}</Label>
        <Button variant="neutral" onClick={add}>
          {t("providers.sanitize.add")}
        </Button>
      </div>
      <p className="text-xs text-muted">{t("providers.sanitize.hint")}</p>
      <div className="flex flex-wrap gap-1.5">
        {SANITIZE_TEMPLATES.map((tmpl) => (
          <button
            key={tmpl.key}
            type="button"
            className={`rounded border px-2 py-1 text-xs font-medium transition ${
              isTemplateActive(tmpl.key)
                ? "border-accent bg-accent/10 text-text"
                : "border-border text-muted hover:border-text hover:text-text"
            }`}
            onClick={() => toggleTemplate(tmpl.key)}
          >
            {tmpl.label}
          </button>
        ))}
      </div>
      {rules.length === 0 ? (
        <p className="text-xs text-muted">{t("providers.sanitize.empty")}</p>
      ) : (
        <div className="space-y-2">
          {rules.map((rule, idx) => (
            <div key={idx} className="flex items-start gap-2">
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
              <Button variant="danger" onClick={() => remove(idx)}>
                ×
              </Button>
            </div>
          ))}
        </div>
      )}
    </div>
  );
}
