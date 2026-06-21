import { useEffect } from "react";
import { useTranslation } from "react-i18next";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import {
  Select, SelectContent, SelectItem, SelectTrigger, SelectValue,
} from "@/components/ui/select";
import { RewriteValueField } from "./rewrite-value";

interface RewriteValue { path?: string; action?: string; value_json?: unknown }

interface Props {
  value: RewriteValue;
  onChange: (v: unknown) => void;
  onValidChange?: (valid: boolean) => void;
}

export function RewriteFields({ value, onChange, onValidChange }: Props) {
  const { t } = useTranslation("rules");
  const action = value.action ?? "set";
  const showValue = action === "set" || action === "merge";

  // Reset when action changes to delete
  useEffect(() => {
    if (!showValue) {
      onValidChange?.(true);
      // Drop value_json from the parent config when switching to delete
      const { value_json: _v, ...rest } = value;
      onChange({ ...rest });
    }
    // Only trigger on showValue change (driven by action)
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [showValue]);

  return (
    <div className="grid gap-3">
      <div className="grid gap-1">
        <Label htmlFor="cfg-rw-path">{t("config.path")}</Label>
        <Input
          id="cfg-rw-path"
          value={value.path ?? ""}
          onChange={(e) => onChange({ ...value, path: e.target.value })}
          placeholder="temperature  ·  stream_options.include_usage"
        />
      </div>
      <div className="grid gap-1">
        <Label htmlFor="cfg-rw-action">{t("config.action")}</Label>
        <Select
          value={action}
          onValueChange={(v) => onChange({ ...value, action: v })}
        >
          <SelectTrigger id="cfg-rw-action"><SelectValue /></SelectTrigger>
          <SelectContent>
            <SelectItem value="set">{t("config.actionOptions.set")}</SelectItem>
            <SelectItem value="delete">{t("config.actionOptions.delete")}</SelectItem>
            <SelectItem value="merge">{t("config.actionOptions.merge")}</SelectItem>
          </SelectContent>
        </Select>
      </div>
      {showValue && (
        <div className="grid gap-1">
          <Label>{t("config.value")}</Label>
          <RewriteValueField
            value={value.value_json}
            onChange={(vj) => onChange({ ...value, value_json: vj })}
            onValidChange={onValidChange}
          />
        </div>
      )}
    </div>
  );
}
