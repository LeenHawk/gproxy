import { useTranslation } from "react-i18next";
import { SANITIZE_TEMPLATES } from "@/lib/sanitize-templates";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";

interface SanitizeValue { pattern?: string; replacement?: string }

interface Props {
  value: SanitizeValue;
  onChange: (v: unknown) => void;
}

export function SanitizeFields({ value, onChange }: Props) {
  const { t } = useTranslation("rules");
  return (
    <div className="grid gap-3">
      <div className="flex flex-wrap items-center gap-1.5">
        <span className="text-xs text-muted-foreground">{t("sanitizeTemplate.fillFrom")}</span>
        {SANITIZE_TEMPLATES.map((tpl) => (
          <Button
            key={tpl.id}
            type="button"
            variant="outline"
            size="sm"
            className="h-6 px-2 text-xs"
            onClick={() => onChange({ pattern: tpl.pattern, replacement: tpl.replacement })}
          >
            {tpl.id}
          </Button>
        ))}
      </div>
      <div className="grid gap-1">
        <Label htmlFor="cfg-san-pattern">{t("config.pattern")}</Label>
        <Input
          id="cfg-san-pattern"
          value={value.pattern ?? ""}
          onChange={(e) => onChange({ ...value, pattern: e.target.value })}
          placeholder="sensitive_\w+"
        />
      </div>
      <div className="grid gap-1">
        <Label htmlFor="cfg-san-repl">{t("config.replacement")}</Label>
        <Input
          id="cfg-san-repl"
          value={value.replacement ?? ""}
          onChange={(e) => onChange({ ...value, replacement: e.target.value })}
          placeholder="[REDACTED]"
        />
      </div>
    </div>
  );
}
