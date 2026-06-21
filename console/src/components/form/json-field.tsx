import { useState } from "react";
import { useTranslation } from "react-i18next";
import { Button } from "@/components/ui/button";
import { Textarea } from "@/components/ui/textarea";
import { cn } from "@/lib/utils";

export function parseJsonText(text: string): { ok: true; value: unknown } | { ok: false } {
  try {
    return { ok: true, value: JSON.parse(text) };
  } catch {
    return { ok: false };
  }
}

interface JsonFieldProps {
  id?: string;
  value: string;
  onChange: (text: string) => void;
  rows?: number;
  placeholder?: string;
  hint?: string;
}

/** JSON textarea with parse validation on blur and a pretty-print button. */
export function JsonField({ id, value, onChange, rows = 6, placeholder, hint }: JsonFieldProps) {
  const { t } = useTranslation("common");
  const [touched, setTouched] = useState(false);
  const invalid = touched && value.trim() !== "" && !parseJsonText(value).ok;
  const describedBy = id ? `${id}-msg` : undefined;
  return (
    <div className="grid gap-1">
      <Textarea
        id={id}
        value={value}
        rows={rows}
        placeholder={placeholder}
        spellCheck={false}
        aria-invalid={invalid}
        aria-describedby={describedBy}
        className={cn("max-h-[50vh] overflow-y-auto font-mono text-xs", invalid && "border-destructive")}
        onChange={(e) => onChange(e.target.value)}
        onBlur={() => setTouched(true)}
      />
      <div className="flex items-start justify-between gap-2">
        <p id={describedBy} className={cn("text-xs", invalid ? "text-destructive" : "text-muted-foreground")}>
          {invalid ? t("json.invalid") : (hint ?? "")}
        </p>
        <Button
          type="button"
          variant="ghost"
          size="sm"
          className="shrink-0"
          onClick={() => {
            const parsed = parseJsonText(value);
            if (parsed.ok) onChange(JSON.stringify(parsed.value, null, 2));
          }}
        >
          {t("json.format")}
        </Button>
      </div>
    </div>
  );
}
