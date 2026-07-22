import { useState } from "react";
import { useTranslation } from "react-i18next";
import { Plus, X } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Switch } from "@/components/ui/switch";
import { VariantPresetPicker } from "@/components/providers/variant-preset-picker";
import type { SuffixAction } from "@/components/providers/suffix-presets";

export interface VariantRow {
  name: string;
  actions: SuffixAction[];
  /** true when behavior was (re)set this session; false for untouched existing variants. */
  touched: boolean;
}

export function VariantEditor({
  rows, exposeBase, modelId, channel, behaviorsLoading, onChange, onExposeBaseChange,
}: {
  rows: VariantRow[];
  exposeBase: boolean;
  modelId: string;
  channel: string;
  behaviorsLoading: boolean;
  onChange: (rows: VariantRow[]) => void;
  onExposeBaseChange: (v: boolean) => void;
}) {
  const { t } = useTranslation("providers");
  const [pickerRow, setPickerRow] = useState<number | null>(null);

  const setName = (i: number, name: string) =>
    onChange(rows.map((r, idx) => (idx === i ? { ...r, name, touched: true } : r)));
  const remove = (i: number) => onChange(rows.filter((_, idx) => idx !== i));
  const add = () => onChange([...rows, { name: "", actions: [], touched: false }]);
  const setBehavior = (i: number, actions: SuffixAction[], suggestedSuffix: string) => {
    onChange(rows.map((r, idx) => {
      if (idx !== i) return r;
      const name = r.name.trim() === "" ? `${modelId}${suggestedSuffix}` : r.name;
      return { ...r, name, actions, touched: true };
    }));
    setPickerRow(null);
  };

  return (
    <fieldset className="grid gap-3 rounded-md border p-3">
      <legend className="px-1 text-sm font-medium">{t("models.variants")}</legend>
      {rows.length === 0 && <p className="text-xs text-muted-foreground">{t("models.variantsEmpty")}</p>}
      {rows.map((r, i) => (
        <div key={i} className="grid gap-1 rounded border bg-muted/20 p-2">
          <div className="flex items-center gap-2">
            <Input
              aria-label={t("models.variantName")}
              className="font-mono text-xs"
              value={r.name}
              placeholder="gpt-image-2"
              disabled={behaviorsLoading}
              onChange={(e) => setName(i, e.target.value)}
            />
            <Button type="button" variant="ghost" size="icon" disabled={behaviorsLoading} aria-label={t("models.variantRemove")} onClick={() => remove(i)}>
              <X className="size-4" />
            </Button>
          </div>
          <div className="flex items-center justify-between text-xs">
            <span className="text-muted-foreground">
              {behaviorsLoading
                ? t("models.variantBehaviorLoading")
                : (r.actions.length > 0 ? r.actions.map((a) => a.path).join(", ") : t("models.variantNoBehavior"))}
            </span>
            <Button type="button" variant="outline" size="sm" disabled={behaviorsLoading} onClick={() => setPickerRow(i)}>
              {t("models.variantSetBehavior")}
            </Button>
          </div>
          {pickerRow === i && (
            <VariantPresetPicker
              modelId={modelId.trim()}
              channel={channel}
              initialActions={r.actions}
              onCancel={() => setPickerRow(null)}
              onConfirm={(actions, suggestedSuffix) => setBehavior(i, actions, suggestedSuffix)}
            />
          )}
        </div>
      ))}
      <Button type="button" variant="outline" size="sm" className="justify-self-start" disabled={behaviorsLoading} onClick={add}>
        <Plus className="size-4" />
        {t("models.addVariant")}
      </Button>
      <label className="flex items-center gap-2 text-sm">
        <Switch checked={exposeBase} disabled={behaviorsLoading} onCheckedChange={onExposeBaseChange} />
        {t("models.exposeBase")}
      </label>
      <p className="text-xs text-muted-foreground">{t("models.variantsHint")}</p>
    </fieldset>
  );
}
