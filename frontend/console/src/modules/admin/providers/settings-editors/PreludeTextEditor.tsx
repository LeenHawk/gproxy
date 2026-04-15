import { useState } from "react";

import { Button } from "../../../../components/ui";
import { CLAUDE_AGENT_SDK_PRELUDE, CLAUDE_CODE_PRELUDE } from "../channel-constants";
import { CollapsibleSection, type TranslateFn } from "./CollapsibleSection";

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
