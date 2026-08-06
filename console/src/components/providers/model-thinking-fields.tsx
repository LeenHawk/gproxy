import { useTranslation } from "react-i18next";
import { Label } from "@/components/ui/label";
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select";

type Props = {
  supported: boolean | null;
  adaptive: boolean | null;
  enabled: boolean | null;
  onSupportedChange: (value: boolean | null) => void;
  onAdaptiveChange: (value: boolean | null) => void;
  onEnabledChange: (value: boolean | null) => void;
};

function wireValue(value: boolean | null): string {
  return value == null ? "unknown" : String(value);
}

function boolValue(value: string): boolean | null {
  return value === "unknown" ? null : value === "true";
}

export function ModelThinkingFields(props: Props) {
  const { t } = useTranslation("providers");
  const fields = [
    ["md-thinking", "models.thinkingSupported", props.supported, props.onSupportedChange],
    ["md-thinking-adaptive", "models.thinkingAdaptive", props.adaptive, props.onAdaptiveChange],
    ["md-thinking-enabled", "models.thinkingEnabled", props.enabled, props.onEnabledChange],
  ] as const;

  return (
    <>
      <div className="grid gap-3 sm:grid-cols-3">
        {fields.map(([id, label, value, onChange]) => (
          <div key={id} className="grid gap-2">
            <Label htmlFor={id}>{t(label)}</Label>
            <Select value={wireValue(value)} onValueChange={(next) => onChange(boolValue(next))}>
              <SelectTrigger id={id}><SelectValue /></SelectTrigger>
              <SelectContent>
                <SelectItem value="unknown">{t("models.thinkingStates.unknown")}</SelectItem>
                <SelectItem value="true">{t("models.thinkingStates.supported")}</SelectItem>
                <SelectItem value="false">{t("models.thinkingStates.unsupported")}</SelectItem>
              </SelectContent>
            </Select>
          </div>
        ))}
      </div>
      <p className="text-xs text-muted-foreground">{t("models.thinkingHint")}</p>
    </>
  );
}
