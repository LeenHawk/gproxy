import { useState } from "react";
import { useTranslation } from "react-i18next";
import { JsonField, parseJsonText } from "@/components/form/json-field";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import {
  Select, SelectContent, SelectItem, SelectTrigger, SelectValue,
} from "@/components/ui/select";

type ValType = "string" | "number" | "boolean" | "null" | "json";

function detect(v: unknown): ValType {
  if (v === null || v === undefined) return "null";
  if (typeof v === "string") return "string";
  if (typeof v === "number") return "number";
  if (typeof v === "boolean") return "boolean";
  return "json";
}

interface Props { value: unknown; onChange: (v: unknown) => void; onValidChange?: (ok: boolean) => void }

export function RewriteValueField({ value, onChange, onValidChange }: Props) {
  const { t } = useTranslation("rules");
  const [type, setType] = useState<ValType>(detect(value));
  const [jsonText, setJsonText] = useState(
    type === "json" ? JSON.stringify(value ?? {}, null, 2) : "",
  );

  const changeType = (next: ValType) => {
    setType(next);
    onValidChange?.(true);
    if (next === "string") onChange(typeof value === "string" ? value : "");
    else if (next === "number") onChange(typeof value === "number" ? value : 0);
    else if (next === "boolean") onChange(typeof value === "boolean" ? value : false);
    else if (next === "null") onChange(null);
    else { setJsonText(JSON.stringify(value ?? {}, null, 2)); }
  };

  return (
    <div className="grid gap-2">
      <div className="grid gap-1">
        <Label>{t("rewriteValue.typeLabel")}</Label>
        <Select value={type} onValueChange={(v) => changeType(v as ValType)}>
          <SelectTrigger><SelectValue /></SelectTrigger>
          <SelectContent>
            {(["string", "number", "boolean", "null", "json"] as ValType[]).map((tt) => (
              <SelectItem key={tt} value={tt}>{t(`rewriteValue.type.${tt}`)}</SelectItem>
            ))}
          </SelectContent>
        </Select>
      </div>

      {type === "string" && (
        <Input value={String(value ?? "")} onChange={(e) => onChange(e.target.value)} />
      )}
      {type === "number" && (
        <Input type="number" value={typeof value === "number" ? value : ""}
          onChange={(e) => onChange(e.target.value === "" ? 0 : Number(e.target.value))} />
      )}
      {type === "boolean" && (
        <Select value={String(value === true)} onValueChange={(v) => onChange(v === "true")}>
          <SelectTrigger><SelectValue /></SelectTrigger>
          <SelectContent>
            <SelectItem value="true">true</SelectItem>
            <SelectItem value="false">false</SelectItem>
          </SelectContent>
        </Select>
      )}
      {type === "null" && <p className="text-sm text-muted-foreground">null</p>}
      {type === "json" && (
        <div className="grid gap-1">
          <JsonField value={jsonText} rows={4} onChange={(text) => {
            setJsonText(text);
            const p = parseJsonText(text);
            onValidChange?.(p.ok);
            if (p.ok) onChange(p.value);
          }} />
          <p className="text-xs text-muted-foreground">{t("rewriteValue.type.json")}</p>
        </div>
      )}
    </div>
  );
}
