import { useTranslation } from "react-i18next";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import {
  Select, SelectContent, SelectItem, SelectTrigger, SelectValue,
} from "@/components/ui/select";

interface HeaderValue { name?: string; value?: string; mode?: string }

interface Props {
  value: HeaderValue;
  onChange: (v: unknown) => void;
}

export function HeaderFields({ value, onChange }: Props) {
  const { t } = useTranslation("rules");
  return (
    <div className="grid gap-3">
      <div className="grid gap-1">
        <Label htmlFor="cfg-hdr-name">{t("config.name")}</Label>
        <Input
          id="cfg-hdr-name"
          value={value.name ?? ""}
          onChange={(e) => onChange({ ...value, name: e.target.value })}
          placeholder="X-Custom-Header"
        />
      </div>
      <div className="grid gap-1">
        <Label htmlFor="hdr-value">{t("config.value")}</Label>
        <Input
          id="hdr-value"
          value={value.value ?? ""}
          onChange={(e) => onChange({ ...value, value: e.target.value })}
        />
      </div>
      <div className="grid gap-1">
        <Label htmlFor="cfg-hdr-mode">{t("config.mode")}</Label>
        <Select
          value={value.mode ?? "override"}
          onValueChange={(v) => onChange({ ...value, mode: v })}
        >
          <SelectTrigger id="cfg-hdr-mode"><SelectValue /></SelectTrigger>
          <SelectContent>
            <SelectItem value="override">{t("config.modeOptions.override")}</SelectItem>
            <SelectItem value="merge">{t("config.modeOptions.merge")}</SelectItem>
          </SelectContent>
        </Select>
      </div>
    </div>
  );
}
