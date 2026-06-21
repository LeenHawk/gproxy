import { useTranslation } from "react-i18next";
import { Label } from "@/components/ui/label";
import { Textarea } from "@/components/ui/textarea";
import {
  Select, SelectContent, SelectItem, SelectTrigger, SelectValue,
} from "@/components/ui/select";

interface SystemTextValue { text?: string; position?: string }

interface Props {
  value: SystemTextValue;
  onChange: (v: unknown) => void;
}

export function SystemTextFields({ value, onChange }: Props) {
  const { t } = useTranslation("rules");
  return (
    <div className="grid gap-3">
      <div className="grid gap-1">
        <Label htmlFor="cfg-st-text">{t("config.text")}</Label>
        <Textarea
          id="cfg-st-text"
          rows={4}
          value={value.text ?? ""}
          onChange={(e) => onChange({ ...value, text: e.target.value })}
        />
      </div>
      <div className="grid gap-1">
        <Label htmlFor="cfg-st-pos">{t("config.position")}</Label>
        <Select
          value={value.position ?? "prepend"}
          onValueChange={(v) => onChange({ ...value, position: v })}
        >
          <SelectTrigger id="cfg-st-pos"><SelectValue /></SelectTrigger>
          <SelectContent>
            <SelectItem value="prepend">{t("config.positionOptions.prepend")}</SelectItem>
            <SelectItem value="append">{t("config.positionOptions.append")}</SelectItem>
          </SelectContent>
        </Select>
      </div>
    </div>
  );
}
