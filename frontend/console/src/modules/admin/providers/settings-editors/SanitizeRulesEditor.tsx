import { useState } from "react";

import { Button, Input } from "../../../../components/ui";
import { SANITIZE_TEMPLATES, parseSanitizeRules, type SanitizeRule } from "../channel-constants";
import { CollapsibleSection, type TranslateFn } from "./CollapsibleSection";

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
      collapseLabel={t("providers.routing.collapse")}
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
