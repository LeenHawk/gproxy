import { useTranslation } from "react-i18next";
import { Label } from "@/components/ui/label";
import { Switch } from "@/components/ui/switch";

export function ModelPullPriceOption({
  checked,
  disabled,
  onCheckedChange,
}: {
  checked: boolean;
  disabled: boolean;
  onCheckedChange: (checked: boolean) => void;
}) {
  const { t } = useTranslation("providers");
  return (
    <div className="flex items-center justify-between gap-3 rounded-md border px-3 py-2">
      <div className="grid gap-0.5">
        <Label htmlFor="pull-default-prices">{t("models.pullDefaultPrices")}</Label>
        <p className="text-xs text-muted-foreground">{t("models.pullDefaultPricesHint")}</p>
      </div>
      <Switch
        id="pull-default-prices"
        checked={checked}
        onCheckedChange={onCheckedChange}
        disabled={disabled}
      />
    </div>
  );
}
