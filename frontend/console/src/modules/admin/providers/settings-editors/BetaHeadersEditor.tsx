import { useState } from "react";

import {
  ANTHROPIC_REFERENCE_BETA_HEADERS,
  CLAUDECODE_OAUTH_BETA,
  parseBetaHeaders,
} from "../channel-constants";
import { CollapsibleSection, type TranslateFn } from "./CollapsibleSection";

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
