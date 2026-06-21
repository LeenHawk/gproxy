import { useState } from "react";
import { useTranslation } from "react-i18next";
import { ChevronsUpDown } from "lucide-react";
import { JsonField, parseJsonText } from "@/components/form/json-field";
import { Collapsible, CollapsibleContent, CollapsibleTrigger } from "@/components/ui/collapsible";
import { SystemTextFields } from "./config/system-text";
import { RewriteFields } from "./config/rewrite";
import { SanitizeFields } from "./config/sanitize";
import { CacheBreakpointFields } from "./config/cache-breakpoint";
import { HeaderFields } from "./config/header";

interface Props {
  kind: string;
  value: unknown;
  onChange: (v: unknown) => void;
  onValidChange?: (valid: boolean) => void;
}

export function RuleConfigFields({ kind, value, onChange, onValidChange }: Props) {
  const { t } = useTranslation("rules");
  const [rawText, setRawText] = useState<string>(() => JSON.stringify(value ?? {}, null, 2));
  const [rawValid, setRawValid] = useState(true);
  const [open, setOpen] = useState(false);

  const handleRawChange = (text: string) => {
    setRawText(text);
    const parsed = parseJsonText(text);
    const ok = parsed.ok;
    setRawValid(ok);
    onValidChange?.(ok);
    if (ok) onChange(parsed.value);
  };

  const handleOpenChange = (next: boolean) => {
    if (next) {
      setRawText(JSON.stringify(value ?? {}, null, 2));
      setRawValid(true);
    } else {
      // Structured editor is source of truth when closing — parent value is always valid.
      setRawValid(true);
      onValidChange?.(true);
    }
    setOpen(next);
  };

  const v = (value ?? {}) as Record<string, unknown>;

  return (
    <div className="grid gap-3">
      {kind === "system_text" && <SystemTextFields value={v} onChange={onChange} />}
      {kind === "rewrite" && <RewriteFields value={v} onChange={onChange} onValidChange={onValidChange} />}
      {kind === "sanitize" && <SanitizeFields value={v} onChange={onChange} />}
      {kind === "cache_breakpoint" && <CacheBreakpointFields value={v} onChange={onChange} />}
      {kind === "header" && <HeaderFields value={v} onChange={onChange} />}

      <Collapsible open={open} onOpenChange={handleOpenChange}>
        <CollapsibleTrigger className="flex items-center gap-1 text-xs text-muted-foreground hover:text-foreground">
          <ChevronsUpDown className="size-3" aria-hidden /> {t("advanced")}
        </CollapsibleTrigger>
        <CollapsibleContent className="pt-2">
          <JsonField value={rawText} onChange={handleRawChange} rows={8} />
          {!rawValid && <p className="text-xs text-destructive">{t("rule.rawJsonError")}</p>}
        </CollapsibleContent>
      </Collapsible>
    </div>
  );
}
